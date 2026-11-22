// Outline and drop shadow for a layer drawn on the full frame (straight
// alpha). `outline_main` puts a solid outline of `radius` pixels around the
// layer's shape under it; `shadow_blur_main` blurs the shape's alpha along
// `direction` (moved by `offset` first); `shadow_composite_main` puts the
// blurred shape, in `color`, under the layer.

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coord: vec2f,
}

struct StyleUniforms {
    resolution: vec2f,
    direction: vec2f,
    offset: vec2f,
    radius: f32,
    _pad: f32,
    color: vec4f,
}

@group(0) @binding(0) var layer_texture: texture_2d<f32>;
@group(0) @binding(1) var layer_sampler: sampler;
@group(1) @binding(0) var second_texture: texture_2d<f32>;
@group(1) @binding(1) var second_sampler: sampler;
@group(2) @binding(0) var<uniform> uniforms: StyleUniforms;

const TAU: f32 = 6.28318530718;

/// `top` over `bottom`, both straight alpha.
fn over(top: vec4f, bottom: vec4f) -> vec4f {
    let alpha = top.a + bottom.a * (1.0 - top.a);
    if (alpha <= 0.0) {
        return vec4f(0.0);
    }
    let rgb = (top.rgb * top.a + bottom.rgb * bottom.a * (1.0 - top.a)) / alpha;
    return vec4f(rgb, alpha);
}

fn alpha_at(uv: vec2f) -> f32 {
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 0.0;
    }
    return textureSampleLevel(layer_texture, layer_sampler, uv, 0.0).a;
}

@fragment
fn outline_main(input: VertexOutput) -> @location(0) vec4f {
    let layer = textureSampleLevel(layer_texture, layer_sampler, input.tex_coord, 0.0);
    let radius = uniforms.radius;
    let texel = 1.0 / uniforms.resolution;
    // The most alpha within `radius`: rings from the outside in, each
    // sampled often enough that thin shapes are not missed.
    var reach = layer.a;
    let rings = i32(clamp(ceil(radius / 1.5), 1.0, 8.0));
    for (var ring = rings; ring >= 1; ring = ring - 1) {
        let r = radius * f32(ring) / f32(rings);
        let steps = i32(clamp(ceil(TAU * r / 1.5), 8.0, 96.0));
        for (var step = 0; step < steps; step = step + 1) {
            let angle = TAU * f32(step) / f32(steps);
            let uv = input.tex_coord + vec2f(cos(angle), sin(angle)) * r * texel;
            reach = max(reach, alpha_at(uv));
        }
        if (reach >= 0.999) {
            break;
        }
    }
    let outline = vec4f(uniforms.color.rgb, uniforms.color.a * reach);
    return over(layer, outline);
}

@fragment
fn shadow_blur_main(input: VertexOutput) -> @location(0) vec4f {
    let texel = 1.0 / uniforms.resolution;
    let origin = input.tex_coord - uniforms.offset * texel;
    let radius = uniforms.radius;
    var total = 0.0;
    var weights = 0.0;
    if (radius < 0.5) {
        total = alpha_at(origin);
        weights = 1.0;
    } else {
        let sigma = max(radius / 2.0, 0.5);
        let taps = i32(min(ceil(radius), 128.0));
        for (var i = -taps; i <= taps; i = i + 1) {
            let x = f32(i);
            let weight = exp(-(x * x) / (2.0 * sigma * sigma));
            total = total + alpha_at(origin + uniforms.direction * x * texel) * weight;
            weights = weights + weight;
        }
    }
    let alpha = total / weights;
    return vec4f(alpha, alpha, alpha, alpha);
}

@fragment
fn shadow_composite_main(input: VertexOutput) -> @location(0) vec4f {
    let layer = textureSampleLevel(layer_texture, layer_sampler, input.tex_coord, 0.0);
    let shadow_alpha = textureSampleLevel(second_texture, second_sampler, input.tex_coord, 0.0).a;
    let shadow = vec4f(uniforms.color.rgb, uniforms.color.a * shadow_alpha);
    return over(layer, shadow);
}
