// Glitch. s0 = (amount 0..1, horizontal bands, time in seconds, seed).
// Some bands, chosen afresh a dozen times a second, slide sideways and
// tear their colour channels apart; the rest pass through.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let amount = uniforms.s0.x;
    let bands = max(uniforms.s0.y, 1.0);
    let time = uniforms.s0.z;
    let seed = uniforms.s0.w;
    let tick = floor(time * 12.0);
    let band = floor(input.tex_coord.y * bands);
    let pick = hash2(vec2f(band + seed, tick));
    var uv = input.tex_coord;
    var split = 0.0;
    if (pick < amount * 0.6) {
        let shift = (hash2(vec2f(tick, band - seed)) - 0.5) * amount * 0.25;
        uv.x = fract(uv.x + shift);
        split = amount * 8.0 / uniforms.resolution.x;
    }
    let red = sample_clamped(uv + vec2f(split, 0.0));
    let green = textureSample(input_texture, input_sampler, uv);
    let blue = sample_clamped(uv - vec2f(split, 0.0));
    return vec4f(red.r, green.g, blue.b, max(green.a, max(red.a, blue.a)));
}
