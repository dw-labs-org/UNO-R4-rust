/// Clock config
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    // Add fields for clock configuration as needed
    // For example:
    pub iclk: u8,  // ICLK frequency
    pub fck: u8,   // FCK frequency
    pub pcka: u8,  // PCKA frequency
    pub pckb: u8,  // PCKB frequency
    pub pckc: u8,  // PCKC frequency
    pub pckd: u8,  // PCKD frequency
    pub cksel: u8, // Clock select
    pub hoco: Hoco,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hoco {
    pub hcstp: bool,
    pub hcfrq: u8,
}

impl Config {
    /// Create a new clock config
    pub fn from_system(sys: &ra4m1::SYSTEM) -> Self {
        let divcr = sys.sckdivcr.read();
        let iclk = divcr.ick().bits();
        let fck = divcr.fck().bits();
        let pcka = divcr.pcka().bits();
        let pckb = divcr.pckb().bits();
        let pckc = divcr.pckc().bits();
        let pckd = divcr.pckd().bits();

        let cksel = sys.sckscr.read().cksel().bits();
        let hoco = sys.hococr.read();
        let hcstp = hoco.hcstp().bit_is_set();
        let hcfrq = (unsafe { core::ptr::read_volatile(0x4001_E037 as *const u8) } >> 3);
        Config {
            // Set default values or read from system registers if needed
            iclk,
            fck,
            pcka,
            pckb,
            pckc,
            pckd,
            cksel,
            hoco: Hoco { hcstp, hcfrq },
        }
    }
}
