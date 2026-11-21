// Chroma key (green or blue screen). s0 = (key red, key green, key blue,
// strength 0..1), s1 = (edge softness 0..1, spill removal 0..1).
//
// Works on the chroma (Cb, Cr in BT.709) so brightness changes across the
// screen matter less than hue. The distance from the key's chroma is
// measured in units of the key's own saturation: 0 at the key, about 1 at
// grey, 2 at the opposite hue. Pixels closer than `strength` turn
// transparent, with `softness` as the ramp. Spill removal takes out the
// part of each pixel's chroma that points towards the key, so a green
// fringe on hair and edges turns neutral. Output is straight alpha.
fn rgb_to_ycbcr(rgb: vec3f) -> vec3f {
    let y = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));
    let cb = dot(rgb, vec3f(-0.114572, -0.385428, 0.5));
    let cr = dot(rgb, vec3f(0.5, -0.454153, -0.045847));
    return vec3f(y, cb, cr);
}

fn ycbcr_to_rgb(ycc: vec3f) -> vec3f {
    let y = ycc.x;
    let cb = ycc.y;
    let cr = ycc.z;
    return vec3f(y + 1.5748 * cr, y - 0.187324 * cb - 0.468124 * cr, y + 1.8556 * cb);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let key = rgb_to_ycbcr(uniforms.s0.xyz).yz;
    let strength = clamp(uniforms.s0.w, 0.0, 2.0);
    let softness = max(uniforms.s1.x, 0.001);
    let spill = clamp(uniforms.s1.y, 0.0, 1.0);
    let key_length = max(length(key), 0.05);

    let pixel = rgb_to_ycbcr(source.rgb);
    let distance_to_key = length(pixel.yz - key) / key_length;
    let keep = smoothstep(strength, strength + softness, distance_to_key);

    let direction = key / key_length;
    let towards = max(dot(pixel.yz, direction), 0.0);
    let chroma = pixel.yz - direction * towards * spill;
    let rgb = clamp(ycbcr_to_rgb(vec3f(pixel.x, chroma)), vec3f(0.0), vec3f(1.0));
    return vec4f(rgb, source.a * keep);
}
