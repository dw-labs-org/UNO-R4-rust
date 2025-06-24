#![no_std]
#![no_main]

use core::fmt::Write;
use embedded_io::Write as W;
// pick a panicking behavior
use panic_halt as _;

use cortex_m_rt::entry;

mod can;
mod clk;
mod interrupts;
mod uart;

bind_interrupts!(struct Irq {
    IEL4 => uart::TXI_Handler<ra4m1::SCI2>;
    IEL5 => uart::TEI_Handler<ra4m1::SCI2>;
});

#[entry]
fn main() -> ! {
    // Get access to the peripherals
    let p = unsafe { ra4m1::Peripherals::steal() };

    // ra4m1::interrupt::IEL0

    // Set p111 as an output
    p.PORT1.pdr().write(|w| unsafe { w.bits(1 << 11) });

    let mut tx_buf = [0u8; 16];

    let mut uart = uart::UART::new(p.SCI2, &mut tx_buf, Irq);

    // Initialise UART
    // uart_old::init(&p);

    // Enable interrupts
    unsafe { cortex_m::interrupt::enable() }

    // Enable usb 3.3V to rs232 converter
    p.MSTP.mstpcrb.modify(|_, w| {
        // Enable USBFS and SCI2
        w.mstpb11()._0()
    });
    p.USBFS.usbmc.write(|w| w.vdcen()._1());

    // wait for a bit to stabilize the USB power
    cortex_m::asm::delay(1_000_000);

    // Serial should be ready now
    // uart.write_all("Hello, RA4M1!\n".as_bytes()).unwrap();

    // Print the clock configuration
    let mut string = heapless::String::<256>::new();
    // Read the clock register
    let config = clk::Config::from_system(&p.SYSTEM);

    writeln!(string, "{:?}", config).unwrap();
    // uart.write_all(string.as_bytes()).unwrap();
    string.clear();
    // let mut count = 0;
    for count in 0..1000000000 {
        writeln!(string, "Count: {}", count).unwrap();
        uart.write_all(string.as_bytes()).unwrap();
        // uart.write_all("1234567\n".as_bytes()).unwrap();
        string.clear();
        for _ in 0..(count * 1000) {
            cortex_m::asm::nop();
        }
    }
    loop {
        // Wait for interrupts
        cortex_m::asm::nop();
        // cortex_m::asm::wfi();
    }
}
