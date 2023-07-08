// Construct WebGPU stuff

use std::mem;

// "This is an attribute array of Float32 pairs"
pub const VEC2_LAYOUT : wgpu::VertexBufferLayout = wgpu::VertexBufferLayout {
    array_stride: (mem::size_of::<f32>()*2) as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
    ],
};

// "This is an attribute array of Float32 pairs, offset one float"
pub const VEC2_LAYOUT_LOCATION_1 : wgpu::VertexBufferLayout = wgpu::VertexBufferLayout {
    array_stride: (mem::size_of::<f32>()*2) as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 1,
        },
    ],
};

// "This is an attribute array of Float32 pair pairs"
// Currently unused
pub const _VEC2X2_LAYOUT : wgpu::VertexBufferLayout = wgpu::VertexBufferLayout {
    array_stride: (mem::size_of::<f32>()*4) as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: (mem::size_of::<f32>()*2) as wgpu::BufferAddress,
            shader_location: 1,
        },
    ],
};

pub fn make_sampler(device: &wgpu::Device) -> wgpu::Sampler { // Currently unused
    device.create_sampler(&wgpu::SamplerDescriptor::default())
}

pub fn make_texture_gray(device: &wgpu::Device, width:u32, height:u32, target:bool, label:&str) -> (wgpu::Texture, wgpu::TextureView) {
	let mut usage = wgpu::TextureUsages::TEXTURE_BINDING.union(wgpu::TextureUsages::COPY_DST);
    if target { usage = usage | wgpu::TextureUsages::TEXTURE_BINDING.union(wgpu::TextureUsages::RENDER_ATTACHMENT) };

    let texture_descriptor:wgpu::TextureDescriptor = wgpu::TextureDescriptor {
	    size: wgpu::Extent3d {width:width, height:height, depth_or_array_layers:1},
	    mip_level_count: 1,
	    sample_count: 1,
	    dimension: wgpu::TextureDimension::D2,
	    format: wgpu::TextureFormat::R8Unorm,
	    usage: usage,
	    label: Some(label),
	    view_formats: &[],
	};

	let texture = device.create_texture(&texture_descriptor);

	let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    (texture, view)
}

pub fn make_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("single bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false }, /* FIXME: Is nearest a filter? */
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    })
}

pub fn make_pipeline(device: &wgpu::Device, shader: &wgpu::ShaderModule, bind_groups:&[&wgpu::BindGroupLayout], vertex_entry: &str, vertex_buffers:&[wgpu::VertexBufferLayout], fragment_entry:&str, fragment_targets:&[Option<wgpu::ColorTargetState>]) -> (wgpu::PipelineLayout, wgpu::RenderPipeline) {
	let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: bind_groups,
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: vertex_entry,
            buffers: vertex_buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: fragment_entry,
            targets: fragment_targets,
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Cw,
            cull_mode: Some(wgpu::Face::Back),
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    (pipeline_layout, render_pipeline)
}