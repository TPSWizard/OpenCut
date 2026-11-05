// Shared by every effect shader: the fullscreen vertex output and the
// uniform block, four vec4s of scalars (`s0.x` is the first number the
// pass lists, `s0.y` the second, and so on) plus the frame size and a
// direction for separable passes.
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coord: vec2f,
}

struct EffectUniforms {
    resolution: vec2f,
    direction: vec2f,
    s0: vec4f,
    s1: vec4f,
    s2: vec4f,
    s3: vec4f,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(1) @binding(0) var<uniform> uniforms: EffectUniforms;

// A cheap hash in [0, 1) from a 2D seed.
fn hash2(p: vec2f) -> f32 {
    let h = dot(p, vec2f(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Samples with the edge clamped, so remapped coordinates never read
// outside the frame.
fn sample_clamped(uv: vec2f) -> vec4f {
    return textureSample(input_texture, input_sampler, clamp(uv, vec2f(0.0), vec2f(1.0)));
}
