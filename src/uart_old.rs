use core::cell::RefCell;

use critical_section::Mutex;
use ra4m1::interrupt;

// Create a buffer accessible from the interrupt handler
static TX: Mutex<RefCell<Tx>> = Mutex::new(RefCell::new(Tx::new()));
static RX: Mutex<RefCell<Rx>> = Mutex::new(RefCell::new(Rx::new()));

pub fn init(p: &ra4m1::Peripherals) {
    // Enable interrupts
    unsafe {
        ra4m1::NVIC::unmask(ra4m1::Interrupt::IEL0);
        ra4m1::NVIC::unmask(ra4m1::Interrupt::IEL1);
        ra4m1::NVIC::unmask(ra4m1::Interrupt::IEL2);
        ra4m1::NVIC::unmask(ra4m1::Interrupt::IEL3);
    };

    // Enable interrupt for SCI2_TXI, SCI2_TEI and SCI2_RXI
    p.ICU.ielsr[0].write(|w| unsafe { w.iels().bits(0xA4) });
    p.ICU.ielsr[1].write(|w| unsafe { w.iels().bits(0xA5) });
    p.ICU.ielsr[2].write(|w| unsafe { w.iels().bits(0xA3) });
    p.ICU.ielsr[3].write(|w| unsafe { w.iels().bits(0xA6) });

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
    p.SCI2.semr.write(|w| unsafe { w.bits(0) });

    // try hit 115200 for 48Mhz clock
    p.SCI2.brr.write(|w| unsafe { w.brr().bits(12) });
    // p.SCI2.mddr

    // Set TE = 0 output level to 1
    p.SCI2.sptr.write(|w| w.spb2dt()._1().spb2io()._1());
    // First write to the B0WI bit
    p.PMISC.pwpr.write(|w| w.b0wi()._0());
    // Then write to the PFSWE bit
    p.PMISC.pwpr.write(|w| w.pfswe()._1());
    // Set RX pin PSEL to 00100 (SCI2_RXD)
    p.PFS.p301pfs().write(|w| unsafe { w.bits(0) });
    p.PFS.p301pfs().write(|w| w.psel().variant(0b00100));
    p.PFS.p301pfs().modify(|_, w| w.pmr()._1());

    // TX as output high
    p.PFS.p302pfs().write(|w| unsafe { w.bits(0) });
    p.PFS.p302pfs().write(|w| w.pdr()._1().podr()._1());

    // Set P302 as TX pin
    p.PFS
        .p302pfs()
        .modify(|_, w| unsafe { w.psel().bits(0b00100) });
    p.PFS.p302pfs().modify(|_, w| w.pmr()._1());

    // Start receiving with interrupts
    p.SCI2.scr().modify(|_, w| w.re()._1().rie()._1());
}

#[interrupt]
unsafe fn IEL0() {
    // Interrupt for SCI2_TXI
    let p = unsafe { ra4m1::Peripherals::steal() };
    // Clear the interrupt flag
    p.ICU.ielsr[0].modify(|_, w| w.ir()._0());

    // Lock the buffer to get access to it
    critical_section::with(|cs| {
        let mut tx = TX.borrow(cs).borrow_mut();
        // Pop a byte from the buffer
        if let Some(value) = tx.buffer.pop_front() {
            // Write the value to the transmit data register
            p.SCI2.tdr.write(|w| unsafe { w.bits(value) });
            // check if the buffer is empty
            if tx.buffer.is_empty() {
                // Disable the transmit interrupt and enable the transmit end interrupt
                p.SCI2.scr().modify(|_, w| w.tie()._0().teie()._1());
            }
        } else {
            // No more data in the buffer, disable the transmit interrupt
            p.SCI2.scr().modify(|_, w| w.tie()._0().teie()._0());
        }
    });
}

