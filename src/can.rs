use embedded_io::Write;
use ra4m1::CAN0;

use embedded_can::{ExtendedId, Id, StandardId};

use crate::interrupts::{Binding, Handler, clear_interrupt, map_and_enable_interrupt};

trait Instance {
    fn peripheral() -> *const ra4m1::can0::RegisterBlock;
}

impl Instance for ra4m1::CAN0 {
    fn peripheral() -> *const ra4m1::can0::RegisterBlock {
        // Return the pointer to the CAN0 peripheral
        CAN0::ptr()
    }
}

/// Triggers on transmission of a frame.
pub struct TxHandler<I: Instance> {
    _phantom: core::marker::PhantomData<I>,
}

impl<I: Instance> Handler for TxHandler<I> {
    unsafe fn on_interrupt(interrupt: ra4m1::Interrupt) {
        // led
        let p = unsafe { ra4m1::Peripherals::steal() };
        p.PORT1.podr().modify(|_, w| w.bits(1 << 11)); // Set P111 high
        clear_interrupt(interrupt);
        // Get access to can registers
        let can = unsafe { &*I::peripheral() };
        // save msmr state
        let msmr = can.msmr.read().bits();
        // Search for transmit
        can.msmr.write(|w| w.mbsm()._01());
        // get mailbox
        let mailbox = can.mssr.read().bits() as usize;
        // check there is one
        if mailbox < 32 {
            // Clear the mailbox status
            can.mctl_tx()[mailbox].write(|w| unsafe { w.bits(0) });
            can.mctl_tx()[mailbox].write(|w| unsafe { w.bits(0) });
        }
        // Restore msmr state
        can.msmr.write(|w| unsafe { w.bits(msmr) });
    }
}

/// Frame that matches the layout of the CAN mailbox registers.
///
/// Each mailbox is 16 bytes, with the first 4 bytes being the ID register,
/// byte 4 is unused, byte 5 is the DLC register, bytes 6-13 are the data registers
/// and the last 2 bytes are the timestamp registers.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    id: MailboxId,
    dlc: u8,       // Data Length Code, 0-8 bytes
    data: [u8; 8], // Data bytes, 0-8 bytes
    ts: u16,
}

impl embedded_can::Frame for Frame {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        // Create a new Frame with the given ID and data
        if data.len() > 8 {
            return None; // Invalid data length, must be 0-8 bytes
        }
        let id: Id = id.into();
        Some(Self {
            id: MailboxId::from(id),
            dlc: data.len() as u8,
            data: {
                let mut arr = [0; 8];
                arr[..data.len()].copy_from_slice(data);
                arr
            },
            ts: 0, // Timestamp is not used here
        })
    }

    fn new_remote(id: impl Into<Id>, dlc: usize) -> Option<Self> {
        // Create a new Frame with the given ID and data length code (DLC)
        if dlc > 8 {
            return None; // Invalid DLC, must be 0-8
        }
        let id: Id = id.into();
        Some(Self {
            id: MailboxId::from(id),
            dlc: dlc as u8,
            data: [0; 8], // Initialize data to zero
            ts: 0,        // Timestamp is not used here
        })
    }

    fn is_extended(&self) -> bool {
        self.id.is_extended()
    }

    fn is_remote_frame(&self) -> bool {
        self.id.RTR()
    }

    fn id(&self) -> Id {
        self.id.into()
    }

    fn dlc(&self) -> usize {
        self.dlc as usize
    }

    fn data(&self) -> &[u8] {
        // Return a slice of the data array, up to the length specified by dlc
        &self.data[..self.dlc as usize]
    }
}

/// Mailbox ID structure that matches the layout of the CAN mailbox ID registers.
///
/// Used in frame and for configuration of mailboxes.
///
/// On Construction the IDE bit is set based on the ID type.
/// The library will clear it if not in mixed mode.
#[bitfield_struct::bitfield(u32, order = Msb)]
struct MailboxId {
    #[bits(1, default = false)]
    IDE: bool,
    #[bits(1, default = false)]
    RTR: bool,
    _reserved: bool,
    #[bits(11, default = 0)]
    SID: u16,
    #[bits(18, default = 0)]
    EID: u32,
}

