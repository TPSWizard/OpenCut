// Colour correction. s0 = (brightness, contrast, saturation, hue radians),
// s1 = (exposure stops, gamma, temperature, tint). Every control at 0
// (gamma 1) leaves the picture alone. Alpha is kept.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let brightness = uniforms.s0.x;
    let contrast = uniforms.s0.y;
    let saturation = uniforms.s0.z;
    let hue = uniforms.s0.w;
    let exposure = uniforms.s1.x;
    let gamma = max(uniforms.s1.y, 0.05);
    let temperature = uniforms.s1.z;
    let tint = uniforms.s1.w;

    var rgb = source.rgb;
    // Exposure in stops, then an additive brightness.
    rgb = rgb * exp2(exposure) + vec3f(brightness);
    // Contrast pivots on mid grey.
    rgb = (rgb - vec3f(0.5)) * (1.0 + contrast) + vec3f(0.5);
    // Warmth pushes red against blue; tint pushes green against magenta.
    rgb = rgb + vec3f(temperature * 0.1, tint * 0.1, -temperature * 0.1);
    // Saturation scales the distance from the pixel's own luma.
    let luma = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));
    rgb = mix(vec3f(luma), rgb, 1.0 + saturation);
    // Hue: rotate around the grey axis (Rodrigues' formula).
    let axis = vec3f(0.57735027);
    let cos_h = cos(hue);
    let sin_h = sin(hue);
    rgb = rgb * cos_h + cross(axis, rgb) * sin_h + axis * dot(axis, rgb) * (1.0 - cos_h);
    // Gamma last, on the clamped result.
    rgb = pow(clamp(rgb, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / gamma));
    return vec4f(rgb, source.a);
}
