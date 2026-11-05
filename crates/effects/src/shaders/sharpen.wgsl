// Sharpen (unsharp mask on the four neighbours). s0.x = amount, 0 leaves
// the picture alone and 1 is a strong edge boost.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let amount = uniforms.s0.x;
    let texel = vec2f(1.0) / uniforms.resolution;
    let centre = textureSample(input_texture, input_sampler, input.tex_coord);
    let left = sample_clamped(input.tex_coord - vec2f(texel.x, 0.0));
    let right = sample_clamped(input.tex_coord + vec2f(texel.x, 0.0));
    let up = sample_clamped(input.tex_coord - vec2f(0.0, texel.y));
    let down = sample_clamped(input.tex_coord + vec2f(0.0, texel.y));
    let neighbours = (left.rgb + right.rgb + up.rgb + down.rgb) * 0.25;
    let rgb = centre.rgb + (centre.rgb - neighbours) * amount * 2.0;
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), centre.a);
}