impl MailboxId {
    fn is_extended(&self) -> bool {
        // Check if the IDE bit is set, indicating an extended ID
        self.IDE() || self.EID() != 0
    }
}

impl From<Id> for MailboxId {
    fn from(id: Id) -> Self {
        // Convert the ID to the register value where the lower 18 bits are used for the extended ID
        // and the upper 11 bits are used for the standard ID.
        match id {
            Id::Standard(standard_id) => Self::new().with_SID(standard_id.as_raw()),
            Id::Extended(extended_id) => {
                // Get upper 18 bits of the extended ID
                let eid = extended_id.as_raw() >> 11;
                // Get 11 bits of the standard ID
                let sid = extended_id.standard_id().as_raw() as u32;
                Self::new()
                    .with_IDE(true) // Set the IDE bit for extended IDs
                    .with_SID(sid as u16)
                    .with_EID(eid)
            }
        }
    }
}

impl From<MailboxId> for Id {
    fn from(mailbox_id: MailboxId) -> Self {
        // Extract extended ID bits
        let eid = mailbox_id.EID();
        if mailbox_id.IDE() || eid != 0 {
            Id::Extended(unsafe {
                ExtendedId::new_unchecked((eid << 11) | (mailbox_id.SID() as u32))
            })
        } else {
            // Standard ID, must be less than 0x7FF (11 bits)
            Id::Standard(unsafe { StandardId::new_unchecked(mailbox_id.SID()) })
        }
    }
}

#[derive(Clone, Copy)]
enum MailboxMode {
    Tx(MailboxTxConfig),
    Rx(MailboxRxConfig),
}

impl MailboxMode {
    fn interrupt(&self) -> bool {
        match self {
            MailboxMode::Tx(config) => config.interrupt,
            MailboxMode::Rx(config) => config.interrupt,
        }
    }
}

#[derive(Clone, Copy)]
struct MailboxRxConfig {
    // Enable interrupts for receiving messages
    interrupt: bool,
    // one-shot mode, not supported yet
    one_shot: bool,
    // If mask is valid, the mailbox will only receive messages that
    // match the corresponding mask. (floor(id/4))
    // If invalid, a message must match the RX ID exactly.
    // Used to set bit in MKIVLR
    mask_valid: bool,
    // Id that is used for filtering
    id: Id,
}

#[derive(Clone, Copy)]
struct MailboxTxConfig {
    // Enable interrupts for transmission complete
    interrupt: bool,
    // One-shot mode, not supported yet
    one_shot: bool,
}

#[derive(Clone, Copy)]
struct Mask {
    id: Id,
}

impl Mask {
    pub fn accept_all() -> Self {
        // Create a mask that accepts all messages
        Mask {
            id: Id::Standard(StandardId::ZERO), // Standard ID 0 will match all messages
        }
    }

    fn mkr(&self) -> u32 {
        // Generate the Mailbox Mask Register (MKR) value
        // based on the mask ID. Remove IDE as not used in masks.
        MailboxId::from(self.id).with_IDE(false).into_bits()
    }
}

/// Mailbox and masking configuration for the peripheral
///
/// Contains 8 masks and 32 mailboxes.
/// Mask 0 is used for mailboxes 0-3, mask 1 for mailboxes 4-7, and so on.
pub struct MailboxConfig {
    masks: [Mask; 8],
    mailboxes: [MailboxMode; 32],
}

impl Default for MailboxConfig {
    fn default() -> Self {
        // Create a default configuration with all mailboxes configured for transmission
        MailboxConfig {
            masks: [Mask::accept_all(); 8],
            mailboxes: [MailboxMode::Tx(MailboxTxConfig {
                interrupt: false,
                one_shot: false,
            }); 32],
        }
    }
}

impl MailboxConfig {
    pub fn set_mailbox_receiver(&mut self, index: usize) {
        // Set the mailbox at the given index to receive mode
        if index < 32 {
            self.mailboxes[index] = MailboxMode::Rx(MailboxRxConfig {
                interrupt: false,
                one_shot: false,
                mask_valid: true,                   // Default to valid mask
                id: Id::Standard(StandardId::ZERO), // Default ID, will be set later
            });
        }
    }

