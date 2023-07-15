// Assets for an image of a diagonal slash

use wgpu::util::DeviceExt;

use crate::boilerplate::*;

/*
	Let's draw a shape... with our MINDS
	Indices in "diagonal buffer":

     0 1
     2
        3
            4
          5 6
*/

pub fn make_diagonal_buffers(device: &wgpu::Device, stroke:f32) -> (wgpu::Buffer, wgpu::Buffer, usize, wgpu::VertexBufferLayout<'static>) {
	let diag_vertex : [f32;14] = [
	    0., 0.,        // 0
	    stroke, 0.,    // 1
	    0., stroke,    // 2
	    0.5, 0.5,      // 3
	    1., 1.-stroke, // 4
	    1.-stroke, 1., // 5
	    1.0, 1.0       // 6
	];

	// Break that down into triangles
	// Each triangle has the midpoint as a vertex
    // Notice a quirk: Every pass drawn in main.rs uses Y-up, but this uses Y-down,
    // so this triangle winding is reversed from the index buffer in main.rs.
	const DIAG_INDEX : [u16;18] = [0, 1, 3,
	                               0, 3, 2,
	                               1, 4, 3,
	                               2, 3, 5,
	                               3, 4, 6,
	                               3, 6, 5];

	let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Diagonal vertex buffer"),
        contents: bytemuck::cast_slice(&diag_vertex),
        usage: wgpu::BufferUsages::VERTEX, // Immutable
    });

    let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Diagonal index buffer"),
        contents: bytemuck::cast_slice(&DIAG_INDEX),
        usage: wgpu::BufferUsages::INDEX, // Immutable
    });

    (vertex, index, DIAG_INDEX.len(), VEC2_LAYOUT)
}
