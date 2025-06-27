#![no_std]
#![no_main]

use embedded_io::Write as _;
// pick a panicking behavior
use panic_halt as _;

#[rtic::app(
    // TODO: Replace `some_hal::pac` with the path to the PAC
    device = ra4m1,
    // TODO: Replace the `FreeInterrupt1, ...` with free interrupt vectors if software tasks are used
    // You can usually find the names of the interrupt vectors in the some_hal::pac::interrupt enum.
    dispatchers = [IEL0]
)]

mod app {

    static CAN_TICK: AtomicU32 = AtomicU32::new(0);

    use core::sync::atomic::AtomicU32;

    use cortex_m::asm::wfi;
    use embedded_io::Write as _;
    use uno_r4_rust::{bind_interrupts, can, uart};

    use rtic_monotonics::{
        fugit::Duration, rtic_time::embedded_hal::delay::DelayNs, systick::prelude::*,
    };

    systick_monotonic!(Mono, 1000);

    bind_interrupts!(struct Irq {
        IEL4 => uart::TXI_Handler<ra4m1::SCI2>;
        IEL5 => uart::TEI_Handler<ra4m1::SCI2>;
        IEL6 => uart::RXI_Handler<ra4m1::SCI2>;
        IEL7 => uart::ERI_Handler<ra4m1::SCI2>;
        IEL8 => can::TxHandler<ra4m1::CAN0>;
    });

    // Shared resources go here
    #[shared]
    struct Shared {
        uart_tx: uart::UartTx<ra4m1::SCI2>,
    }

    // Local resources go here
    #[local]
    struct Local {
        // TODO: Add resources
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Get access to the peripherals
        let p = unsafe { ra4m1::Peripherals::steal() };

        // Start monotonic
        Mono::start(cx.core.SYST, 48_000_000);

        // Set p111 as an output
        p.PORT1.pdr().write(|w| unsafe { w.bits(1 << 11) });

        let mut tx_buf = [0u8; 64];
        let mut rx_buf = [0u8; 64];
        let uart = uart::Uart::new(p.SCI2, &mut tx_buf, &mut rx_buf, Irq);
        let (mut tx, rx) = uart.split();

        // Enable usb 3.3V to rs232 converter
        p.MSTP.mstpcrb.modify(|_, w| {
            // Enable USBFS
            w.mstpb11()._0()
        });
        p.USBFS.usbmc.write(|w| w.vdcen()._1());

        // wait for a bit to stabilize the USB power
        cortex_m::asm::delay(1_000_000);

        tx.write_all("\nHello from RA4M1!\n".as_bytes()).unwrap();

        // can init
        let mut can = can::Can::new(
            p.CAN0,
            can::BitConfig::new_checked(false, 3, 5, 2, 1).unwrap(),
            Irq,
        );

        tx.write_all(b"CAN initialized\n").unwrap();

        let mut mailbox = can::MailboxConfig::default();
        mailbox.set_mailbox_receiver(0);
        mailbox.enable_all_interrupts();
        can.configure_mailboxes(mailbox);

        can.start();

        // Send a test frame
        // let test_frame = Frame::new(Id::Standard(StandardId::new(0x123).unwrap()), &[0xFF]).unwrap();
        // can.send_frame(test_frame).unwrap();

        tx.write_all(b"Ready to echo CAN frames\n").unwrap();

        task1::spawn().ok();

        (
            Shared {
                // Initialization of shared resources go here
                uart_tx: tx,
            },
            Local {
                // Initialization of local resources go here
            },
        )
    }

    // Optional idle, can be removed if not needed.
    #[idle()]
    fn idle(_cx: idle::Context) -> ! {
        // This is the idle task, it runs when no other tasks are ready to run.
        loop {
            unsafe {
                ra4m1::Peripherals::steal().PORT1.podr().write(
                    |w| w.bits(1 << 11), // Set p111 high
                );
                // wfi();
                ra4m1::Peripherals::steal().PORT1.podr().write(
                    |w| w.bits(0), // Set p111 low
                );
            }
        }
    }

    #[task(priority = 1, shared = [uart_tx])]
    async fn task1(cx: task1::Context) {
        let mut tx = cx.shared.uart_tx;
        loop {
            CAN_TICK.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            let start = Mono::now();
            tx.lock(|tx| tx.write_all("Task1\n".as_bytes()).unwrap());
            Mono::delay_until(start + 1000.millis()).await;
        }
    }
}