    pub fn enable_all_interrupts(&mut self) {
        // Enable interrupts for all mailboxes
        for mailbox in &mut self.mailboxes {
            match mailbox {
                MailboxMode::Tx(config) => config.interrupt = true,
                MailboxMode::Rx(config) => config.interrupt = true,
            }
        }
    }

    fn mier(&self) -> u32 {
        // Generate the Mailbox Interrupt Enable Register (MIER) value
        // based on the mailbox configuration.
        let mut mier = 0;
        for (i, mailbox) in self.mailboxes.iter().enumerate() {
            if mailbox.interrupt() {
                mier |= 1 << i;
            }
        }
        mier
    }

    fn mkivlr(&self) -> u32 {
        // Generate the Mailbox Mask Invalid Register (MKIVLR) value
        // based on the mask configuration.
        let mut mkivlr = 0;
        // iterate mailboxes
        for (i, mailbox) in self.mailboxes.iter().enumerate() {
            // Get the mask index for the mailbox if it is a receive mailbox
            if let MailboxMode::Rx(config) = mailbox {
                // set bit to 1 if the mask is invalid
                if !config.mask_valid {
                    mkivlr |= 1 << i;
                }
            }
        }
        mkivlr
    }
}

// Get a ptr to the mailbox ID register of mailbox `index`
// ## Safety
// The caller must ensure that `index` is within the range of 0 to 31
unsafe fn mb_id(can0: &CAN0, index: usize) -> *mut u32 {
    let base = can0.mb0_id.as_ptr();
    // Calculate the address of the mailbox ID register
    unsafe { base.add(4 * index) }
}

// Get a ptr to the first mailbox DLC register if mailbox `index`
// ## Safety
// The caller must ensure that `index` is within the range of 0 to 31
unsafe fn mb_dl(can0: &CAN0, index: usize) -> *mut u8 {
    let base = can0.mb0_id.as_ptr() as *mut u8;
    // Based on Table 30.4 in section 30.2.6 Mailbox Register
    unsafe { base.add((16 * index) + 5) }
}

// Get a ptr to the first mailbox data register if mailbox `index`
// ## Safety
// The caller must ensure that `index` is within the range of 0 to 31
unsafe fn mb_d0(can0: &CAN0, index: usize) -> *mut u8 {
    // Get a ptr to the base of the mailbox data registers
    let base = can0.mb0_id.as_ptr() as *mut u8;
    // Based on Table 30.4 in section 30.2.6 Mailbox Register
    unsafe { base.add((16 * index) + 6) }
}

/// Layout of the Bit Configuration Register (BCR)
#[bitfield_struct::bitfield(u32)]
pub struct BitConfig {
    #[allow(non_snake_case)]
    CCLKS: bool,
    #[bits(7)]
    _reserved: u8,
    #[bits(3)]
    #[allow(non_snake_case)]
    TSEG2: u8,
    _reserved: bool,
    #[bits(2)]
    #[allow(non_snake_case)]
    SJW: u8,
    #[bits(2)]
    _reserved: u8,
    #[bits(10)]
    #[allow(non_snake_case)]
    BRP: u16,
    #[bits(2)]
    _reserved: u8,
    #[bits(4)]
    #[allow(non_snake_case)]
    TSEG1: u8,
}

impl BitConfig {
    /// Create a new Config with checked and converted values.
    /// cclks is false if the clock is PCLKB, true if it is CANMCLK.
    /// brp_scale is the prescaler value from the clock source to 1 TQ. (1-> 1024)
    /// sjw_tq, tseg1_tq, and tseg2_yq are in units of time quanta (TQ).
    /// sjw 1 - 4 TQ, tseg1 4 - 16 TQ, tseg2 2 - 8 TQ.
    pub const fn new_checked(
        cclks: bool,
        brp_scale: u16,
        tseg1_tq: u8,
        tseg2_tq: u8,
        sjw_tq: u8,
    ) -> Option<Self> {
        // Check if the values are within the valid ranges
        if brp_scale > 1024
            || brp_scale == 0
            || tseg1_tq < 4
            || tseg1_tq > 16
            || tseg2_tq < 2
            || tseg2_tq > 8
            || sjw_tq < 1
            || sjw_tq > 4
        {
            return None; // Invalid configuration
        }
        Some(
            Self::new()
                .with_CCLKS(cclks)
                .with_BRP(brp_scale - 1)
                .with_SJW(sjw_tq - 1)
                .with_TSEG1(tseg1_tq - 1)
                .with_TSEG2(tseg2_tq - 1),
        )
    }
}