#[interrupt]
fn IEL1() {
    // This is the interrupt for SCI2_TEI
    // Triggers when the last byte has been transmitted
    // Clear the interrupt flag
    let p = unsafe { ra4m1::Peripherals::steal() };

    p.ICU.ielsr[1].modify(|_, w| w.ir()._0());

    // Disable transmission and interrupts

    p.SCI2
        .scr()
        .modify(|_, w| w.teie()._0().tie()._0().te()._0());

    // Try start again if needed
    critical_section::with(|cs| {
        let mut tx = TX.borrow(cs).borrow_mut();
        // Start transmission if there is more data in the buffer
        tx.start_transmit();
    });
}

#[interrupt]
fn IEL2() {
    // SCI_RXI interrupt handler
    // Clear the interrupt flag
    let p = unsafe { ra4m1::Peripherals::steal() };

    p.ICU.ielsr[2].modify(|_, w| w.ir()._0());
    p.PORT1.podr().write(|w| unsafe { w.bits(0) });

    // Read the received data
    let data = p.SCI2.rdr.read().bits();
    // Put it in the RX buffer
    critical_section::with(|cs| {
        let mut rx = RX.borrow(cs).borrow_mut();
        // Try to push the data to the buffer
        if rx.buffer.try_push_back(data).is_err() {
            // Maybe should set an overrun flag here or something
        }
    });
}

#[interrupt]
fn IEL3() {
    // This is the interrupt for SCI2_ERI
    let p = unsafe { ra4m1::Peripherals::steal() };
    // Clear the interrupt flag
    p.ICU.ielsr[3].modify(|_, w| w.ir()._0());

    // Clear error flags
    p.SCI2
        .ssr()
        .modify(|_, w| w.per()._0().fer()._0().orer()._0());
}

/// Static object that holds the circular buffer
/// And ensures interrupt free manipulation of registers by existing
/// inside a mutex
struct Tx {
    buffer: circular_buffer::CircularBuffer<128, u8>,
}

impl Tx {
    const fn new() -> Self {
        Tx {
            buffer: circular_buffer::CircularBuffer::new(),
        }
    }

    // Can be called in the TEI interrupt handler if more data is available
    // in the buffer or when new data is added to the buffer
    fn start_transmit(&mut self) {
        let p = unsafe { ra4m1::Peripherals::steal() };
        p.SCI2.scr().modify(|r, w| {
            if r.tie().bit_is_set() || r.teie().bit_is_set() {
                // do nothing, transmission is already in progress
                w
            } else if !self.buffer.is_empty() {
                w.te()
                    ._1() // Enable transmission
                    .tie()
                    ._1() // Enable transmit interrupt
                    .teie()
                    ._0() // Disable transmit end interrupt
            } else {
                w
            }
        });
    }
}

struct Rx {
    buffer: circular_buffer::CircularBuffer<64, u8>,
}

impl Rx {
    const fn new() -> Self {
        Rx {
            buffer: circular_buffer::CircularBuffer::new(),
        }
    }
}

pub fn serial_print(str: &str) {
    // Convert string to bytes
    let bytes = str.as_bytes();
    // track index of bytes
    let mut index = 0;

    loop {
        // Loop until all bytes are pushed to the buffer
        let mut done = true;
        // Get access to buffer
        critical_section::with(|cs| {
            let mut tx = TX.borrow(cs).borrow_mut();
            // Loop through remaining bytes

            for (i, b) in bytes[index..].iter().enumerate() {
                // try push byte to buffer
                if tx.buffer.try_push_back(*b).is_err() {
                    // Buffer is full, exit loop to release critical section
                    // and allow the interrupt to add more data to uart
                    index += i;
                    done = false;
                    break;
                }
            }
            // Ensure that the transmit starts
            tx.start_transmit();
        });
        if done {
            // All bytes were pushed to the buffer, exit loop
            break;
        } else {
            // Not all bytes were pushed, wait for the interrupt to handle the buffer
            cortex_m::asm::wfi();
        }
    }
}

pub fn serial_read() -> Option<char> {
    // Create a string to hold the received data
    critical_section::with(|cs| {
        let mut rx = RX.borrow(cs).borrow_mut();
        // Try to pop a byte from the buffer
        rx.buffer.pop_front().map(|byte| byte as char)
    })
}
