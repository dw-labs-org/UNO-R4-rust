#![no_std]
#![no_main]

use cortex_m::asm::nop;
// pick a panicking behavior
use panic_halt as _;

use cortex_m_rt::entry;

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

    loop {
        // Set output high
        p.PORT1.podr().write(|w| unsafe { w.bits(1 << 11) });
        // Set output low
        p.PORT1.podr().write(|w| unsafe { w.bits(0) });
    }
}
