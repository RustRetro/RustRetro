use rustretro_plugin::ControllerInput;
use rustretro_wasmtime_runner::Runner;
use std::{
    sync::{mpsc, Arc},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub enum EmulationMessage {
    Input(ControllerInput),
    Stop,
}

pub fn start(
    mut emulator: Runner,
    queue: Arc<wgpu::Queue>,
    texture: wgpu::Texture,
) -> (JoinHandle<()>, mpsc::Sender<EmulationMessage>) {
    let (input_sender, input_receiver) = mpsc::channel::<EmulationMessage>();

    let join_handle = std::thread::spawn(move || {
        let metadata = emulator.get_metadata().clone();

        let frame_time = Duration::from_secs_f32(1.0 / metadata.frames_per_seconds);
        let mut last_frame_time = Instant::now();

        loop {
            match input_receiver.try_recv() {
                Ok(EmulationMessage::Input(x)) => emulator.controller_input(x),
                Ok(EmulationMessage::Stop) => break,
                _ => {}
            }

            let current_time = Instant::now();

            if last_frame_time + frame_time < current_time {
                last_frame_time = Instant::now();

                // Get a frame from the emulation and write it to the texture
                let frame = emulator.clock_until_frame();

                let emulator_width = metadata.width;
                let emulator_height = metadata.height;

                // Update texture
                let texture_size = wgpu::Extent3d {
                    width: emulator_width,
                    height: emulator_height,
                    depth_or_array_layers: 1,
                };

                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &frame,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: std::num::NonZeroU32::new(4 * emulator_width),
                        rows_per_image: std::num::NonZeroU32::new(emulator_height),
                    },
                    texture_size,
                );
            } else {
                // Sleep for the remaining time until the frame
                std::thread::sleep(last_frame_time + frame_time - current_time);
            }
        }
    });

    (join_handle, input_sender)
}
