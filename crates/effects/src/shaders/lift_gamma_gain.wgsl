// Three-way colour: lift (shadows), gamma (midtones) and gain
// (highlights), each a master amount plus red, green and blue offsets.
// s0 = (lift, lift red, lift green, lift blue), s1 = (gamma, gamma red,
// gamma green, gamma blue), s2 = (gain, gain red, gain green, gain blue).
// Lift 0, gamma 1, gain 1 with zero offsets leaves the picture alone.
// out = (in * gain + lift * (1 - in)) ^ (1 / gamma), per channel.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let lift = vec3f(uniforms.s0.x) + uniforms.s0.yzw;
    let gamma = max(vec3f(uniforms.s1.x) + uniforms.s1.yzw, vec3f(0.05));
    let gain = max(vec3f(uniforms.s2.x) + uniforms.s2.yzw, vec3f(0.0));
    var rgb = clamp(source.rgb, vec3f(0.0), vec3f(1.0));
    rgb = rgb * gain + lift * (vec3f(1.0) - rgb);
    rgb = pow(clamp(rgb, vec3f(0.0), vec3f(1.0)), vec3f(1.0) / gamma);
    return vec4f(rgb, source.a);
}
