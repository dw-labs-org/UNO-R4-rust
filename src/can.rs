pub fn init(p: &ra4m1::Peripherals) {
    // TX pin is D4 / p103
    // RX pin is D5 / p102
    // p.CAN0.ctlr.write(|w| {
    //     w.mbm()
    //         ._0() // Normal mode
    //         .idfm()
    //         ._00() // Standard ID format
    // });
}
