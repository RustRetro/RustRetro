#![no_std]

use rustretro_plugin::*;
use nestadia::Emulator;

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;

struct NestadiaRustretro {
    emulator: Emulator,
}

#[rustretro_plugin]
impl RustretroPlugin for NestadiaRustretro {
    fn create_core(rom: &[u8]) -> Box<Self> {
        Box::new(Self {
            emulator: Emulator::new(rom, None).unwrap(),
        })
    }

    fn get_metadata(&self) -> Box<Metadata> {
        Box::new(Metadata {
            name: "Nestadia".to_string(),
            width: 256,
            height: 240,

            pixel_format: PixelFormat::RGBA,
            frames_per_seconds: 59.94f32,
        })
    }

    fn controller_input(&mut self, input: ControllerInput) {
        self.emulator.set_controller1(input.bits())
    }

    fn clock_until_frame(&mut self) -> Vec<u8> {
        let mask_reg = self.emulator.get_ppu_mask_reg();

        let frame = loop {
            if let Some(frame) = self.emulator.clock() {
                break frame;
            }
        };

        let mut buffer = [0u8; 256 * 240 * 4];

        nestadia::frame_to_rgba(mask_reg, frame, &mut buffer);

        buffer.to_vec()
    }
}
