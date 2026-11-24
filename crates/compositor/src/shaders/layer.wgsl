struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coord: vec2f,
}

struct LayerUniforms {
    resolution: vec2f,
    center: vec2f,
    size: vec2f,
    rotation_radians: f32,
    opacity: f32,
    flip_x: f32,
    flip_y: f32,
    /// Rounded corners, in pixels (0 for square ones).
    corner_radius: f32,
    /// 1 when `pin_row_*` map box points to texture uv (a corner pin).
    pinned: f32,
    pin_row_0: vec4f,
    pin_row_1: vec4f,
    pin_row_2: vec4f,
    /// Motion blur (`crates/compositor/src/motion_blur.rs`): the quad at
    /// shutter open and close as (centre, size), and (open rotation, close
    /// rotation, draws, unused). Fewer than two draws is no blur.
    motion_from: vec4f,
    motion_to: vec4f,
    motion: vec4f,
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(1) @binding(0) var<uniform> uniforms: LayerUniforms;

fn rotate_inverse(point: vec2f, angle: f32) -> vec2f {
    let c = cos(angle);
    let s = sin(angle);
    return vec2f(
        point.x * c + point.y * s,
        -point.x * s + point.y * c,
    );
}

fn box_point_at(pixel: vec2f, center: vec2f, size: vec2f, rotation: f32) -> vec2f {
    let local = rotate_inverse(pixel - center, rotation);
    return vec2f(
        local.x / size.x + 0.5,
        local.y / size.y + 0.5,
    );
}

/// A corner pin maps the box point back to the texture in perspective
/// (`crates/compositor/src/corner_pin.rs`); xy over z is the uv.
fn pin_point(box_point: vec2f) -> vec3f {
    return vec3f(
        dot(uniforms.pin_row_0.xyz, vec3f(box_point, 1.0)),
        dot(uniforms.pin_row_1.xyz, vec3f(box_point, 1.0)),
        dot(uniforms.pin_row_2.xyz, vec3f(box_point, 1.0)),
    );
}

/// The layer drawn with this quad at `pixel`, as straight colour and alpha
/// (alpha includes coverage and opacity). `uv_per_pixel` softens pinned
/// edges.
fn layer_sample(pixel: vec2f, center: vec2f, size: vec2f, rotation: f32, uv_per_pixel: vec2f) -> vec4f {
    let box_point = box_point_at(pixel, center, size, rotation);
    // A point beyond a corner pin's horizon (w not positive) shows nothing.
    let pinned = uniforms.pinned > 0.5;
    let h = pin_point(box_point);
    let pinned_uv = h.xy / max(abs(h.z), 1e-6);
    let uv = select(box_point, pinned_uv, pinned);

    if (pinned && h.z <= 0.0) {
        return vec4f(0.0, 0.0, 0.0, 0.0);
    }
    var edge = 1.0;
    if (pinned) {
        let inside = min(uv, vec2f(1.0) - uv) / uv_per_pixel;
        edge = clamp(min(inside.x, inside.y) + 0.5, 0.0, 1.0);
        if (edge <= 0.0) {
            return vec4f(0.0, 0.0, 0.0, 0.0);
        }
    } else if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4f(0.0, 0.0, 0.0, 0.0);
    }

    let clamped_uv = clamp(uv, vec2f(0.0), vec2f(1.0));
    let sample_uv = vec2f(
        select(clamped_uv.x, 1.0 - clamped_uv.x, uniforms.flip_x > 0.5),
        select(clamped_uv.y, 1.0 - clamped_uv.y, uniforms.flip_y > 0.5),
    );
    let color = textureSampleLevel(source_texture, source_sampler, sample_uv, 0.0);
    var coverage = edge;
    let half_size = abs(size) * 0.5;
    let radius = min(uniforms.corner_radius, min(half_size.x, half_size.y));
    if (radius > 0.0) {
        // Signed distance to the rounded box, softened over one pixel; with
        // a pin, measured on the texture so the corners bend with it.
        let shape_point = (clamped_uv - vec2f(0.5)) * abs(size);
        let q = abs(shape_point) - (half_size - vec2f(radius));
        let distance = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - radius;
        coverage = min(coverage, clamp(0.5 - distance, 0.0, 1.0));
    }
    return vec4f(color.rgb, color.a * uniforms.opacity * coverage);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let pixel = input.tex_coord * uniforms.resolution;
    // Soft pinned edges: the distance to the nearest edge in pixels, from
    // the quad at the frame's time (derivatives need uniform control flow).
    let h = pin_point(box_point_at(pixel, uniforms.center, uniforms.size, uniforms.rotation_radians));
    let uv_per_pixel = max(fwidth(h.xy / max(abs(h.z), 1e-6)), vec2f(1e-6));

    let count = u32(uniforms.motion.z);
    if (count < 2u) {
        return layer_sample(pixel, uniforms.center, uniforms.size, uniforms.rotation_radians, uv_per_pixel);
    }
    // Motion blur: the quad at evenly spaced points from shutter open to
    // close, averaged with premultiplied alpha.
    var sum = vec4f(0.0);
    for (var i = 0u; i < count; i = i + 1u) {
        let t = (f32(i) + 0.5) / f32(count);
        let sample = layer_sample(
            pixel,
            mix(uniforms.motion_from.xy, uniforms.motion_to.xy, t),
            mix(uniforms.motion_from.zw, uniforms.motion_to.zw, t),
            mix(uniforms.motion.x, uniforms.motion.y, t),
            uv_per_pixel,
        );
        sum = sum + vec4f(sample.rgb * sample.a, sample.a);
    }
    let average = sum / f32(count);
    if (average.a <= 0.0) {
        return vec4f(0.0, 0.0, 0.0, 0.0);
    }
    return vec4f(average.rgb / average.a, average.a);
}
