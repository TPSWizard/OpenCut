// Shadows and highlights: brightens or darkens the dark and the bright
// parts of the picture separately, weighted by each pixel's luma.
// s0 = (shadows, highlights, range, unused). Shadows and highlights run
// from -1 (darken) to 1 (brighten); range widens the band of tones each
// one reaches into (0 narrow, 1 wide). Both at 0 leave the picture alone.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let shadows = uniforms.s0.x;
    let highlights = uniforms.s0.y;
    let range = clamp(uniforms.s0.z, 0.0, 1.0);
    let rgb = clamp(source.rgb, vec3f(0.0), vec3f(1.0));
    let luma = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));
    // How far up the tone scale the shadow adjustment reaches, and how far
    // down the highlight one does.
    let reach = mix(0.25, 0.75, range);
    let shadow_weight = 1.0 - smoothstep(0.0, reach, luma);
    let highlight_weight = smoothstep(1.0 - reach, 1.0, luma);
    // Brightening shadows lifts towards white; darkening highlights pulls
    // towards black, each scaled by how much room the pixel has.
    var out = rgb;
    out = out + shadows * shadow_weight * select(out, vec3f(1.0) - out, shadows > 0.0) * 0.6;
    out = out + highlights * highlight_weight * select(out, vec3f(1.0) - out, highlights > 0.0) * 0.6;
    return vec4f(clamp(out, vec3f(0.0), vec3f(1.0)), source.a);
}
