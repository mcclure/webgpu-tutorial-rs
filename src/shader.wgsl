// All shaders

// Assume input is 0..1,0..1, Y-down. WebGPU wants -1..1, -1..1, Y-up.
@vertex
fn vs_plain(@location(0) v_position: vec2<f32>) -> @builtin(position) vec4<f32> {
    let position = v_position*2. - 1.;
    return vec4(position.x, -position.y, 0., 1.);
}

@fragment
fn fs_plain() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

struct Textured {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

// Coordinates in NDCs
@vertex
fn vs_textured(@location(0) v_position: vec2<f32>, @location(1) v_texcoord: vec2<f32>) -> Textured {
	var result: Textured;
	result.position = vec4(v_position, 0., 1.);
	result.tex_coord = v_texcoord;
    return result;
}

@fragment
fn fs_textured(vertex: Textured) -> @location(0) vec4<f32> {
    return vec4<f32>(vertex.tex_coord, 0.0, 1.0);
}
