// Bloom, first pass: keeps what is brighter than the threshold. s0 =
// (threshold 0..1, knee 0..1). A pixel's brightest channel ramps its share
// in over threshold - knee .. threshold + knee (smoothstep). The output is
// premultiplied by the pixel's alpha and opaque, ready for the blur.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let rgb = clamp(source.rgb, vec3f(0.0), vec3f(1.0));
    let threshold = uniforms.s0.x;
    let knee = max(uniforms.s0.y, 0.001);
    let bright = max(rgb.r, max(rgb.g, rgb.b));
    let share = smoothstep(threshold - knee, threshold + knee, bright);
    return vec4f(rgb * share * source.a, 1.0);
}
