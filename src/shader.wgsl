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

// Y is fixed at max; X is an exponential curve
const readback_curve = 20.; // FIXME: Match to x_across?
@fragment
fn fs_textured_readback(vertex: Textured) -> @location(0) vec4<f32> {
    let x = 1. - pow(2., -vertex.tex_coord.x*readback_curve);
    let value = textureSample(gray, gray_sampler, vec2(x, 1.)).r;
    return vec4(value, value, value, 1.0);
}

// This one-dimensional separable blur filter samples five points and averages them by different amounts.
// If we do it on two separate axes, we get a 2d blur.
// Weights and offsets taken from http://rastergrid.com/blog/2010/09/efficient-gaussian-blur-with-linear-sampling/

// The weights for the center, one-point-out, and two-point-out samples
const WEIGHT0 = 0.2270270270;
const WEIGHT1 = 0.3162162162;
const WEIGHT2 = 0.0702702703;

// The distances-from-center for the samples
const OFFSET1 = 1.3846153846;
const OFFSET2 = 3.2307692308;

@group(0)
@binding(2)
var<uniform> blur_resolution: vec2<f32>;

fn separable_blur(uv:vec2<f32>) -> f32 {
    var blurVector:vec2<f32> = blur_resolution;
    var color = 0.0;
    color += textureSample(gray, gray_sampler, uv).r * WEIGHT0;
    color += textureSample(gray, gray_sampler, uv + blurVector * OFFSET1).r * WEIGHT1;
    color += textureSample(gray, gray_sampler, uv - blurVector * OFFSET1).r * WEIGHT1;
    color += textureSample(gray, gray_sampler, uv + blurVector * OFFSET2).r * WEIGHT2;
    color += textureSample(gray, gray_sampler, uv - blurVector * OFFSET2).r * WEIGHT2;
    return color;
}

// Postprocess x component
@fragment
fn fs_postprocess_blur(vertex: Textured) -> @location(0) vec4<f32> {
    let value = separable_blur(vertex.tex_coord);
    return vec4(value, value, value, 1.0);
}

// Postprocess y component
@fragment
fn fs_postprocess_blur_threshold(vertex: Textured) -> @location(0) vec4<f32> {
    let prevalue = separable_blur(vertex.tex_coord);
    var value = smoothstep(0.6, 0.7, prevalue); // .4, .5 are also interesting. TODO scale to size of window
//    if (prevalue > 0.65) { value = 1.0; }
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

