use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use rustretro_plugin::serde_json;
use rustretro_plugin::{ControllerInput, Metadata};
use wasmtime::*;

pub struct Runner {
    emulator_pointer: u32,

    timeout_ms: u64,
    epoch_handle: Option<JoinHandle<()>>,
    epoch_stop_sender: Sender<()>,

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

        // Stop the epoch
        self.epoch_stop_sender
            .send(())
            .expect("Sending shouldn't fail if the thread didn't panic");
        match self.epoch_handle.take() {
            Some(x) => {
                x.join().unwrap(); // Unwrap to bubble up errors
            }
            _ => {}
        };
    }
}

struct WasmVec {
    pub ptr: u32,
    pub length: u32,
}

#[cfg(debug_assertions)]
impl Drop for WasmVec {
    fn drop(&mut self) {
        // Detect if a vector hasn't been freed properly
        panic!("A WASM vec wasn't dropped!");
    }
}

impl Runner {
    ///
    pub fn new(core: &[u8], rom: &[u8], timeout_ms: u64) -> Self {
        // Optimize the engine for execution speed
        let mut config = Config::new();
        config.cranelift_opt_level(OptLevel::Speed);
        config.epoch_interruption(true);

        // Compile the WASM module
        let engine = Engine::new(&config).expect("wgpu config is invalid!");
        let module = Module::new(&engine, core).unwrap();
        let mut store = Store::new(&engine, "Rustretro Wasmtime Runner".to_string());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();

        // Limit time for the new function
        store.set_epoch_deadline(timeout_ms);

        // Increment the epoch
        let (epoch_stop_sender, epoch_stop_receiver) = std::sync::mpsc::channel();
        let epoch_handle = Some(std::thread::spawn(move || loop {
            match epoch_stop_receiver.try_recv() {
                Ok(_) => break,
                _ => {}
            };

            std::thread::sleep(Duration::from_millis(1));
            engine.increment_epoch();
        }));

        // The default memory is simply called "memory"
        let memory = instance.get_memory(&mut store, "memory").unwrap();

        // Bind allocation functions
        let wasm_alloc_vec = instance
            .get_typed_func::<u32, u32, _>(&mut store, "__rustretro_plugin_alloc_vec")
            .unwrap();
        let wasm_free_vec = instance
            .get_typed_func::<(u32, u32), (), _>(&mut store, "__rustretro_plugin_free_vec")
            .unwrap();

        // Copy the rom to WASM memory
        let rom_buffer = alloc_vec_static(&mut store, &wasm_alloc_vec, rom.len() as u32);

        memory
            .write(&mut store, rom_buffer.ptr as usize, rom)
            .unwrap();

        // Instanciate the emulator
        let wasm_create_core = instance
            .get_typed_func::<(u32, u32), u32, _>(&mut store, "__rustretro_plugin_create_core")
            .unwrap();

        let emulator_pointer = wasm_create_core
            .call(&mut store, (rom_buffer.ptr, rom_buffer.length))
            .unwrap();

        // Free the ROM buffer
        free_vec_static(&mut store, &wasm_free_vec, rom_buffer);

        // Fetch the core metadata
        let wasm_get_metadata = instance
            .get_typed_func::<u32, u64, _>(&mut store, "__rustretro_plugin_get_metadata")
            .unwrap();

        let ptr = wasm_get_metadata
            .call(&mut store, emulator_pointer)
            .unwrap();

        let metadata_buffer = expand_return_pointer(ptr);

        let mut metadata_bytes = vec![0u8; metadata_buffer.length as usize];
        memory
            .read(
                &mut store,
                metadata_buffer.ptr as usize,
                &mut metadata_bytes,
            )
            .unwrap();

        let metadata: Metadata = serde_json::from_slice(&metadata_bytes).unwrap();

        // Free the metadata buffer
        free_vec_static(&mut store, &wasm_free_vec, metadata_buffer);

        // Bind exposed functions
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

            timeout_ms,
            epoch_handle,
            epoch_stop_sender,

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
        self.store.set_epoch_deadline(self.timeout_ms);

        let ptr = self
            .wasm_clock_until_frame
            .call(&mut self.store, self.emulator_pointer)
            .unwrap();

        let frame_buffer = expand_return_pointer(ptr);

        let mut buffer = vec![0u8; frame_buffer.length as usize];
        self.memory
            .read(&mut self.store, frame_buffer.ptr as usize, &mut buffer)
            .unwrap();

        self.free_vec(frame_buffer);

        buffer
    }

    pub fn get_metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn free_vec(&mut self, wasm_vec: WasmVec) {
        free_vec_static(&mut self.store, &self.wasm_free_vec, wasm_vec)
    }
}

fn alloc_vec_static(
    store: &mut Store<String>,
    wasm_alloc_vec: &TypedFunc<u32, u32>,
    length: u32,
) -> WasmVec {
    let ptr = wasm_alloc_vec.call(store, length).unwrap();

    WasmVec { ptr, length }
}

fn free_vec_static(
    store: &mut Store<String>,
    wasm_free_vec: &TypedFunc<(u32, u32), ()>,
    wasm_vec: WasmVec,
) {
    wasm_free_vec
        .call(store, (wasm_vec.ptr, wasm_vec.length))
        .unwrap();

    std::mem::forget(wasm_vec);
}

/// Returning tuples is not well supported yet, so we return a u64 and bitmask/shift to split in into two
fn expand_return_pointer(ptr: u64) -> WasmVec {
    WasmVec {
        ptr: (ptr & 0xFFFFFFFF) as u32,
        length: (ptr >> 32) as u32,
    }
}
