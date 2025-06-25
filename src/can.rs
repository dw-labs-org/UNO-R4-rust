use embedded_io::Write;

pub fn init(tx: &mut impl Write) {
    tx.write_all("\nInitialising CAN\n".as_bytes()).unwrap();
    // TX pin is D4 / p103
    // RX pin is D5 / p102
    let p = unsafe { ra4m1::Peripherals::steal() };

    tx.write_fmt(format_args!(
        "P102: {:08X}\n",
        p.PFS.p102pfs_ha().read().bits()
    ))
    .unwrap();

    tx.write_fmt(format_args!(
        "P103: {:08X}\n",
        p.PFS.p103pfs_ha().read().bits()
    ))
    .unwrap();

    // Set the pins for CAN0
    // For some reason access to the PSEL bits can't be done in PAC
    // Get access to them with ptr offsets from p100
    let p100: *mut u32 = (p.PFS.p100pfs().as_ptr() as *mut u32); // PSEL for P102 is at offset 2, P103 is at offset 3
    // print pointer address
    tx.write_fmt(format_args!("P100 ptr: {:08X}\n", p100 as usize))
        .unwrap();

    unsafe {
        let p102 = p100.add(2);
        let p103 = p100.add(3);
        // print the pointer addresses
        tx.write_fmt(format_args!("P102 ptr: {:08X}\n", p102 as usize))
            .unwrap();
        tx.write_fmt(format_args!("P103 ptr: {:08X}\n", p103 as usize))
            .unwrap();
        let rx_bits = 0b10000 << 24; // PSEL for CAN0 RX
        let tx_bits = (0b10000 << 24); // PSEL for CAN0 TX
        // clear first
        p102.write_volatile(0); // PSEL for CAN0 RX
        p103.write_volatile(0); // PSEL for CAN0 TX
        p102.write_volatile(rx_bits); // PSEL for CAN0 RX
        p103.write_volatile(tx_bits); // PSEL for CAN0 TX
        p102.write_volatile(rx_bits | (1 << 16)); // Now with PMR = 1
        p103.write_volatile(tx_bits | (1 << 16)); // Now with PMR = 1
    }

    // read back 16 bits registers to see if anything happened
    tx.write_fmt(format_args!(
        "P102 16: {:08X}\n",
        p.PFS.p102pfs_ha().read().bits()
    ))
    .unwrap();

    tx.write_fmt(format_args!(
        "P103 16: {:08X}\n",
        p.PFS.p103pfs_ha().read().bits()
    ))
    .unwrap();

    // Ensure that the can module is enabled
    p.MSTP.mstpcrb.modify(|_, w| {
        // Enable CAN0
        w.mstpb2()._0()
    });

    status(tx);

    tx.write_all("Entering CAN reset mode...\n".as_bytes())
        .unwrap();
    // After reset CAN is in sleep mode.
    // Go to reset mode by setting CANM to 01
    // when the SLPM bit is 0.
    p.CAN0.ctlr.modify(|_, w| {
        w.slpm()
            ._0() // Not in sleep mode
            .canm()
            ._01() // Reset mode
    });

    status(tx);

    // Wait for STR.RSTST to go to 1
    while p.CAN0.str.read().rstst().bit_is_clear() {}

    // By default CAN runs from PCLKB, which is set to 24 MHz (im pretty sure)
    // The prescaler value in BCR determines the time quanta of the CAN bus.
    // The baud rate is PCLKB / (prescaler * time quanta). where time quanta is the
    // sum of the SS, TSEG1, and TSEG2 values.
    // Aim for sample point at 75% of the bit time. (TSEG1->TSEG2 boundary)
    // SS is always 1, TSEG1 of 5 and TSEG2 of 2 gives a total of 8 time quanta.
    // TSEGx must be larger than SJW, which can be 1.
    p.CAN0.bcr.modify(|_, w| {
        // Set the prescaler 2, (24 / (2 + 1) = 8 MHz)
        // 8 / (tq = 8) = 1 MHz
        unsafe { w.brp().bits(2).sjw()._00().tseg1()._0100().tseg2()._001() }
    });

    tx.write_fmt(format_args!("CAN0 BCR: {:08X}\n", p.CAN0.bcr.read().bits()))
        .unwrap();

    status(tx);

    // Got to halt mode
    tx.write_all("Entering CAN halt mode...\n".as_bytes())
        .unwrap();

    p.CAN0.ctlr.modify(|_, w| w.canm()._10());

    status(tx);

    tx.write_all("Enabling Test mode loopback...\n".as_bytes())
        .unwrap();
    // p.CAN0.tcr.write(|w| w.tste()._1().tstm()._10());

    status(tx);

    // Enable the first mailbox for transmission
    // This can only be done in halt mode or operation mode
    // p.CAN0.mctl_tx()[0].write(|w| {
    //     w.sentdata()
    //         ._0()
    //         .trmabt()
    //         ._0()
    //         .trmreq()
    //         ._1()
    //         .oneshot()
    //         ._0()
    //         .recreq()
    //         ._0()
    //         .trmabt()
    //         ._0()
    // });

    tx.write_fmt(format_args!(
        "CAN0 MCTL_TX[0]: {:08X}\n",
        p.CAN0.mctl_tx()[0].read().bits()
    ))
    .unwrap();

    status(tx);

    // Go to operation mode
    tx.write_all("Entering CAN operation mode...\n".as_bytes())
        .unwrap();
    p.CAN0.ctlr.modify(|_, w| {
        w.canm()._00() // Operation mode
    });

    status(tx);

    tx.write_all("Clearing Errors...\n".as_bytes()).unwrap();
    // clear bus lock flag like a savage
    p.CAN0.eifr.modify(|_, w| {
        w.blif()._0() // Clear bus lock flag
    });

    status(tx);

    // tx.write_all("Resetting CAN timer...\n".as_bytes()).unwrap();
    // // Rest timer as per docs
    // p.CAN0.ctlr.write(|w| w.tsrc()._1()); // Reset timer
    // status(tx);

    // wait a bit for the bus lock flag to maybe come back
    tx.write_all("Waiting...\n".as_bytes()).unwrap();
    cortex_m::asm::delay(1_000_000);
    status(tx);

    // Write some data into the mailbox i guess
    p.CAN0.mb0_d0.write(|w| unsafe { w.data0().bits(0x55) });
    p.CAN0.mb0_dl.write(|w| unsafe { w.dlc().bits(1) });

    //
    p.CAN0.mb0_id.write(|w| unsafe { w.sid().bits(0x0) });

    p.CAN0.mctl_tx()[0].write(|w| w.trmreq()._1());
    loop {
        // Loop through IDS
        for i in 0..2048 {
            // Wait for sent data flag to be set
            while p.CAN0.mctl_tx()[0].read().sentdata().bit_is_clear() {
                // Wait for the transmission to complete
            }
            // Clear the register
            p.CAN0.mctl_tx()[0].modify(|_, w| unsafe { w.bits(0) });
            // Set ID
            p.CAN0.mb0_id.write(|w| unsafe { w.sid().bits(i) });
            // Trigger the transmission
            p.CAN0.mctl_tx()[0].write(|w| w.trmreq()._1());

            status(tx);
        }
    }
}

fn status(tx: &mut impl Write) {
    // Print the STR reg
    let p = unsafe { ra4m1::Peripherals::steal() };
    tx.write_all("State\n".as_bytes()).unwrap();
    tx.write_fmt(format_args!("CAN0 STR: {:08X}\n", p.CAN0.str.read().bits()))
        .unwrap();
    // Print EIFR reg
    tx.write_fmt(format_args!(
        "CAN0 EIFR: {:08X}\n",
        p.CAN0.eifr.read().bits()
    ))
    .unwrap();
    // Print ctlr
    tx.write_fmt(format_args!(
        "CAN0 CTLR: {:08X}\n",
        p.CAN0.ctlr.read().bits()
    ))
    .unwrap();
}
