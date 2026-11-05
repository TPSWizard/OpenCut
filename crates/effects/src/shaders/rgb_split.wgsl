// RGB split (chromatic aberration). s0 = (offset in frame pixels, angle
// in radians). Red moves one way, blue the other, green stays.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let offset_px = uniforms.s0.x;
    let angle = uniforms.s0.y;
    let offset = vec2f(cos(angle), sin(angle)) * offset_px / uniforms.resolution;
    let red = sample_clamped(input.tex_coord + offset);
    let green = textureSample(input_texture, input_sampler, input.tex_coord);
    let blue = sample_clamped(input.tex_coord - offset);
    let alpha = max(green.a, max(red.a, blue.a));
    return vec4f(red.r, green.g, blue.b, alpha);
}
