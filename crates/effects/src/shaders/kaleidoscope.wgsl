// Kaleidoscope. s0 = (segments, rotation in radians). The frame is cut
// into wedges around its centre and each wedge mirrors the first.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let segments = max(uniforms.s0.x, 1.0);
    let rotation = uniforms.s0.y;
    let aspect = uniforms.resolution.x / max(uniforms.resolution.y, 1.0);
    var centred = input.tex_coord - vec2f(0.5);
    centred.x = centred.x * aspect;
    let radius = length(centred);
    let wedge = 6.28318530718 / segments;
    var angle = atan2(centred.y, centred.x) + rotation;
    angle = angle - wedge * floor(angle / wedge);
    angle = abs(angle - wedge * 0.5);
    var uv = vec2f(cos(angle), sin(angle)) * radius;
    uv.x = uv.x / aspect;
    uv = uv + vec2f(0.5);
    // Mirror-repeat so wedges that reach past the frame fold back in.
    uv = abs(fract(uv * 0.5) * 2.0 - vec2f(1.0));
    return textureSample(input_texture, input_sampler, uv);
}
