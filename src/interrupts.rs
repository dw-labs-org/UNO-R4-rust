use ra4m1::Interrupt;

#[macro_export]
macro_rules! bind_interrupts {
    ($(#[$outer:meta])* $vis:vis struct $name:ident {
        $(
            $(#[doc = $doc:literal])*
            $(#[cfg($cond_irq:meta)])?
            $irq:ident => $(
                $(#[cfg($cond_handler:meta)])?
                $handler:ty
            ),*;
        )*
    }) => {
        #[derive(Copy, Clone)]
        $(#[$outer])*
        $vis struct $name;

        $(
            #[allow(non_snake_case)]
            #[unsafe(no_mangle)]
            $(#[cfg($cond_irq)])?
            $(#[doc = $doc])*
            unsafe extern "C" fn $irq() {
                $(
                    $(#[cfg($cond_handler)])?
                    unsafe {<$handler as $crate::interrupts::Handler>::on_interrupt(ra4m1::Interrupt::$irq)};

                )*
            }

            $(#[cfg($cond_irq)])?
            $crate::bind_interrupts!(@inner
                $(
                    $(#[cfg($cond_handler)])?
                    unsafe impl $crate::interrupts::Binding<$handler> for $name {
                        fn interrupt() -> ra4m1::Interrupt {
                            ra4m1::Interrupt::$irq
                        }
                    }
                )*
            );
        )*
    };
    (@inner $($t:tt)*) => {
        $($t)*
    }
}

/// Defines a trait for handling interrupts.
///
/// The on_interrupt method is called when an interrupt occurs
/// after binding with the bind_interrupts! macro.
pub trait Handler {
    unsafe fn on_interrupt(interrupt: Interrupt);
}

/// Confirms that the Handler is bound to an interrupt
///
/// ## Safety
/// Must only be implemented using the `bind_interrupts!` macro.
pub unsafe trait Binding<H: Handler> {
    /// Get the interrupt variant (from which the index/number can be derived
    fn interrupt() -> ra4m1::Interrupt;
}

pub fn clear_interrupt(interrupt: Interrupt) {
    let p = unsafe { ra4m1::Peripherals::steal() };
    p.ICU.ielsr[interrupt as usize].modify(|_, w| w.ir().clear_bit());
}

pub fn enable_interrupt(interrupt: Interrupt) {
    // Enable the interrupt by writing to the interrupt enable register
    unsafe {
        ra4m1::NVIC::unmask(interrupt);
    }
}

pub fn disable_interrupt(interrupt: Interrupt) {
    // Disable the interrupt by writing to the interrupt disable register

    ra4m1::NVIC::mask(interrupt);
}

pub fn pend_interrupt(interrupt: Interrupt) {
    // Pend the interrupt by writing to the interrupt pend register
    ra4m1::NVIC::pend(interrupt);
}

pub fn map_interrupt(interrupt: Interrupt, event_id: u8) {
    // Map the interrupt to a specific priority
    let p = unsafe { ra4m1::Peripherals::steal() };
    p.ICU.ielsr[interrupt as usize].write(|w| unsafe { w.iels().bits(event_id) });
}

pub fn map_and_enable_interrupt(interrupt: Interrupt, event_id: u8) {
    // Map and enable the interrupt
    map_interrupt(interrupt, event_id);
    enable_interrupt(interrupt);
}
