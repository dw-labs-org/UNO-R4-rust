use embassy_hal_internal::atomic_ring_buffer::RingBuffer;
use ra4m1::{SCI2, sci2};

use crate::interrupts::{Binding, Handler};

/// An SCI UART instance.
pub trait Instance {
    // Get access to the peripheral's register block.
    fn peripheral() -> *const sci2::RegisterBlock;
    fn state() -> &'static State;
    // Event ID of first event in this instance (RXI)
    fn event_base() -> u8;
}

pub struct TXI_Handler<T: Instance> {
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Instance> Handler for TXI_Handler<T> {
    unsafe fn on_interrupt(interrupt: ra4m1::Interrupt) {
        let sci = unsafe { &*T::peripheral() };
        // clear the interrupt flag
        let p = unsafe { ra4m1::Peripherals::steal() };
        p.ICU.ielsr[interrupt as usize].modify(|_, w| w.ir()._0());
        // Grab a byte from the transmit buffer
        let state = T::state();
        let mut reader = unsafe { state.tx_buf.reader() };

        let data = reader.pop_slice();

        if !data.is_empty() {
            // Write the byte to the transmit data register
            sci.tdr.write(|w| unsafe { w.bits(data[0]) });
            // Inform the reader that we popped a byte
            reader.pop_done(1);
            // Check the buffer len here not the reader slice as the
            // reader slice may be a single byte at the end of the buffer
            if state.tx_buf.is_empty() {
                // Sent byte but trigger TEI next
                sci.scr().modify(|_, w| w.teie()._1().tie()._0());
            }
        } else {
            // This shouldnt happen, but if it does, disable the TX interrupts
            sci.scr().modify(|_, w| w.tie()._0().teie()._0().te()._0());
        }
    }
}

pub struct TEI_Handler<T: Instance> {
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Instance> Handler for TEI_Handler<T> {
    unsafe fn on_interrupt(interrupt: ra4m1::Interrupt) {
        // Clear the interrupt flag
        let p = unsafe { ra4m1::Peripherals::steal() };
        p.ICU.ielsr[interrupt as usize].modify(|_, w| w.ir()._0());
        // Disable the TEI and TX interrupts and end transmission
        let sci = unsafe { &*T::peripheral() };
        sci.scr().modify(|_, w| w.teie()._0().tie()._0().te()._0());
    }
}

pub struct RXI_Handler<T: Instance> {
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Instance> Handler for RXI_Handler<T> {
    unsafe fn on_interrupt(interrupt: ra4m1::Interrupt) {
        // Clear the interrupt flag
        let p = unsafe { ra4m1::Peripherals::steal() };
        p.ICU.ielsr[interrupt as usize].modify(|_, w| w.ir()._0());
        // Get data, do stuff
        let sci = unsafe { &*T::peripheral() };
        let state = T::state();
        let byte = sci.rdr.read().bits();
        // Get writer for the RX buffer
        let mut writer = unsafe { state.rx_buf.writer() };
        // Try write to buffer
        // Should probably indicate the user if this fails
        // indicating a buffer overflow
        writer.push_one(byte);
    }
}

pub struct ERI_Handler<T: Instance> {
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Instance> Handler for ERI_Handler<T> {
    unsafe fn on_interrupt(interrupt: ra4m1::Interrupt) {
        // Clear the interrupt flag
        let p = unsafe { ra4m1::Peripherals::steal() };
        p.ICU.ielsr[interrupt as usize].modify(|_, w| w.ir()._0());
        // Clear error flags
        let sci = unsafe { &*T::peripheral() };
        sci.ssr().modify(|_, w| w.orer()._0().fer()._0().per()._0());
    }
}

struct State {
    tx_buf: RingBuffer,
    rx_buf: RingBuffer,
}

impl State {
    const fn new() -> Self {
        State {
            tx_buf: RingBuffer::new(),
            rx_buf: RingBuffer::new(),
        }
    }
}

unsafe impl Send for State {}
unsafe impl Sync for State {}

unsafe impl Sync for Uart<SCI2> {}
unsafe impl Send for Uart<SCI2> {}

/// Interface for UART operations.
pub struct Uart<T: Instance> {
    tx: UartTx<T>,
    rx: UartRx<T>,
}

pub struct UartTx<T: Instance> {
    state: &'static State,
    _phantom: core::marker::PhantomData<T>,
}

unsafe impl<T: Instance> Send for UartTx<T> {}
unsafe impl<T: Instance> Sync for UartTx<T> {}

pub struct UartRx<T: Instance> {
    state: &'static State,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Instance> Uart<T> {
    pub fn new<IRQ>(_instance: T, tx_buf: &mut [u8], rx_buf: &mut [u8], _irq: IRQ) -> Self
    where
        IRQ: Binding<TEI_Handler<T>>
            + Binding<TXI_Handler<T>>
            + Binding<RXI_Handler<T>>
            + Binding<ERI_Handler<T>>,
    {
        let sci = unsafe { &*T::peripheral() };
        let state = T::state();

        // Get interrupts for TXE and TEI
        let tei = <IRQ as Binding<TEI_Handler<T>>>::interrupt();
        let txi = <IRQ as Binding<TXI_Handler<T>>>::interrupt();
        let rxi = <IRQ as Binding<RXI_Handler<T>>>::interrupt();
        let eri = <IRQ as Binding<ERI_Handler<T>>>::interrupt();

        // Unmask the interrupts in the NVIC
        unsafe {
            ra4m1::NVIC::unmask(rxi);
            ra4m1::NVIC::unmask(txi);
            ra4m1::NVIC::unmask(tei);
            ra4m1::NVIC::unmask(eri);
        }
        let p = unsafe { ra4m1::Peripherals::steal() };
        // Event number of RXI
        let event_base = T::event_base();
        // Map events to interrupts
        p.ICU.ielsr[rxi as usize].write(|w| unsafe { w.iels().bits(event_base) });
        p.ICU.ielsr[txi as usize].write(|w| unsafe { w.iels().bits(event_base + 1) });
        p.ICU.ielsr[tei as usize].write(|w| unsafe { w.iels().bits(event_base + 2) });
        p.ICU.ielsr[eri as usize].write(|w| unsafe { w.iels().bits(event_base + 3) });

        // Initialise the buffers
        unsafe { state.tx_buf.init(tx_buf.as_mut_ptr(), tx_buf.len()) };
        unsafe { state.rx_buf.init(rx_buf.as_mut_ptr(), rx_buf.len()) };
        // Configure the SCI peripheral
        init(&p, sci);

        Self {
            tx: UartTx {
                state,
                _phantom: core::marker::PhantomData,
            },
            rx: UartRx {
                state,
                _phantom: core::marker::PhantomData,
            },
        }
    }

