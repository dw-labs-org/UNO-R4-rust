#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::asm::nop;
// pick a panicking behavior
use panic_halt as _;

use cortex_m_rt::entry;
use critical_section::Mutex;
use ra4m1::{can0::str, interrupt};

use core::fmt::Write;

use crate::uart::serial_print;

mod clk;
mod uart;

#[entry]
fn main() -> ! {
    // Get access to the peripherals
    let p = unsafe { ra4m1::Peripherals::steal() };

    // Set p111 as an output
    p.PORT1.pdr().write(|w| unsafe { w.bits(1 << 11) });

    // check VTOR register
    const VTOR_ADDRESS: *const u32 = 0xE000ED08 as *const u32;
    let value = unsafe { core::ptr::read_volatile(VTOR_ADDRESS) };
    if value != 0x4000 {
        p.PORT1.podr().write(|w| unsafe { w.bits(1 << 11) });
        loop {
            nop();
        }
    }

    uart::init(&p);

    // Enable interrupts
    unsafe { cortex_m::interrupt::enable() }

    // Serial should be ready now
    serial_print("Hello, world!\n");

    let mut string = heapless::String::<256>::new();

    // Read the clock register
    let config = clk::Config::from_system(&p.SYSTEM);

    writeln!(string, "{:?}", config).unwrap();
    serial_print(&string);
    string.clear();

    let reg = p.SYSTEM.prcr.read().bits();
    writeln!(string, "PRCR: 0x{:X}", reg).unwrap();
    serial_print(&string);
    string.clear();

    p.SYSTEM
        .prcr
        .write(|w| unsafe { w.prkey().bits(0xA5).prc0()._1() });
    // Try change the peripheral B clock by half
    p.SYSTEM.sckdivcr.modify(|_, w| {
        // Set the FCK divider to 2
        w.pckb()._000()
    });
    p.SYSTEM.ckocr.write(|w| w.ckoen()._1().ckodiv()._010());
    p.PFS.p109pfs().write(|w| unsafe { w.bits(0) });
    p.PFS.p109pfs().write(|w| w.psel().variant(0b01001));
    p.PFS.p109pfs().modify(|_, w| w.pmr()._1());

    let mut count = 0;
    loop {
        let mut string = heapless::String::<64>::new();
        // // Write the count to the string
        writeln!(string, "Count: {}", count).unwrap();
        serial_print(&string);
        count += 1;
        // for _ in 0..100000 {
        //     // Do some work to slow down the loop
        //     nop();
        // }

        // Turn LED on
        p.PORT1.podr().write(|w| unsafe { w.bits(1 << 11) });
        // Turn LED off
        p.PORT1.podr().write(|w| unsafe { w.bits(0) });
    }
}
