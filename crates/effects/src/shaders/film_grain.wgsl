// Film grain. s0 = (amount 0..1, grain size in frame pixels, time in
// seconds, seed). The noise is hashed per grain cell and changes every
// frame, scaled by the pixel's brightness so blacks and whites stay calm.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let amount = uniforms.s0.x;
    let grain = max(uniforms.s0.y, 1.0);
    let time = uniforms.s0.z;
    let seed = uniforms.s0.w;
    let cell = floor(input.position.xy / grain);
    let frame = floor(time * 24.0);
    let noise = hash2(cell + vec2f(frame * 17.0 + seed, frame * 7.0 - seed)) - 0.5;
    let luma = dot(source.rgb, vec3f(0.2126, 0.7152, 0.0722));
    let weight = 1.0 - abs(luma * 2.0 - 1.0) * 0.6;
    let rgb = source.rgb + vec3f(noise * amount * 0.35 * weight);
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), source.a);
}
