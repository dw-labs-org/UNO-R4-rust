#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _;

use cortex_m_rt::entry;

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

    // Initialise UART
    uart::init(&p);

    // Enable interrupts
    unsafe { cortex_m::interrupt::enable() }

    // Serial should be ready now
    serial_print("Hello, world!\n");

    // Print the clock configuration
    let mut string = heapless::String::<256>::new();
    // Read the clock register
    let config = clk::Config::from_system(&p.SYSTEM);

    writeln!(string, "{:?}", config).unwrap();
    serial_print(&string);
    string.clear();

    loop {
        if let Some(c) = uart::serial_read() {
            // echo the received character as integer
            let mut string = heapless::String::<8>::new();
            writeln!(string, "{}", c as u8).unwrap();
            serial_print(&string);
        }
    }
}