enum CanMode {
    Sleep,
    Reset,
    Halt,
    Operation,
    BusOff,
}

pub struct Can {
    reg: CAN0,
}

impl Can {
    /// Create a new CAN interface with the given CAN0 peripheral and bit configuration.
    ///
    /// Will enter reset mode, configure the peripheral, then go to halt mode ready
    /// for mailbox configuration.
    pub fn new<IRQ>(can: CAN0, bit_config: BitConfig, irq: IRQ) -> Self
    where
        IRQ: Binding<TxHandler<ra4m1::CAN0>>,
    {
        // TX pin is D4 / p103
        // RX pin is D5 / p102
        let p = unsafe { ra4m1::Peripherals::steal() };

        // Enable and map interrupts
        map_and_enable_interrupt(<IRQ as Binding<TxHandler<ra4m1::CAN0>>>::interrupt(), 0x4E);

        // Set the pins for CAN0

        // For some reason access to the PSEL bits can't be done in PAC
        // Get access to them with ptr offsets from p100
        let p100: *mut u32 = p.PFS.p100pfs().as_ptr() as *mut u32; // PSEL for P102 is at offset 2, P103 is at offset 3

        unsafe {
            let p102 = p100.add(2);
            let p103 = p100.add(3);
            let rx_bits = 0b10000 << 24; // PSEL for CAN0 RX
            let tx_bits = 0b10000 << 24; // PSEL for CAN0 TX
            // clear first
            p102.write_volatile(0); // PSEL for CAN0 RX
            p103.write_volatile(0); // PSEL for CAN0 TX
            p102.write_volatile(rx_bits); // PSEL for CAN0 RX
            p103.write_volatile(tx_bits); // PSEL for CAN0 TX
            p102.write_volatile(rx_bits | (1 << 16)); // Now with PMR = 1
            p103.write_volatile(tx_bits | (1 << 16)); // Now with PMR = 1
        }

        // Ensure that the can module is enabled
        p.MSTP.mstpcrb.modify(|_, w| {
            // Enable CAN0
            w.mstpb2()._0()
        });

        let can = Can { reg: can };

        // After MCU reset CAN is in sleep mode.
        // Go to reset mode by setting CANM to 01
        // when the SLPM bit is 0.
        // Will reset from any mode
        can.go_to_mode(CanMode::Reset);

        // Wait for STR.RSTST to go to 1
        while can.reg.str.read().rstst().bit_is_clear() {}

        // Set the bit configuration register (BCR)
        p.CAN0
            .bcr
            .write(|w| unsafe { w.bits(bit_config.into_bits()) });

        // Go to halt mode
        can.go_to_mode(CanMode::Halt);
        can
    }

    // Write the mode bits to the control register
    // Does not check current mode
    fn go_to_mode(&self, mode: CanMode) {
        // Set the CAN mode
        match mode {
            CanMode::Sleep => {
                self.reg.ctlr.modify(|_, w| w.slpm()._1()); // Set sleep mode
            }
            CanMode::Reset => {
                self.reg.ctlr.modify(|_, w| {
                    w.slpm()
                        ._0() // Not in sleep mode
                        .canm()
                        ._01() // Reset mode
                });
            }
            CanMode::Halt => {
                self.reg.ctlr.modify(|_, w| w.canm()._10().slpm()._0()); // Halt mode
            }
            CanMode::Operation => {
                self.reg.ctlr.modify(|_, w| w.canm()._00().slpm()._0()); // Operation mode
            }
            CanMode::BusOff => {
                // Not implemented, bus off is a state that can be entered by the hardware
            }
        }
    }