    /// Split the Uart into a transmitter and receiver.
    pub fn split(self) -> (UartTx<T>, UartRx<T>) {
        (self.tx, self.rx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

impl<T: Instance> embedded_io::ErrorType for UartTx<T> {
    type Error = Error;
}

impl<T: Instance> embedded_io::Write for UartTx<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        loop {
            let state = self.state;
            let mut writer = unsafe { state.tx_buf.writer() };
            let data = writer.push_slice();
            if !data.is_empty() {
                // Copy data to the buffer
                let len = data.len().min(buf.len());
                data[..len].copy_from_slice(&buf[..len]);
                // Inform the writer that we pushed some data
                writer.push_done(len);
                // Check if transmission is already in progress
                let sci = unsafe { &*T::peripheral() };
                let reg = sci.scr().read();
                // If te is clear, TEI has triggered and we need to start transmission
                if reg.te().bit_is_clear() {
                    sci.scr().modify(|_, w| w.tie()._1().teie()._0().te()._1());
                } else if reg.teie().bit_is_set() {
                    // final byte is in flight, wait until done then start a new transmission
                    // This can't be done in the TEI interrupt handler as it seems
                    // to cause a data race and bytes are lost.
                    loop {
                        // Wait for the TEI interrupt to be triggered
                        cortex_m::asm::wfi();
                        // Check if the TEI interrupt has been triggered
                        let reg = sci.scr().read();
                        if reg.teie().bit_is_clear() && reg.te().bit_is_clear() {
                            // TEI has been triggered, we can start a new transmission
                            break;
                        }
                    }
                    // Start transmission
                    sci.scr().modify(|_, w| w.tie()._1().teie()._0().te()._1());
                }

                // Return the number of bytes written
                return Ok(len);
            } else {
                // No space in the buffer.
                // Make sure transmission is started
                let sci = unsafe { &*T::peripheral() };
                let reg = sci.scr().read();
                if reg.te().bit_is_clear() {
                    sci.scr().modify(|_, w| w.tie()._1().teie()._0().te()._1());
                }
                // Wait for space in the buffer
                cortex_m::asm::wfi();
            }
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        loop {
            let state = self.state;
            if state.tx_buf.is_empty() {
                // Buffer is empty, we can flush
                return Ok(());
            } else {
                // Wait for the buffer to be empty
                cortex_m::asm::wfi();
            }
        }
    }
}

impl<T: Instance> embedded_io::ErrorType for Uart<T> {
    type Error = Error;
}

impl<T: Instance> embedded_io::Write for Uart<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.tx.write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.tx.flush()
    }
}

// ================ Read Traits ================
impl<T: Instance> embedded_io::ErrorType for UartRx<T> {
    type Error = Error;
}

impl<T: Instance> embedded_io::Read for UartRx<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        loop {
            let mut reader = unsafe { self.state.rx_buf.reader() };
            let data = reader.pop_slice();
            if !data.is_empty() {
                // Copy data to the buffer
                let len = data.len().min(buf.len());
                buf[..len].copy_from_slice(&data[..len]);
                // Inform the reader that we popped some data
                reader.pop_done(len);
                // Return the number of bytes read
                return Ok(len);
            } else {
                // No data in the buffer, wait for more data
                cortex_m::asm::wfi();
            }
        }
    }
}

impl<T: Instance> embedded_io::ReadReady for UartRx<T> {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.state.rx_buf.is_empty())
    }
}

