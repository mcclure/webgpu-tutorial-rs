// Construct WebGPU stuff

use wgpu::ColorTargetState;

pub fn make_pipeline(device: &wgpu::Device, shader: &wgpu::ShaderModule, vertex_entry: &str, vertex_buffers:&[wgpu::VertexBufferLayout], fragment_entry:&str, fragment_targets:&[Option<wgpu::ColorTargetState>]) -> (wgpu::PipelineLayout, wgpu::RenderPipeline) {
	let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
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