    pub fn configure_mailboxes(&mut self, config: MailboxConfig) {
        // Must be in halt mode to configure mailboxes and masks
        self.go_to_mode(CanMode::Halt);

        for (i, mask) in config.masks.iter().enumerate() {
            // Write to the mkr register
            self.reg.mkr[i].write(|w| unsafe { w.bits(mask.mkr()) });
        }
        // Write to the MIER register
        self.reg.mier().write(|w| unsafe { w.bits(config.mier()) });
        // Write to the MKIVLR register
        self.reg
            .mkivlr
            .write(|w| unsafe { w.bits(config.mkivlr()) });
        // The PAC does not provide access to the mailbox registers by index,
        // the numbers are part of the register name.
        // Each mailbox is 16 bytes

        // Configure each mailbox depending on its mode
        for (i, mailbox) in config.mailboxes.iter().enumerate() {
            // Clear first, twice because some bits can't be cleared at the same time
            self.reg.mctl_tx()[i].write(|w| unsafe { w.bits(0) });
            self.reg.mctl_rx()[i].write(|w| unsafe { w.bits(0) });
            match mailbox {
                MailboxMode::Tx(_) => {
                    // Just leave at 0, one-shot mode is not supported yet
                }
                MailboxMode::Rx(config) => {
                    // Enable the RECREQ bit for the mailbox
                    self.reg.mctl_rx()[i].modify(|_, w| {
                        w.recreq()._1() // Enable receive request
                    });
                    // Turn the ID into a register value
                    let mut id = MailboxId::from(config.id);
                    // Clear IDE bit if not in mixed mode
                    self.configure_ide_bit(&mut id);

                    // Write the ID to the mailbox ID register
                    unsafe {
                        mb_id(&self.reg, i).write_volatile(id.into_bits()); // Write the ID to the mailbox ID register
                    }
                }
            }
        }
    }

    /// Clears IDE bit if not in mixed mode.
    #[inline(always)]
    fn configure_ide_bit(&self, id: &mut MailboxId) {
        // Clear IDE bit if not in mixed mode
        if self.reg.ctlr.read().idfm().variant() != ra4m1::can0::ctlr::IDFM_A::_10 {
            id.set_IDE(false);
        }
    }

    pub fn internal_self_test(&self) {
        self.go_to_mode(CanMode::Halt);
        self.reg.tcr.write(|w| w.tste()._1().tstm()._11());
    }

    pub fn external_self_test(&self) {
        self.go_to_mode(CanMode::Halt);
        self.reg.tcr.write(|w| w.tste()._1().tstm()._10());
    }

    pub fn listen_only_mode(&self) {
        // Set the listen-only mode
        self.go_to_mode(CanMode::Halt);
        self.reg.tcr.write(|w| {
            w.tste()
                ._1() // Enable test mode
                .tstm()
                ._01() // Listen-only mode
        });
    }

    pub fn disable_test_mode(&self) {
        // Disable test mode
        self.go_to_mode(CanMode::Halt);
        self.reg.tcr.write(|w| w.tste()._0().tstm()._00());
    }

    pub fn start(&self) {
        // Go to operation mode
        self.go_to_mode(CanMode::Operation);
        // reset the timer
        self.reg.ctlr.modify(|_, w| w.tsrc()._1()); // Reset timer
    }

    pub fn send_frame(&self, frame: Frame) -> Result<(), ()> {
        // Find the first available mailbox for transmission
        for i in 0..32 {
            let r = self.reg.mctl_tx()[i].read();
            // Check if the mailbox is available for transmission
            if r.trmreq().bit_is_clear() && r.recreq().bit_is_clear() {
                {
                    // Write the ID to the mailbox ID register
                    unsafe {
                        mb_id(&self.reg, i).write_volatile(frame.id.into_bits());
                    }
                    // write the dlc
                    unsafe {
                        mb_dl(&self.reg, i).write_volatile(frame.dlc);
                    }
                    // Write the data to the mailbox data registers
                    let data_ptr = unsafe { mb_d0(&self.reg, i) };
                    for (j, &byte) in <Frame as embedded_can::Frame>::data(&frame)
                        .iter()
                        .enumerate()
                    {
                        unsafe {
                            data_ptr.add(j).write_volatile(byte);
                        }
                    }
                    // Put mailbox id into first byte
                    // unsafe { data_ptr.write_volatile(i as u8) };
                    // Request transmission
                    self.reg.mctl_tx()[i].write(|w| w.trmreq()._1());
                    return Ok(()); // Exit after sending the frame
                }
            }
        }
        Err(())
    }

