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

@group(0)
@binding(0)
var gray: texture_2d<f32>;
@group(0)
@binding(1)
var gray_sampler: sampler;

@group(0)
@binding(2)
var<uniform> offset: vec2<f32>;

// Coordinates in NDCs
@vertex
fn vs_textured(@location(0) v_position: vec2<f32>, @location(1) v_texcoord: vec2<f32>) -> Textured {
    var result: Textured;
    result.position = vec4(v_position, 0., 1.);
    result.tex_coord = v_texcoord;
    return result;
}

@vertex
fn vs_textured_offset(@location(0) v_position: vec2<f32>, @location(1) v_texcoord: vec2<f32>) -> Textured {
	var result: Textured;
	result.position = vec4(v_position + offset, 0., 1.);
	result.tex_coord = v_texcoord;
    return result;
}

@fragment
fn fs_textured(vertex: Textured) -> @location(0) vec4<f32> {
    let value = textureSample(gray, gray_sampler, vertex.tex_coord).r;
    return vec4(value, value, value, 1.0);
}

// To test postprocess shaders, invert
@fragment
fn fs_postprocess1(vertex: Textured) -> @location(0) vec4<f32> {
    let value = 1.0 - textureSample(gray, gray_sampler, vertex.tex_coord).r;
    return vec4(value, value, value, 1.0);
}

// Compute shaders

@group(0)
@binding(0)
var<storage, read_write> internal_copy_buffer: array<f32>;

@group(0)
@binding(1)
var<uniform> internal_copy_stride: u32;

@compute
@workgroup_size(1)
fn internal_copy(@builtin(global_invocation_id) global_id: vec3<u32>) {
    var limit = arrayLength(&internal_copy_buffer)-internal_copy_stride;
    for(var idx = 0u; idx < limit; idx++) {
        internal_copy_buffer[idx] = internal_copy_buffer[idx+internal_copy_stride];
    }
}

