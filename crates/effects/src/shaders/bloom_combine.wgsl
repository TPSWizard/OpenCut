// Bloom, last pass: adds the blurred bright parts (this pass's input) onto
// the frame as it was before the effect (bound as the pass's table,
// `@source`). s0 = (intensity 0..2, tint red, tint green, tint blue).
// The glow may reach past the frame's opaque parts, so it is added in
// premultiplied terms and the alpha grows to hold it.
@group(2) @binding(0) var original_texture: texture_2d<f32>;
@group(2) @binding(1) var original_sampler: sampler;

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let glow = textureSample(input_texture, input_sampler, input.tex_coord).rgb;
    let base = textureSample(original_texture, original_sampler, input.tex_coord);
    let add = glow * vec3f(uniforms.s0.y, uniforms.s0.z, uniforms.s0.w) * uniforms.s0.x;
    let premultiplied = min(base.rgb * base.a + add, vec3f(1.0));
    let alpha = max(base.a, min(max(add.r, max(add.g, add.b)), 1.0));
    if (alpha <= 0.0) {
        return vec4f(0.0);
    }
    return vec4f(min(premultiplied / alpha, vec3f(1.0)), alpha);
}