    pub fn try_receive_frame(&self) -> Option<Frame> {
        // Check each mailbox for received frames
        for i in 0..32 {
            let r = self.reg.mctl_rx()[i].read();
            // Check if the mailbox has a received frame
            if r.newdata().bit_is_set() && r.trmreq().bit_is_clear() {
                // clear register
                self.reg.mctl_rx()[i].write(|w| unsafe {
                    w.bits(0) // Clear the mailbox control register
                });
                // Read the ID from the mailbox ID register
                let id = unsafe { mb_id(&self.reg, i).read_volatile() };
                let id = MailboxId::from_bits(id);
                // Read the DLC
                let dlc = unsafe { mb_dl(&self.reg, i).read_volatile() };
                // Read the data from the mailbox data registers
                let mut data = [0; 8];
                let data_ptr = unsafe { mb_d0(&self.reg, i) };
                for (j, b) in data[..(dlc as usize)].iter_mut().enumerate() {
                    *b = unsafe { data_ptr.add(j).read_volatile() };
                }
                // Go back to ready state
                self.reg.mctl_rx()[i].write(|w| w.recreq()._1()); // Clear the receive request
                return Some(Frame {
                    id,
                    dlc,
                    data,
                    ts: 0, // Timestamp is not used here
                });
            }
        }
        None // No frame received
    }
}

