// Entry point

mod boilerplate;
mod constants;
mod diagonal;

use std::borrow::Cow;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window, dpi::PhysicalSize,
};
use divrem::DivCeil;
use wgpu::util::DeviceExt;
use rand::Rng;

#[cfg(target_arch="wasm32")]
use winit::platform::web::WindowExtWebSys;

use crate::boilerplate::*;
use crate::constants::*;
use crate::diagonal::*;

async fn run(event_loop: EventLoop<()>, window: Window) {
    // ----------------------- Basic setup ----------------------

    let size = window.inner_size();

    let instance = wgpu::Instance::default();

    let surface = unsafe { instance.create_surface(&window) };

    // If window create failed on web, assume webgpu versioning is the cause.
    #[cfg(target_arch="wasm32")]
    if surface.is_err() {
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| Some(
                doc.body()
                    .and_then(|body| {
                        let div = doc.create_element("p").unwrap();
                        div.set_class_name("alert");
                        div.append_child(&doc.create_text_node("This app requires WebGPU. Either your browser does not support WebGPU, or you must enable an experimental flag to access it.")).unwrap();
                        body.replace_child(
                            &div,
                            &web_sys::Element::from(window.canvas().unwrap()))
                            .ok()
                    })
                    .expect("couldn't append canvas to document body")
            ));
        return
    }

    let surface = surface.unwrap();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            // Request an adapter which can render to our surface
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    // Create the logical device and command queue
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::empty(),
                // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    // Load the shaders from disk
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];

    // ----------------------- Content and pipelines -----------------------

    // ------ Data/operations for init/resize ------

    // Parts for diagonal (will be needed on resize)
    let (diagonal_vertex_buffer, diagonal_index_buffer, diagonal_index_len, diagonal_vertex_layout) = make_diagonal_buffers(&device, DEFAULT_STROKE);

    // Throw away diagonal pipeline layout, we will not be attaching bind groups
    let (_, diagonal_render_pipeline) = make_pipeline(&device, &shader, &[], "vs_plain", &[diagonal_vertex_layout], "fs_plain", &[Some(wgpu::TextureFormat::R8Unorm.into())]);

    let texture_bind_group_layout = make_texture_bind_group_layout(&device);

    let default_sampler = make_sampler(&device);

    fn generate_resize(size:PhysicalSize<u32>, device: &wgpu::Device, queue: &wgpu::Queue, surface: &wgpu::Surface, swapchain_format: wgpu::TextureFormat, swapchain_capabilities: &wgpu::SurfaceCapabilities, diagonal_vertex_buffer: &wgpu::Buffer, diagonal_index_buffer: &wgpu::Buffer, diagonal_index_len: usize, diagonal_render_pipeline: &wgpu::RenderPipeline, texture_bind_group_layout: &wgpu::BindGroupLayout, default_sampler:&wgpu::Sampler) -> (u32, wgpu::Texture, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, u32, wgpu::BindGroup) {
        // Set size
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        // ------ Diagonal ------

        // Decide how big the diagonal texture should be
        // TODO: What should TILES_ACROSS be? Should TILES_ACROSS depend on window DPI?
        let diagonal_texture_side = std::cmp::min(DivCeil::div_ceil(size.height, TILES_ACROSS), DivCeil::div_ceil(size.width, TILES_ACROSS));

        let (diagonal_texture, diagonal_view) = make_texture_gray(&device, diagonal_texture_side, diagonal_texture_side, true, "diagonal-texture");

        // Draw into the diagonal texture
        {
            let mut diagonal_texture_encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("diagonal-texture-generate") });
            {
                let mut rpass = diagonal_texture_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &diagonal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                rpass.set_pipeline(&diagonal_render_pipeline);
                rpass.set_vertex_buffer(0, diagonal_vertex_buffer.slice(..));
                rpass.set_index_buffer(diagonal_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..diagonal_index_len as u32, 0, 0..1);
            }
            queue.submit(Some(diagonal_texture_encoder.finish()));
        }

        // ------ Grid buffer ------

        // Dummy generation of grid buffer-- In next pass this will be a compute buffer
        let (side_x, side_y) = (diagonal_texture_side as f32/size.width  as f32,
                                diagonal_texture_side as f32/size.height as f32);

        // Vertices of one square
        const GRID_VERTEX_BASE : [f32;8] = [
            -1./2., -1./2.,
             1./2., -1./2.,
            -1./2.,  1./2.,
             1./2.,  1./2.,
        ];
        const GRID_UV_BASE : [f32;8] = [
            0., 0.,
            0., 1.,
            1., 0.,
            1., 1.,
        ];

        // Break that down into triangles
        // Each triangle has the midpoint as a vertex
        // In next pass this will not be const
        const GRID_INDEX_BASE : [u16;6] = [0, 2, 1,
                                           1, 2, 3];

        let mut grid_vertex:Vec<f32> = Default::default();
        let mut grid_uv:Vec<f32>     = Default::default();
        let mut grid_index:Vec<u16>  = Default::default();

        // Fill out grid vertex buffer
        let (across_x, across_y) = ((2./side_x).ceil() as i64,
                                    (2./side_y).ceil() as i64);
        let (offset_x, offset_y) = ((across_x as f32-1.)*side_x/2.,
                                    (across_y as f32-1.)*side_y/2.);
        {
            let mut index_offset:u16 = 0;
            let mut rng = rand::thread_rng();
            for y in 0..across_y {
                for x in 0..across_x {
                    let flip = rng.gen::<bool>();
                    for idx in 0..8 {
                        {
                            let mut value = GRID_VERTEX_BASE[idx];
                            match idx%2 {
                                0 => { value = (value + x as f32)*side_x - offset_x; }
                                1 => { value = (value - y as f32)*side_y + offset_y; }
                                _ => { unreachable!(); }
                            }
                            grid_vertex.push(value);
                        }
                        {
                            let mut value = GRID_UV_BASE[idx];
                            if 0==idx%2 && flip { value = 1. - value }
                            grid_uv.push(value);
                        }
                    }
                    for idx in 0..6 {
                        let value = GRID_INDEX_BASE[idx] + index_offset*4;
                        grid_index.push(value);
                    }
                    index_offset += 1;
                }
            }
        }

        // Upload grid vertex buffer
        let grid_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid vertex buffer"),
            contents: bytemuck::cast_slice(&grid_vertex),
            usage: wgpu::BufferUsages::VERTEX, // Immutable
        });

        // Upload grid vertex buffer
        let grid_uv_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid uv buffer"),
            contents: bytemuck::cast_slice(&grid_uv),
            usage: wgpu::BufferUsages::VERTEX, // Immutable
        });

        // Upload grid index buffer
        let grid_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid index buffer"),
            contents: bytemuck::cast_slice(&grid_index),
            usage: wgpu::BufferUsages::INDEX, // Immutable
        });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diagonal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&default_sampler),
                }
            ],
            layout: &texture_bind_group_layout,
            label: Some("grid bind group"),
        });

        (diagonal_texture_side, diagonal_texture, grid_vertex_buffer, grid_uv_buffer, grid_index_buffer, grid_index.len() as u32, texture_bind_group)
    }

    let (mut diagonal_texture_side, mut diagonal_texture, mut grid_vertex_buffer, mut grid_uv_buffer, mut grid_index_buffer, mut grid_index_len, mut texture_bind_group) = generate_resize(size, &device, &queue, &surface, swapchain_format, &swapchain_capabilities, &diagonal_vertex_buffer, &diagonal_index_buffer, diagonal_index_len, &diagonal_render_pipeline, &texture_bind_group_layout, &default_sampler);

    // ------ Data/operations for frame draw ------

    let (render_pipeline_layout, render_pipeline) = make_pipeline(&device, &shader, &[&texture_bind_group_layout], "vs_textured", &[VEC2_LAYOUT, VEC2_LAYOUT_LOCATION_1], "fs_textured", &[Some(swapchain_format.into())]);

    event_loop.run(move |event, _, control_flow| {
        // Have the closure take ownership of the resources.
        // `event_loop.run` never returns, therefore we must do this to ensure
        // the resources do not leak.
        let _ = (&instance, &adapter, &shader, &render_pipeline_layout);

        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // Reconfigure the surface with the new size
                (diagonal_texture_side, diagonal_texture, grid_vertex_buffer, grid_uv_buffer, grid_index_buffer, grid_index_len, texture_bind_group) = generate_resize(size, &device, &queue, &surface, swapchain_format, &swapchain_capabilities, &diagonal_vertex_buffer, &diagonal_index_buffer, diagonal_index_len, &diagonal_render_pipeline, &texture_bind_group_layout, &default_sampler);
                // On macos the window needs to be redrawn manually after resizing
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_vertex_buffer(0, grid_vertex_buffer.slice(..));
                    rpass.set_vertex_buffer(1, grid_uv_buffer.slice(..));
                    rpass.set_index_buffer(grid_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.set_bind_group(0, &texture_bind_group, &[]);
                    rpass.draw_indexed(0..grid_index_len, 0, 0..1);
                }

                queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

fn main() {
    let event_loop = EventLoop::new();
    let window = winit::window::Window::new(&event_loop).unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| {
                body.append_child(&web_sys::Element::from(window.canvas().unwrap()))
                    .ok()
            })
            .expect("couldn't append canvas to document body");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}
