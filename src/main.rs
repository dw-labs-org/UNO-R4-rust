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

    // Initialise the UART
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

    let mut count = 0;
    loop {
        let mut string = heapless::String::<64>::new();
        // // Write the count to the string
        writeln!(string, "Count: {}", count).unwrap();
        serial_print(&string);
        count += 1;

        // Turn LED on
        p.PORT1.podr().write(|w| unsafe { w.bits(1 << 11) });
        // Turn LED off
        p.PORT1.podr().write(|w| unsafe { w.bits(0) });
    }
}
