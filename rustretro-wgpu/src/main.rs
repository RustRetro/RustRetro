use emulation_thread::EmulationMessage;
use futures::executor::block_on;
use wgpu::util::DeviceExt;

use std::sync::Arc;
use std::{sync::mpsc::Sender, thread::JoinHandle};

use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use std::path::PathBuf;
use structopt::StructOpt;

use rustretro_plugin::ControllerInput;
use rustretro_wasmtime_runner::Runner;

mod emulation_thread;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(parse(from_os_str))]
    core: Option<PathBuf>,

    #[structopt(parse(from_os_str))]
    rom: Option<PathBuf>,
}

// A 2D position is mapped to a 2D texture.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coord: [f32; 2],
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

struct State {
    emulator_handle: Sender<EmulationMessage>,
    controller1: ControllerInput,

    thread_join_handles: Vec<JoinHandle<()>>,

    surface: wgpu::Surface,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: Arc<wgpu::Queue>,
    size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    screen_bind_group: wgpu::BindGroup,
}

impl State {
    /// Create a new state and initialize the rendering pipeline.
    async fn new(window: &winit::window::Window, emulator: Runner) -> Self {
        let size = window.inner_size();

        // Used prefered graphic API
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        // Using an Arc because this will be shared with the emulation thread
        let queue = Arc::new(queue);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        surface.configure(&device, &config);

        let emulator_width = emulator.get_metadata().width;
        let emulator_height = emulator.get_metadata().height;

        // Create the texture to show the emulator screen
        let texture_size = wgpu::Extent3d {
            width: emulator_width,
            height: emulator_height,
            depth_or_array_layers: 1,
        };

        // Using an Arc here because this will be shared with the emulation thread
        let screen_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screen Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        });

        // Write an initial black screen before the first frame arrive
        let texture = vec![0u8; (emulator_width * emulator_height * 4) as usize];

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &screen_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &texture,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(4 * emulator_width),
                rows_per_image: std::num::NonZeroU32::new(emulator_height),
            },
            texture_size,
        );

        let screen_texture_view =
            screen_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let screen_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Bind groups are used to access the texture from the shader
        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &screen_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&screen_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&screen_texture_sampler),
                },
            ],
        });

        // Load the shader
        let shader = device.create_shader_module(&wgpu::include_wgsl!("shaders/base.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&screen_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Maps the four corner of the screen to the four corner of the texture
        let vertices = [
            Vertex {
                position: [-1.0, -1.0],
                tex_coord: [0.0, 1.0],
            },
            Vertex {
                position: [-1.0, 1.0],
                tex_coord: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, -1.0],
                tex_coord: [1.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0],
                tex_coord: [1.0, 0.0],
            },
        ];

        // Use two triangle to make a square filling the screen.
        let indices: [u16; 6] = [0, 3, 1, 0, 2, 3];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let (join_handle, emulator_handle) =
            emulation_thread::start(emulator, queue.clone(), screen_texture);

        let thread_join_handles = vec![join_handle];

        Self {
            emulator_handle,
            thread_join_handles,
            controller1: Default::default(),

            surface,
            config,
            device,
            queue,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,

            screen_bind_group,
        }
    }

    /// Update the size of the window so rendering is aware of the change
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// This is where we handle controller inputs
    fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { input, .. } => match input {
                // Handle controller inputs
                KeyboardInput {
                    state: ElementState::Pressed,
                    virtual_keycode: Some(key_code),
                    ..
                } => {
                    if let Ok(f) = virtual_keycode_to_controller_input(key_code) {
                        self.controller1.insert(f);

                        let _ = self
                            .emulator_handle
                            .send(EmulationMessage::Input(self.controller1));
                        true
                    } else {
                        false
                    }
                }

                KeyboardInput {
                    state: ElementState::Released,
                    virtual_keycode: Some(key_code),
                    ..
                } => {
                    if let Ok(f) = virtual_keycode_to_controller_input(key_code) {
                        self.controller1.remove(f);

                        let _ = self
                            .emulator_handle
                            .send(EmulationMessage::Input(self.controller1));
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Render the screen
    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.screen_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl Drop for State {
    fn drop(&mut self) {
        // Stop the emulator
        let _ = self.emulator_handle.send(EmulationMessage::Stop);

        // Wait for the threads to stop
        let mut handles = Vec::new();
        std::mem::swap(&mut self.thread_join_handles, &mut handles);

        for join_handle in handles {
            join_handle.join().unwrap(); // unwrap here is to bubble up panics
        }
    }
}

fn main() {
    // Parse CLI options
    let opt = Opt::from_args();

    // Create the window
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("RustRetro")
        .build(&event_loop)
        .unwrap();

    // Find core path
    let core_path = if let Some(p) = opt.core {
        p
    } else {
        native_dialog::FileDialog::new()
            .add_filter("WASM core", &["wasm"])
            .show_open_single_file()
            .unwrap()
            .expect("No core passed!")
    };

    // Find ROM path
    let rom_path = if let Some(p) = opt.rom {
        p
    } else {
        native_dialog::FileDialog::new()
            .add_filter("NES roms", &["nes"])
            .show_open_single_file()
            .unwrap()
            .expect("No rom passed!")
    };

    let mut save_path = rom_path.clone();
    save_path.set_extension("sav");

    // Read the ROM
    let rom = std::fs::read(rom_path).expect("Could not read the ROM file");

    // Read the core
    let core = std::fs::read(core_path).expect("Could not read the core file");

    // Create the emulator
    let emulator = Runner::new(&core, &rom, 1000);

    window.set_title(&emulator.get_metadata().name);

    // Wait until WGPU is ready
    let mut state = block_on(State::new(&window, emulator));

    // Handle window events
    event_loop.run(move |event, _, control_flow| match event {
        Event::RedrawRequested(_) => match state.render() {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
            Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
            Err(e) => eprintln!("{:?}", e),
        },
        Event::MainEventsCleared => window.request_redraw(),
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => {
            if !state.input(event) {
                match event {
                    // Exit if X button is clicked
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

                    // Update rendering if window is resized
                    WindowEvent::Resized(physical_size) => state.resize(*physical_size),
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        state.resize(**new_inner_size)
                    }

                    // Exit if ESC is pressed
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    } => *control_flow = ControlFlow::Exit,
                    _ => {}
                }
            }
        }
        _ => {}
    });
}

// This maps the keyboard input to a controller input
fn virtual_keycode_to_controller_input(keycode: &VirtualKeyCode) -> Result<ControllerInput, ()> {
    match keycode {
        VirtualKeyCode::X => Ok(ControllerInput::A),
        VirtualKeyCode::Z => Ok(ControllerInput::B),
        VirtualKeyCode::S => Ok(ControllerInput::START),
        VirtualKeyCode::A => Ok(ControllerInput::SELECT),
        VirtualKeyCode::Down => Ok(ControllerInput::DOWN),
        VirtualKeyCode::Left => Ok(ControllerInput::LEFT),
        VirtualKeyCode::Right => Ok(ControllerInput::RIGHT),
        VirtualKeyCode::Up => Ok(ControllerInput::UP),
        _ => Err(()),
    }
}