pub fn init(tx: &mut impl Write) {
    tx.write_all("\nInitialising CAN\n".as_bytes()).unwrap();
    // TX pin is D4 / p103
    // RX pin is D5 / p102
    let p = unsafe { ra4m1::Peripherals::steal() };

    tx.write_fmt(format_args!(
        "P102: {:08X}\n",
        p.PFS.p102pfs_ha().read().bits()
    ))
    .unwrap();

    tx.write_fmt(format_args!(
        "P103: {:08X}\n",
        p.PFS.p103pfs_ha().read().bits()
    ))
    .unwrap();

    // Set the pins for CAN0
    // For some reason access to the PSEL bits can't be done in PAC
    // Get access to them with ptr offsets from p100
    let p100: *mut u32 = (p.PFS.p100pfs().as_ptr() as *mut u32); // PSEL for P102 is at offset 2, P103 is at offset 3
    // print pointer address
    tx.write_fmt(format_args!("P100 ptr: {:08X}\n", p100 as usize))
        .unwrap();

    unsafe {
        let p102 = p100.add(2);
        let p103 = p100.add(3);
        // print the pointer addresses
        tx.write_fmt(format_args!("P102 ptr: {:08X}\n", p102 as usize))
            .unwrap();
        tx.write_fmt(format_args!("P103 ptr: {:08X}\n", p103 as usize))
            .unwrap();
        let rx_bits = 0b10000 << 24; // PSEL for CAN0 RX
        let tx_bits = (0b10000 << 24); // PSEL for CAN0 TX
        // clear first
        p102.write_volatile(0); // PSEL for CAN0 RX
        p103.write_volatile(0); // PSEL for CAN0 TX
        p102.write_volatile(rx_bits); // PSEL for CAN0 RX
        p103.write_volatile(tx_bits); // PSEL for CAN0 TX
        p102.write_volatile(rx_bits | (1 << 16)); // Now with PMR = 1
        p103.write_volatile(tx_bits | (1 << 16)); // Now with PMR = 1
    }

    // read back 16 bits registers to see if anything happened
    tx.write_fmt(format_args!(
        "P102 16: {:08X}\n",
        p.PFS.p102pfs_ha().read().bits()
    ))
    .unwrap();

    tx.write_fmt(format_args!(
        "P103 16: {:08X}\n",
        p.PFS.p103pfs_ha().read().bits()
    ))
    .unwrap();

    // Ensure that the can module is enabled
    p.MSTP.mstpcrb.modify(|_, w| {
        // Enable CAN0
        w.mstpb2()._0()
    });

    status(tx);

    tx.write_all("Entering CAN reset mode...\n".as_bytes())
        .unwrap();
    // After reset CAN is in sleep mode.
    // Go to reset mode by setting CANM to 01
    // when the SLPM bit is 0.
    p.CAN0.ctlr.modify(|_, w| {
        w.slpm()
            ._0() // Not in sleep mode
            .canm()
            ._01() // Reset mode
    });

    status(tx);

    // Wait for STR.RSTST to go to 1
    while p.CAN0.str.read().rstst().bit_is_clear() {}

    // By default CAN runs from PCLKB, which is set to 24 MHz (im pretty sure)
    // The prescaler value in BCR determines the time quanta of the CAN bus.
    // The baud rate is PCLKB / (prescaler * time quanta). where time quanta is the
    // sum of the SS, TSEG1, and TSEG2 values.
    // Aim for sample point at 75% of the bit time. (TSEG1->TSEG2 boundary)
    // SS is always 1, TSEG1 of 5 and TSEG2 of 2 gives a total of 8 time quanta.
    // TSEGx must be larger than SJW, which can be 1.
    p.CAN0.bcr.modify(|_, w| {
        // Set the prescaler 2, (24 / (2 + 1) = 8 MHz)
        // 8 / (tq = 8) = 1 MHz
        unsafe { w.brp().bits(2).sjw()._00().tseg1()._0100().tseg2()._001() }
    });

    tx.write_fmt(format_args!("CAN0 BCR: {:08X}\n", p.CAN0.bcr.read().bits()))
        .unwrap();

    status(tx);

    // Got to halt mode
    tx.write_all("Entering CAN halt mode...\n".as_bytes())
        .unwrap();

    p.CAN0.ctlr.modify(|_, w| w.canm()._10());

    status(tx);

    tx.write_all("Enabling Test mode loopback...\n".as_bytes())
        .unwrap();
    // p.CAN0.tcr.write(|w| w.tste()._1().tstm()._10());

    status(tx);

    // Go to operation mode
    tx.write_all("Entering CAN operation mode...\n".as_bytes())
        .unwrap();
    p.CAN0.ctlr.modify(|_, w| {
        w.canm()._00() // Operation mode
    });

    status(tx);

    tx.write_all("Clearing Errors...\n".as_bytes()).unwrap();
    // clear bus lock flag like a savage
    p.CAN0.eifr.modify(|_, w| {
        w.blif()._0() // Clear bus lock flag
    });

    status(tx);

    tx.write_all("Resetting CAN timer...\n".as_bytes()).unwrap();
    // Rest timer as per docs
    p.CAN0.ctlr.modify(|_, w| w.tsrc()._1()); // Reset timer
    status(tx);

    // wait a bit for the bus lock flag to maybe come back
    tx.write_all("Waiting...\n".as_bytes()).unwrap();
    cortex_m::asm::delay(1_000_000);
    status(tx);

    // Write some data into the mailbox i guess
    p.CAN0.mb0_d0.write(|w| unsafe { w.data0().bits(0x55) });
    p.CAN0.mb0_dl.write(|w| unsafe { w.dlc().bits(1) });

    //
    p.CAN0.mb0_id.write(|w| unsafe { w.sid().bits(0x0) });

    p.CAN0.mctl_tx()[0].write(|w| w.trmreq()._1());

    // Loop through IDS
    for i in 0..1 {
        // Wait for sent data flag to be set
        while p.CAN0.mctl_tx()[0].read().sentdata().bit_is_clear() {
            // Wait for the transmission to complete
        }
        // Clear the register
        p.CAN0.mctl_tx()[0].modify(|_, w| unsafe { w.bits(0) });
        // Set ID
        p.CAN0.mb0_id.write(|w| unsafe { w.sid().bits(i) });
        // Trigger the transmission
        p.CAN0.mctl_tx()[0].write(|w| w.trmreq()._1());

        status(tx);
    }
}

fn status(tx: &mut impl Write) {
    // Print the STR reg
    let p = unsafe { ra4m1::Peripherals::steal() };
    tx.write_all("State\n".as_bytes()).unwrap();
    tx.write_fmt(format_args!("CAN0 STR: {:08X}\n", p.CAN0.str.read().bits()))
        .unwrap();
    // Print EIFR reg
    tx.write_fmt(format_args!(
        "CAN0 EIFR: {:08X}\n",
        p.CAN0.eifr.read().bits()
    ))
    .unwrap();
    // Print ctlr
    tx.write_fmt(format_args!(
        "CAN0 CTLR: {:08X}\n",
        p.CAN0.ctlr.read().bits()
    ))
    .unwrap();
}
