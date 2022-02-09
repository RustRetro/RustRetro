#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;

mod controller_input;
mod metadata;
mod pixel_format;

pub use controller_input::*;
pub use metadata::*;
pub use pixel_format::*;
pub use rustretro_procmacro::rustretro_plugin;
pub use serde_json;

pub trait RustretroPlugin {
    fn create_core(rom: &[u8]) -> Box<Self>;
    fn get_metadata(&self) -> Box<Metadata>;
    fn controller_input(&mut self, input: ControllerInput);
    fn clock_until_frame(&mut self) -> Vec<u8>;
}
