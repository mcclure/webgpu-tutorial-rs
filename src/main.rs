// Entry point

mod audio;
mod boilerplate;
mod constants;
mod diagonal;

use std::array;
use std::borrow::Cow;
use std::mem;
use std::num::NonZeroU64;
use std::ops::DerefMut;
use web_time::{Duration, Instant};
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
                limits: wgpu::Limits::downlevel_defaults()
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
    let (_, diagonal_render_pipeline) = make_pipeline(&device, &shader, &[], "vs_plain", &[diagonal_vertex_layout], "fs_plain", &[Some(wgpu::TextureFormat::R8Unorm.into())], "diagonal");

    let grid_bind_group_layout = make_texture_bind_group_layout(&device, &[
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new((mem::size_of::<f32>()*2) as u64),
            },
            count: None,
        }], "Grid");

    let target_bind_group_layout = make_texture_bind_group_layout(&device, &[
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new((mem::size_of::<f32>()*2) as u64),
            },
            count: None,
        }], "Target");

    let rowshift_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Row shift bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage {read_only:false},
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new((mem::size_of::<f32>()) as u64),
                },
                count: None
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new((mem::size_of::<f32>()) as u64),
                },
                count: None
            },
        ]
    });

    let readback_bind_group_layout = make_texture_bind_group_layout(&device, &[], "Readback");

    let default_sampler = make_sampler(&device);

    const ZERO_ZERO_F32: [f32; 2] = [0.,0.];
    let grid_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Grid Uniform Buffer"),
        contents: bytemuck::cast_slice(&ZERO_ZERO_F32),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    const ZERO_U32: [u32; 1] = [0];
    let rowshift_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Row shift shift Uniform Buffer"),
        contents: bytemuck::cast_slice(&ZERO_U32),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    const TARGET_PASSES:usize = 8;
    let target_uniform_buffers: [wgpu::Buffer; TARGET_PASSES] = array::from_fn(|idx|
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Target-{} Uniform Buffer", idx+1)),
            contents: bytemuck::cast_slice(&ZERO_ZERO_F32),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    );

    // Triangle order for a quad in grid or target passes
    const GRID_INDEX_BASE : [u16;6] = [0, 2, 1,
                                       1, 2, 3];

    // GPUImageCopyBuffer requires this to be a multiple of 256?
    // FIXME: Make 1024
    const AUDIO_READBACK_BUFFER_LEN:usize = 256;

    // Create a quad UV buffer with random reflection. Assumes grid_uv is a multiple of 8.
    fn random_uv_push(grid_uv: &mut [f32]) {
        let mut rng = rand::thread_rng();

        const GRID_UV_BASE: [f32;8] = [
            0., 0.,
            0., 1.,
            1., 0.,
            1., 1.,
        ];

        // Generate 8 values at a time, by copying in GRID_UV_BASE and perturbing the X UV.
        let mut base = 0;
        while base < grid_uv.len() {
            let flip = rng.gen::<bool>();
            for idx in 0..8 {
                let mut value = GRID_UV_BASE[idx];
                if 0==idx%2 && flip { value = 1. - value }
                grid_uv[base+idx] = value;
            }
            base += 8;
        }
    }

    fn generate_resize(size:PhysicalSize<u32>, device: &wgpu::Device, queue: &wgpu::Queue, surface: &wgpu::Surface, swapchain_format: wgpu::TextureFormat, swapchain_capabilities: &wgpu::SurfaceCapabilities, diagonal_vertex_buffer: &wgpu::Buffer, diagonal_index_buffer: &wgpu::Buffer, diagonal_index_len: usize, diagonal_render_pipeline: &wgpu::RenderPipeline, grid_bind_group_layout: &wgpu::BindGroupLayout, default_sampler:&wgpu::Sampler, grid_uniform_buffer:&wgpu::Buffer, rowshift_bind_group_layout:&wgpu::BindGroupLayout, rowshift_uniform_buffer:&wgpu::Buffer, target_bind_group_layout:&wgpu::BindGroupLayout, target_uniform_buffers:&[wgpu::Buffer;TARGET_PASSES], readback_bind_group_layout:&wgpu::BindGroupLayout) -> (u32, f32, u64, u64, wgpu::Texture, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, u32, wgpu::BindGroup, wgpu::BindGroup, wgpu::util::StagingBelt, wgpu::BufferAddress, wgpu::BufferSize, [wgpu::TextureView;2], [wgpu::BindGroup;TARGET_PASSES], wgpu::Texture, wgpu::TextureView, wgpu::BindGroup, std::sync::Arc<wgpu::Buffer>) {
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

        let (diagonal_texture, diagonal_view) = make_texture_gray(&device, diagonal_texture_side, diagonal_texture_side, true, false, "diagonal-texture");

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
        // FIXME: side_x needs a min(2)
        let (side_x, side_y) = (diagonal_texture_side as f32/size.width  as f32,
                                diagonal_texture_side as f32/size.height as f32);

        // Vertices of one square
        // GRID_INDEX_BASE breaks this down into triangles
        const GRID_VERTEX_BASE : [f32;8] = [
            -1./2., -1./2.,
             1./2., -1./2.,
            -1./2.,  1./2.,
             1./2.,  1./2.,
        ];

        let mut grid_vertex:Vec<f32>  = Default::default();
        let mut grid_index:Vec<u16>   = Default::default();

        // Fill out grid vertex and index buffers
        let (across_x, across_y) = ((2./side_x).ceil() as i64,
                                    (2./side_y).ceil() as i64 + 1);
        let (offset_x, offset_y) = ((across_x as f32-1.)*side_x/2.,
                                    1.-side_y/2.);
        {
            let mut index_offset:u16 = 0;
            for y in 0..across_y {
                for x in 0..across_x {
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

        // Upload grid index buffer
        let grid_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid index buffer"),
            contents: bytemuck::cast_slice(&grid_index),
            usage: wgpu::BufferUsages::INDEX, // Immutable
        });

        // Create grid uv buffer.
        // Instead of passing an initial value, for this one it's most convenient to create it mapped...
        let grid_uv_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid uv buffer"),
            size: across_x as u64*across_y as u64*8*mem::size_of::<f32>() as u64,
            mapped_at_creation: true,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, // Mutable, can be targeted by copies or by shaders
        });

        { // ...and then write bytes to write-mapped uv buffer
            let mut mapped_bytes = grid_uv_buffer.slice(..).get_mapped_range_mut();
            random_uv_push(bytemuck::cast_slice_mut::<u8, f32>(&mut mapped_bytes));
        }
        grid_uv_buffer.unmap();

        // Constants for uploading new rows to the grid of UVs
        let grid_uv_staging_size = across_x as u64*8*mem::size_of::<f32>() as u64;
        let grid_uv_staging_belt = wgpu::util::StagingBelt::new(grid_uv_staging_size);
        let grid_uv_staging_offset = grid_uv_staging_size*(across_y as u64-1);

        // Make a bind group for a stage which takes a texture as input
        let texture_bind_group = |view:&wgpu::TextureView, buffer:&wgpu::Buffer, bind_group_layout:&wgpu::BindGroupLayout, name:&str| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&default_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buffer.as_entire_binding(),
                    }
                ],
                layout: &bind_group_layout,
                label: Some(name),
            })
        };

        // Bind group for initial grid draw
        let grid_bind_group = texture_bind_group(&diagonal_view, &grid_uniform_buffer, &grid_bind_group_layout, "grid bind group");

        // Bind group for shift-up compute shader
        let rowshift_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: grid_uv_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: rowshift_uniform_buffer.as_entire_binding(),
                },
            ],
            layout: &rowshift_bind_group_layout,
            label: Some("Row shift bind group")
        });

        // How much to shift up, in compute shader? This value doesn't change until next resize.
        let pair:[u32;1] = [(across_x*8).try_into().unwrap()]; // 8 because we copy floats not [f32x2;4]s
        queue.write_buffer(&rowshift_uniform_buffer, 0, bytemuck::cast_slice(&pair));

        // The blur needs to happen multiple times to look soft. Make two back-buffer textures; we'll render out of one into the other, then swap.
        let target_views: [wgpu::TextureView; 2] = array::from_fn(|view_idx| {
            let (_, target_view) = make_texture_gray(&device, size.width, size.height, true, false, &format!("target texture {}", view_idx));
            target_view
        });

        const BLUR_SCALE_BASE:u32 = 1;

        // Bind groups for our chain of blur passes; they can reuse texture targets, but each one needs its own parameters.
        let target_bind_groups: [wgpu::BindGroup; TARGET_PASSES] = array::from_fn(|stage| {            
            texture_bind_group(&target_views[stage%2], &target_uniform_buffers[stage], &target_bind_group_layout, &format!("target-{} bind group", stage))
        });

        // Fill out blur-pass parameters
        for stage in 0..TARGET_PASSES {
            let blur_scale = (BLUR_SCALE_BASE << (stage/2)) as f32;
            let target_buffer_contents: [f32; 2] =
                if 0==stage%2 { [blur_scale/size.width as f32, 0.] }   // Even passes X-blur
                else          { [0., blur_scale/size.height as f32] }; // Odd passes Y-blur
            queue.write_buffer(&target_uniform_buffers[stage], 0, bytemuck::cast_slice(&target_buffer_contents));
        }

        // Read-back texture
        // FIXME: Should this be a 1D texture instead of a 1-height 2D texture? Does it even matter?
        let (readback_texture, readback_view) = make_texture_gray(&device, AUDIO_READBACK_BUFFER_LEN as u32, 1, true, true, "readback texture");

        // Read-back buffer (will be used by callback, so has to be refcounted, and callback is 'Send so the Rust typesystem forces an unnecessary atomicity requirement)
        let readback_buffer = std::sync::Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback buffer"),
            size: AUDIO_READBACK_BUFFER_LEN as u64*mem::size_of::<f32>() as u64,
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ // Mutable, can be targeted by copies or by shaders
        }));

        // Bind group for write into read-back texture
        // Can't use texture_bind_group because no parameters
        // Attempt to read from "final" view texture
        let readback_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&target_views[(TARGET_PASSES+1)%2]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&default_sampler),
                },
            ],
            layout: &readback_bind_group_layout,
            label: Some("Readback bind group"),
        });

        (diagonal_texture_side, side_y, across_x.try_into().unwrap(), across_y.try_into().unwrap(), diagonal_texture, grid_vertex_buffer, grid_uv_buffer, grid_index_buffer, grid_index.len() as u32, grid_bind_group, rowshift_bind_group, grid_uv_staging_belt, grid_uv_staging_offset, NonZeroU64::new(grid_uv_staging_size).unwrap(), target_views, target_bind_groups, readback_texture, readback_view, readback_bind_group, readback_buffer)
    }

    let (mut diagonal_texture_side, mut diagonal_texture_side_ndc, mut diagonal_texture_count_x, mut diagonal_texture_count_y, mut diagonal_texture, mut grid_vertex_buffer, mut grid_uv_buffer, mut grid_index_buffer, mut grid_index_len, mut grid_bind_group, mut rowshift_bind_group, mut grid_uv_staging_belt, mut grid_uv_staging_offset, mut grid_uv_staging_size, mut target_views, mut target_bind_groups, mut readback_texture, mut readback_view, mut readback_bind_group, mut readback_buffer) = generate_resize(size, &device, &queue, &surface, swapchain_format, &swapchain_capabilities, &diagonal_vertex_buffer, &diagonal_index_buffer, diagonal_index_len, &diagonal_render_pipeline, &grid_bind_group_layout, &default_sampler, &grid_uniform_buffer, &rowshift_bind_group_layout, &rowshift_uniform_buffer, &target_bind_group_layout, &target_uniform_buffers, &readback_bind_group_layout);

    // ------ Data/operations for frame draw ------

    let (render_pipeline_layout, render_pipeline) = make_pipeline(&device, &shader, &[&grid_bind_group_layout], "vs_textured_offset", &[VEC2_LAYOUT, VEC2_LAYOUT_LOCATION_1], "fs_textured", &[Some(wgpu::TextureFormat::R8Unorm.into())], "grid");

    let rowshift_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Row shift pipeline"),
        layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Row shift pipeline layout"),
            bind_group_layouts:&[&rowshift_bind_group_layout],
            push_constant_ranges:&[]
        })),
        module: &shader,
        entry_point: "internal_copy",
    });

    let (target_pipeline_layout, target_pipeline) = make_pipeline(&device, &shader, &[&target_bind_group_layout], "vs_textured", &[VEC2X2_LAYOUT], "fs_postprocess_blur", &[Some(wgpu::TextureFormat::R8Unorm.into())], "target-blur");
    let (target_final_pipeline_layout, target_final_pipeline) = make_pipeline(&device, &shader, &[&target_bind_group_layout], "vs_textured", &[VEC2X2_LAYOUT], "fs_postprocess_blur_threshold", &[Some(swapchain_format.into())], "target-blur-threshold");

    let (target_vertex_buffer, target_index_buffer, target_index_len) = {
        // Combined vertex and UV for a full-screen quad
        const TARGET_VERTEX : [f32;16] = [
            -1., -1., 0., 0.,
             1., -1., 1., 0.,
            -1.,  1., 0., 1.,
             1.,  1., 1., 1.,
        ];

        // Upload grid vertex buffer
        let target_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Target vertex buffer"),
            contents: bytemuck::cast_slice(&TARGET_VERTEX),
            usage: wgpu::BufferUsages::VERTEX, // Immutable
        });

        // Upload grid index buffer
        // This is always equal to the first 6 values in grid_indx_buffer; technically they could be merged
        let target_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid index buffer"),
            contents: bytemuck::cast_slice(&GRID_INDEX_BASE),
            usage: wgpu::BufferUsages::INDEX, // Immutable
        });

        (target_vertex_buffer, target_index_buffer, 6)
    };

    let (readback_pipeline_layout, readback_pipeline) = make_pipeline(&device, &shader, &[&readback_bind_group_layout], "vs_textured", &[VEC2X2_LAYOUT], "fs_textured_ymax", &[Some(wgpu::TextureFormat::R8Unorm.into())], "readback");

    let mut grid_last_reset = Instant::now();
    let mut grid_last_reset_overflow = 0.;

    event_loop.run(move |event, _, control_flow| {
        // Have the closure take ownership of the resources.
        // `event_loop.run` never returns, therefore we must do this to ensure
        // the resources do not leak.
        let _ = (&instance, &adapter, &shader, &render_pipeline_layout);

        *control_flow = if cfg!(feature = "metal-auto-capture") {
            // We are running in a debugging tool, quit after 1 frame
            ControlFlow::Exit
        } else {
            // Request continuous animation
            ControlFlow::Poll
        };

        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // Reconfigure the surface with the new size
                (diagonal_texture_side, diagonal_texture_side_ndc, diagonal_texture_count_x, diagonal_texture_count_y, diagonal_texture, grid_vertex_buffer, grid_uv_buffer, grid_index_buffer, grid_index_len, grid_bind_group, rowshift_bind_group, grid_uv_staging_belt, grid_uv_staging_offset, grid_uv_staging_size, target_views, target_bind_groups, readback_texture, readback_view, readback_bind_group, readback_buffer) = generate_resize(size, &device, &queue, &surface, swapchain_format, &swapchain_capabilities, &diagonal_vertex_buffer, &diagonal_index_buffer, diagonal_index_len, &diagonal_render_pipeline, &grid_bind_group_layout, &default_sampler, &grid_uniform_buffer, &rowshift_bind_group_layout, &rowshift_uniform_buffer, &target_bind_group_layout, &target_uniform_buffers, &readback_bind_group_layout);
                // On macos the window needs to be redrawn manually after resizing
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let mut encoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                const DRAW_OPS: wgpu::Operations<wgpu::Color> = wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: true,
                };

                // Animate
                {
                    // Time frame is drawn at, for animation purposes
                    let grid_current = Instant::now();
                    // Time since last rowshift (in % of time to next rowshift)
                    let mut grid_time_offset = grid_current.duration_since(grid_last_reset).as_secs_f32()*GRID_ANIMATE_SPEED + grid_last_reset_overflow;
                    // Time-in-% is more than 100%
                    if grid_time_offset > 1. {
                        grid_time_offset %=  1.; // FIXME: What if it's more than 2?
                        grid_last_reset = grid_current;
                        grid_last_reset_overflow = grid_time_offset;

                        // Begin this frame with a rowshift compute pass
                        {
                            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
                            cpass.set_pipeline(&rowshift_pipeline);
                            cpass.set_bind_group(0, &rowshift_bind_group, &[]);
                            cpass.dispatch_workgroups(1, 1, 1); // Number of cells to run, the (x,y,z) size of item being processed
                        }

                        // Shifting up one row leaves an effectively blank space; fill it with new values
                        // If this were JavaScript we'd map a temp buffer here, but instead the staging belt maps one for us.
                        {
                            let mut mapped_bytes = grid_uv_staging_belt.write_buffer(&mut encoder, &grid_uv_buffer, grid_uv_staging_offset, grid_uv_staging_size, &device);
                            random_uv_push(bytemuck::cast_slice_mut::<u8, f32>(mapped_bytes.deref_mut()));
                        }
                        grid_uv_staging_belt.finish();
                    }

                    // Set animation (scroll) parameter
                    // Notice: We may have already added passes to encoder, but those don't get submitted until cpass is "finished", so this write happens *before* any passes
                    let pair:[f32;2] = [0., grid_time_offset*diagonal_texture_side_ndc];
                    queue.write_buffer(&grid_uniform_buffer, 0, bytemuck::cast_slice(&pair));
                }

                // Draw
                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Initial draw of grid
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target_views[0],
                            resolve_target: None,
                            ops: DRAW_OPS,
                        })],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_vertex_buffer(0, grid_vertex_buffer.slice(..));
                    rpass.set_vertex_buffer(1, grid_uv_buffer.slice(..));
                    rpass.set_index_buffer(grid_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.set_bind_group(0, &grid_bind_group, &[]);
                    rpass.draw_indexed(0..grid_index_len, 0, 0..1);
                }

                // Postprocessing passes
                for stage in 0..TARGET_PASSES {
                    // All stages do one dimension in a separable blur-- except the last, which blur-then-thresholds.
                    let final_stage = stage == TARGET_PASSES-1;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: if final_stage {
                                    &view
                                } else {
                                    &target_views[(stage+1)%2]
                                },
                            resolve_target: None,
                            ops: DRAW_OPS,
                        })],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(if final_stage { &target_final_pipeline } else { &target_pipeline });
                    rpass.set_vertex_buffer(0, target_vertex_buffer.slice(..));
                    rpass.set_index_buffer(grid_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.set_bind_group(0, &target_bind_groups[stage], &[]);
                    rpass.draw_indexed(0..target_index_len, 0, 0..1);
                }

                // Read back final row for audio
                // Draw final row into 1-pixel-high texture:
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &readback_view,
                            resolve_target: None,
                            ops: DRAW_OPS,
                        })],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&readback_pipeline);
                    rpass.set_vertex_buffer(0, target_vertex_buffer.slice(..));
                    rpass.set_index_buffer(grid_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.set_bind_group(0, &readback_bind_group, &[]);
                    rpass.draw_indexed(0..target_index_len, 0, 0..1);
                }
                encoder.copy_texture_to_buffer(
                    wgpu::ImageCopyTextureBase {
                        texture: &readback_texture,
                        mip_level:0,
                        origin: wgpu::Origin3d { x:0,y:0,z:0 },
                        aspect: wgpu::TextureAspect::All
                    },
                    wgpu::ImageCopyBuffer {
                        buffer: &readback_buffer,
                        layout: wgpu::ImageDataLayout {
                            offset:0,
                            bytes_per_row:None, // Not required, one row.
                            rows_per_image:None, // Not required, texture not cubic.
                        }
                    },
                    wgpu::Extent3d {width:AUDIO_READBACK_BUFFER_LEN as u32, height:1, depth_or_array_layers:1}
                );

                // Done
                queue.submit(Some(encoder.finish()));
                {
                    let slice = readback_buffer.slice(..);
                    let readback_buffer = readback_buffer.clone();

                    // The WebGPU spec says this promise resolves successfully only "after the completion of currently-enqueued operations that use 'this'", so this doubles as an on_submitted_work_done for these purposes.
                    slice.map_async(wgpu::MapMode::Read, move |result| {
                        if let Ok(()) = result {
                            let slice = readback_buffer.slice(..);
                            let range = slice.get_mapped_range();
                            let row = bytemuck::cast_slice::<u8, f32>(&range);
                            println!("HAVE MAPPED RANGE! {}", row[0]);
                        }
                        readback_buffer.unmap();
                    });
                }
                frame.present();
            }
            // The winit docs recommend doing your "state update" in MainEventsCleared and your draw triggering/logic here.
            Event::RedrawEventsCleared => {
                window.request_redraw()
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
    let audio = crate::audio::audio_spawn();

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
