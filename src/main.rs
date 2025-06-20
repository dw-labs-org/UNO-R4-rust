#![no_std]
#![no_main]

use cortex_m::asm::nop;
// pick a panicking behavior
use panic_halt as _;

use cortex_m_rt::entry;
use ra4m1::{icu::ielsr::IELS_A, interrupt};

#[entry]
fn main() -> ! {
    // Get access to the peripherals
    let p = unsafe { ra4m1::Peripherals::steal() };
    // Set p111 as an output
    p.PORT1.pdr().write(|w| unsafe { w.bits(1 << 11) });

    // // Try 302 as an output
    // p.PORT3.pdr().write(|w| unsafe { w.bits(1 << 2) });
    // // Set 302 high
    // p.PORT3.podr().write(|w| unsafe { w.bits(1 << 2) });

    // check VTOR register
    const VTOR_ADDRESS: *const u32 = 0xE000ED08 as *const u32;
    let value = unsafe { core::ptr::read_volatile(VTOR_ADDRESS) };
    if value != 0x4000 {
        p.PORT1.podr().write(|w| unsafe { w.bits(1 << 11) });
        loop {
            nop();
        }
    }

    // Configure serial

    unsafe {
        ra4m1::NVIC::unmask(ra4m1::Interrupt::IEL0);
        ra4m1::NVIC::unmask(ra4m1::Interrupt::IEL1);
    };
    unsafe { cortex_m::interrupt::enable() }
    // Enable interrupt for SCI2_TEI and SCI2_TXI
    p.ICU.ielsr[0].write(|w| unsafe { w.iels().bits(0x0A4) });
    p.ICU.ielsr[1].write(|w| unsafe { w.iels().bits(0x0A5) });

    // Enable SCI
    p.MSTP.mstpcrb.modify(|_, w| w.mstpb29()._0()); // Enable SCI2
    // Reset scr
    p.SCI2.scr().write(|w| unsafe { w.bits(0) });
    // In theory set FCR.FM to 0 but the default is 0
    // (and register isn't in PAC)
    // Set clock config to use on chip clock
    p.SCI2.scr().modify(|_, w| w.cke()._00());
    // Async mode (and others)
    p.SCI2.simr1.write(|w| w.iicm()._0());
    // Clock polarity and phase
    p.SCI2
        .spmr
        .write(|w| w.ckph()._0().ckpol()._0().ctse()._0().mss()._0());
    // Configure serial format
    p.SCI2.smr().write(|w| {
        w.cks()
            ._00() // no prescaler
            .mp()
            ._0() // no multiprocessor mode
            .stop()
            ._0() // 1 stop bit
            .pe()
            ._0() // no parity
            .chr()
            ._0() // 8-bit data
            .cm()
            ._0() // async mode
    });
    p.SCI2.scmr.write(|w| {
        w.smif()
            ._0() // no smart card interface
            .sinv()
            ._0() // no inversion
            .sdir()
            ._0() // LSB first (no affect in async non-multi)
            .chr1()
            ._1() // 8-bit data
    });
    // Defaults
    // p.SCI2.semr.write(|w| unsafe { w.bits(0) });

    // p.SCI2.brr.write(|w| w.brr().bits(value));
    // p.SCI2.mddr

    // Set TE = 0 output level to 1
    p.SCI2.sptr.write(|w| w.spb2dt()._1().spb2io()._1());
    // First write to the B0WI bit
    p.PMISC.pwpr.write(|w| w.b0wi()._0());
    // Then write to the PFSWE bit
    p.PMISC.pwpr.write(|w| w.pfswe()._1());
    // Set PFS for 302
    p.PFS.p302pfs().write(|w| unsafe { w.bits(0) });
    p.PFS.p302pfs().write(|w| w.pdr()._1()); // Set as output

    // Do the same for 301
    p.PFS.p301pfs().write(|w| unsafe { w.bits(0) });
    p.PFS.p301pfs().write(|w| w.psel().variant(0b00100));
    p.PFS.p301pfs().modify(|_, w| w.pmr()._1());

    p.SCI2
        .scr()
        .modify(|_, w| w.te()._1().tie()._1().teie()._1()); // Enable transmit and transmit interrupt

    p.PFS
        .p302pfs()
        .modify(|_, w| unsafe { w.psel().bits(0b00100) });
    p.PFS.p302pfs().modify(|_, w| w.pmr()._1()); // Set P302 as output

    // Lock register down again
    p.PMISC.pwpr.write(|w| w.pfswe()._0());
    p.PMISC.pwpr.write(|w| w.b0wi()._1());

    p.SCI2.tdr.write(|w| unsafe { w.bits(0x0A4) });

    loop {
        // Set output high
        p.PORT1.podr().write(|w| unsafe { w.bits(1 << 11) });
        // Set output low
        // p.PORT1.podr().write(|w| unsafe { w.bits(0) });
        // p.PORT1.podr().write(|w| unsafe { w.bits(0) });
        for _ in 0..10000 {
            // Wait for a bit
            nop();
        }
    }
}

#[interrupt]
unsafe fn IEL0() {
    // Interrupt for SCI2_TXI
    static mut count: u8 = 0;
    *count = count.wrapping_add(1);
    unsafe {
        ra4m1::Peripherals::steal()
            .PORT1
            .podr()
            .write(|w| w.bits(0))
    };
    // Clear the interrupt flag
    ra4m1::Peripherals::steal().ICU.ielsr[0].modify(|_, w| w.ir()._0());
    // Place data in SCI2 transmit data register
    unsafe {
        ra4m1::Peripherals::steal()
            .SCI2
            .tdr
            .write(|w| w.bits(*count))
    };
}

#[interrupt]
fn IEL1() {
    // This is the interrupt for SCI2_TEI
    // Triggers when the last byte has been transmitted
    // Clear the interrupt flag
    unsafe {
        ra4m1::Peripherals::steal().ICU.ielsr[1].modify(|_, w| w.ir()._0());
    }

    unsafe {
        ra4m1::Peripherals::steal()
            .PORT1
            .podr()
            .write(|w| w.bits(0))
    };
}
