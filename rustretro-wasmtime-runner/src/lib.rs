use rustretro_plugin::serde_json;
use rustretro_plugin::{ControllerInput, Metadata};
use wasmtime::*;

pub struct Runner {
    emulator_pointer: u32,

    metadata: Metadata,

    store: Store<String>,
    memory: Memory,

    _wasm_alloc_vec: TypedFunc<u32, u32>,
    wasm_controller_input: TypedFunc<(u32, u32), ()>,
    wasm_clock_until_frame: TypedFunc<u32, u64>,
    wasm_free_vec: TypedFunc<(u32, u32), ()>,
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

        let wasm_alloc_vec = instance
            .get_typed_func::<u32, u32, _>(&mut store, "__rustretro_plugin_alloc_vec")
            .unwrap();
        let wasm_free_vec = instance
            .get_typed_func::<(u32, u32), (), _>(&mut store, "__rustretro_plugin_free_vec")
            .unwrap();

        // Copy the rom to WASM memory
        let rom_buffer = wasm_alloc_vec.call(&mut store, rom.len() as u32).unwrap();

        memory.write(&mut store, rom_buffer as usize, rom).unwrap();

        // Instanciate the emulator
        let wasm_create_core = instance
            .get_typed_func::<(u32, u32), u32, _>(&mut store, "__rustretro_plugin_create_core")
            .unwrap();

        let emulator_pointer = wasm_create_core
            .call(&mut store, (rom_buffer as u32, rom.len() as u32))
            .unwrap();

        // Free the ROM buffer
        wasm_free_vec
            .call(&mut store, (rom_buffer, rom.len() as u32))
            .unwrap();

        let wasm_get_metadata = instance
            .get_typed_func::<u32, u64, _>(&mut store, "__rustretro_plugin_get_metadata")
            .unwrap();

        let ptr = wasm_get_metadata
            .call(&mut store, emulator_pointer)
            .unwrap();

        let (ptr, length) = expand_return_pointer(ptr);

        let mut metadata_bytes = vec![0u8; length as usize];
        memory
            .read(&mut store, ptr as usize, &mut metadata_bytes)
            .unwrap();

        let metadata: Metadata = serde_json::from_slice(&metadata_bytes).unwrap();

        // Free the metadata buffer
        wasm_free_vec
            .call(&mut store, (ptr, length as u32))
            .unwrap();

        let wasm_controller_input = instance
            .get_typed_func::<(u32, u32), (), _>(&mut store, "__rustretro_plugin_controller_input")
            .unwrap();
        let wasm_clock_until_frame = instance
            .get_typed_func::<u32, u64, _>(&mut store, "__rustretro_plugin_clock_until_frame")
            .unwrap();

        let wasm_free_emulator = instance
            .get_typed_func::<u32, (), _>(&mut store, "__rustretro_plugin_free_emulator")
            .unwrap();

        Self {
            emulator_pointer,

            store,
            memory,

            metadata,

            wasm_controller_input,
            wasm_clock_until_frame,
            _wasm_alloc_vec: wasm_alloc_vec,
            wasm_free_vec,
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

        let (ptr, length) = expand_return_pointer(ptr);

        let mut buffer = vec![0u8; length as usize];
        self.memory
            .read(&mut self.store, ptr as usize, &mut buffer)
            .unwrap();

        self.wasm_free_vec
            .call(&mut self.store, (ptr, buffer.len() as u32))
            .unwrap();

        buffer
    }

    pub fn get_metadata(&self) -> &Metadata {
        &self.metadata
    }
}

/// Returning tuples is not well supported yet, so we return a u64 and bitmask/shift to split in into two
fn expand_return_pointer(ptr: u64) -> (u32, u32) {
    ((ptr & 0xFFFFFFFF) as u32, (ptr >> 32) as u32)
}
