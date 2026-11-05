// Vignette. s0 = (amount 0..1, radius 0..1.5 where the darkening starts,
// softness 0..1 over which it ramps). Distance is measured so the frame's
// corners sit near 1.41 and the centre at 0.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let amount = uniforms.s0.x;
    let radius = uniforms.s0.y;
    let softness = max(uniforms.s0.z, 0.001);
    let centred = (input.tex_coord - vec2f(0.5)) * 2.0;
    let distance_from_centre = length(centred);
    let shade = smoothstep(radius, radius + softness, distance_from_centre);
    return vec4f(source.rgb * (1.0 - amount * shade), source.a);
}
