use std::borrow::BorrowMut;

use rustretro_plugin::serde_json;
use rustretro_plugin::{ControllerInput, Metadata};
use wasmtime::*;

pub struct Runner {
    emulator_pointer: u32,

    metadata: Metadata,

    store: Store<String>,
    memory: Memory,

    wasm_controller_input: TypedFunc<(u32, u32), ()>,
    wasm_clock_until_frame: TypedFunc<u32, u32>,
    wasm_free_frame: TypedFunc<(u32, u32), ()>,
    wasm_free_emulator: TypedFunc<u32, ()>,
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.wasm_free_emulator
            .call(&mut self.store, self.emulator_pointer)
            .unwrap();
    }
}

impl Runner {
    pub fn new(core: &[u8], rom: &[u8]) -> Self {
        let engine = Engine::default();
        let module = Module::new(&engine, core).unwrap();
        let mut store = Store::new(&engine, "Rustretro Wasmtime Runner".to_string());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();

        // It isn't really clear if the default memory is called memory
        let memory = instance.get_memory(&mut store, "memory").unwrap();

        // Write a fat pointer of the rom in memory
        memory
            .write(&mut store, 0, &(rom.len() as u32).to_le_bytes())
            .unwrap();
        memory.write(&mut store, 4, rom).unwrap();

        // Instanciate the emulator
        let wasm_create_core = instance
            .get_typed_func::<(), u32, _>(&mut store, "__create_core")
            .unwrap();

        let emulator_pointer = wasm_create_core.call(&mut store, ()).unwrap();

        let wasm_get_metadata = instance
            .get_typed_func::<u32, (), _>(&mut store, "__get_metadata")
            .unwrap();

        wasm_get_metadata
            .call(&mut store, emulator_pointer)
            .unwrap();

        let metadata_bytes = read_return_vec(&memory, &mut store, 0);

        let metadata: Metadata = serde_json::from_slice(&metadata_bytes).unwrap();

        let wasm_controller_input = instance
            .get_typed_func::<(u32, u32), (), _>(&mut store, "__controller_input")
            .unwrap();
        let wasm_clock_until_frame = instance
            .get_typed_func::<u32, u32, _>(&mut store, "__clock_until_frame")
            .unwrap();

        let wasm_free_frame = instance
            .get_typed_func::<(u32, u32), (), _>(&mut store, "__free_frame")
            .unwrap();

        let wasm_free_emulator = instance
            .get_typed_func::<u32, (), _>(&mut store, "__free_emulator")
            .unwrap();

        Self {
            emulator_pointer,

            store,
            memory,

            metadata,

            wasm_controller_input,
            wasm_clock_until_frame,
            wasm_free_frame,
            wasm_free_emulator,
        }
    }

    pub fn controller_input(&mut self, input: ControllerInput) {
        let input = input.bits() as u32;
        self.wasm_controller_input
            .call(&mut self.store, (self.emulator_pointer, input))
            .unwrap();
    }

    pub fn clock_until_frame(&mut self) -> Vec<u8> {
        let ptr = self
            .wasm_clock_until_frame
            .call(&mut self.store, self.emulator_pointer)
            .unwrap();

        let mut buffer = vec![0u8; self.read_u32(0) as usize];
        self.memory
            .read(&mut self.store, ptr as usize, &mut buffer)
            .unwrap();

        self.wasm_free_frame
            .call(&mut self.store, (ptr, buffer.len() as u32))
            .unwrap();

        buffer
    }

    pub fn get_metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn read_u32(&mut self, ptr: u32) -> u32 {
        let mut buffer = [0u8; 4];
        self.memory
            .read(&mut self.store, ptr as usize, &mut buffer)
            .unwrap();

        u32::from_le_bytes(buffer)
    }
}

fn read_return_vec(memory: &Memory, store: &mut Store<String>, ptr: u32) -> Vec<u8> {
    let mut buffer = [0u8; 4];

    {
        memory
            .read(store.borrow_mut(), ptr as usize, &mut buffer)
            .unwrap();
    }

    let data_length = u32::from_le_bytes(buffer);
    let mut buffer = vec![0u8; data_length as usize];

    memory.read(store, (ptr + 4) as usize, &mut buffer).unwrap();

    buffer
}
