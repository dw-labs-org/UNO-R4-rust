#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _;

use cortex_m::asm;
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    loop {
        asm::nop(); // Do nothing, just loop forever, doesnt get optimised away
    }
}
