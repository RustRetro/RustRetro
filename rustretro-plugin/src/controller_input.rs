use bitflags::bitflags;

bitflags! {
    #[derive(Default)]
    pub struct ControllerInput: u8 {
        const A = 0x80;
        const B = 0x40;
        const SELECT = 0x20;
        const START = 0x10;
        const UP = 0x08;
        const DOWN = 0x04;
        const LEFT = 0x02;
        const RIGHT = 0x01;
    }
}
