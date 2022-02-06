#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;

mod controller_input;
mod metadata;
mod pixel_format;

pub use bincode;
pub use controller_input::*;
pub use metadata::*;
pub use pixel_format::*;

pub trait RustretroPlugin {
    fn create_core(rom: &[u8]) -> Box<Self>;
    fn get_metadata(&self) -> Box<Metadata>;
    fn controller_input(&mut self, input: ControllerInput);
    fn clock_until_frame(&mut self) -> Vec<u8>;
}

#[macro_export]
macro_rules! rustretro_plugin_register {
    ($struct:ident) => {
        extern crate alloc as _rustretro_plugin_alloc;
        #[no_mangle]
        pub fn __create_core() -> u32 {
            let rom = unsafe {
                let mut buffer = [0u8; 4];
                let length: &[u8] = ::core::slice::from_raw_parts(0 as *const u8, 4);

                buffer.copy_from_slice(length);
                let length = u32::from_le_bytes(buffer);

                ::core::slice::from_raw_parts(4 as *const u8, length as usize)
            };

            let emulator = $struct::create_core(rom);

            ::_rustretro_plugin_alloc::boxed::Box::into_raw(emulator) as u32
        }

        #[no_mangle]
        pub unsafe fn __get_metadata(ptr: u32) {
            let emulator = &mut *(ptr as *mut $struct);

            let metadata = emulator.get_metadata();

            let data = ::rustretro_plugin::bincode::serialize(&metadata).unwrap();

            let buffer = ::core::slice::from_raw_parts_mut(0 as *mut u8, data.len() + 4);

            let length = data.len().to_le_bytes();
            buffer[0..4].copy_from_slice(&length);
            buffer[4..].copy_from_slice(&data);
        }

        #[no_mangle]
        pub unsafe fn __controller_input(ptr: u32, input: u32) {
            let emulator = &mut *(ptr as *mut $struct);

            let input = ::rustretro_plugin::ControllerInput::from_bits_truncate(input as u8);
            emulator.controller_input(input);
        }

        #[no_mangle]
        pub unsafe fn __clock_until_frame(ptr: u32) -> u32 {
            let emulator = &mut *(ptr as *mut $struct);
            let frame = emulator.clock_until_frame();

            let buffer = ::core::slice::from_raw_parts_mut(0 as *mut u8, frame.len() + 4);

            let length = frame.len().to_le_bytes();
            buffer[0..4].copy_from_slice(&length);

            ::_rustretro_plugin_alloc::boxed::Box::into_raw(frame.into_boxed_slice()) as *mut u8
                as u32
        }

        #[no_mangle]
        pub unsafe fn __free_frame(ptr: u32, length: u32) {
            let _: ::_rustretro_plugin_alloc::boxed::Box<[u8]> =
                ::_rustretro_plugin_alloc::boxed::Box::from_raw(::core::slice::from_raw_parts_mut(
                    ptr as *mut u8,
                    length as usize,
                ));
        }
    };
}
