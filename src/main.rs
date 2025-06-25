#![no_std]
#![no_main]

use core::mem::MaybeUninit;

use ::log::info;
use embedded_io::{Read, ReadReady, Write as W};
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
    IEL6 => uart::RXI_Handler<ra4m1::SCI2>;
    IEL7 => uart::ERI_Handler<ra4m1::SCI2>;
});

#[entry]
fn main() -> ! {
    // Get access to the peripherals
    let p = unsafe { ra4m1::Peripherals::steal() };

    // Set p111 as an output
    p.PORT1.pdr().write(|w| unsafe { w.bits(1 << 11) });

    let mut tx_buf = [0u8; 64];
    let mut rx_buf = [0u8; 64];
    let uart = uart::Uart::new(p.SCI2, &mut tx_buf, &mut rx_buf, Irq);
    let (mut tx, rx) = uart.split();

    // Enable interrupts
    unsafe { cortex_m::interrupt::enable() }

    // Enable usb 3.3V to rs232 converter
    p.MSTP.mstpcrb.modify(|_, w| {
        // Enable USBFS
        w.mstpb11()._0()
    });
    p.USBFS.usbmc.write(|w| w.vdcen()._1());

    // wait for a bit to stabilize the USB power
    cortex_m::asm::delay(1_000_000);

    // can init
    can::init(&mut tx);

    // Print the clock configuration
    // let mut string = heapless::String::<256>::new();
    // // Read the clock register
    // let config = clk::Config::from_system(&p.SYSTEM);

    // writeln!(string, "{:?}", config).unwrap();
    // uart.write_all(string.as_bytes()).unwrap();
    // string.clear();
    // let mut count = 0;
    loop {
        // tx.write_all(b"Hello from RA4M1!\n").unwrap();
    }
}
