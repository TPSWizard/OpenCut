// Separable Gaussian blur. s0.x = sigma in pixels, s0.y = sample step,
// direction = (1, 0) or (0, 1).
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let texel_size = vec2f(1.0, 1.0) / uniforms.resolution;
    let sigma = uniforms.s0.x;
    let step_size = uniforms.s0.y;

    var color = vec4f(0.0, 0.0, 0.0, 0.0);
    var total_weight = 0.0;

    for (var index = -30; index <= 30; index = index + 1) {
        let position = f32(index) * step_size;
        let weight = exp(-(position * position) / (2.0 * sigma * sigma));
        let sample_uv = input.tex_coord + (texel_size * uniforms.direction * position);
        color = color + textureSample(input_texture, input_sampler, sample_uv) * weight;
        total_weight = total_weight + weight;
    }

    return color / total_weight;
}
