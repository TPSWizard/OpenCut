// Levels: input black and white points, a midtone gamma, and output
// black and white points, like the classic Levels dialog.
// s0 = (input black, input white, gamma, output black), s1 = (output
// white, unused, unused, unused). Black 0, white 1, gamma 1 leaves the
// picture alone.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let in_black = uniforms.s0.x;
    let in_white = max(uniforms.s0.y, in_black + 0.001);
    let gamma = max(uniforms.s0.z, 0.01);
    let out_black = uniforms.s0.w;
    let out_white = uniforms.s1.x;
    var rgb = clamp((source.rgb - vec3f(in_black)) / (in_white - in_black), vec3f(0.0), vec3f(1.0));
    rgb = pow(rgb, vec3f(1.0 / gamma));
    rgb = vec3f(out_black) + rgb * (out_white - out_black);
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), source.a);
}