impl<T: Instance> embedded_io::Read for Uart<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.rx.read(buf)
    }
}

impl<T: Instance> embedded_io::ReadReady for Uart<T> {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        self.rx.read_ready()
    }
}

impl Instance for SCI2 {
    fn peripheral() -> *const sci2::RegisterBlock {
        SCI2::ptr()
    }

    fn state() -> &'static State {
        static STATE: State = State::new();
        &STATE
    }

    fn event_base() -> u8 {
        0xA3
    }
}

fn init(p: &ra4m1::Peripherals, sci: &sci2::RegisterBlock) {
    // Enable SCI
    p.MSTP.mstpcrb.modify(|_, w| w.mstpb29()._0()); // Enable SCI2
    // Reset scr
    sci.scr().write(|w| unsafe { w.bits(0) });
    // In theory set FCR.FM to 0 but the default is 0
    // (and register isn't in PAC)
    // Set clock config to use on chip clock
    sci.scr().modify(|_, w| w.cke()._00());
    // Async mode (and others)
    sci.simr1.write(|w| w.iicm()._0());
    // Clock polarity and phase
    sci.spmr
        .write(|w| w.ckph()._0().ckpol()._0().ctse()._0().mss()._0());
    // Configure serial format
    sci.smr().write(|w| {
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
    sci.scmr.write(|w| {
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
    sci.semr.write(|w| unsafe { w.bits(0) });

    // try hit 115200 for 48Mhz clock
    sci.brr.write(|w| unsafe { w.brr().bits(12) });
    // sci.mddr

    // Set TE = 0 output level to 1
    sci.sptr.write(|w| w.spb2dt()._1().spb2io()._1());
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
    sci.scr().modify(|_, w| w.re()._1().rie()._1());
}
