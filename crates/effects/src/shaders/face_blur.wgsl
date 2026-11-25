// Face blur: hides the faces of a clip for privacy. s0 = (style,
// strength, size, feather); s1, s2, s3 = up to three face ovals (centre x,
// y and radii in frame pixels, from the clip's face track:
// `crates/facetrack`, the same ovals face-only Retouch uses). Each oval is
// grown by `size`; inside it the picture is hidden at a reach of
// `strength` x half the grown oval's smaller radius (so near and far faces
// are hidden alike; the oval covering a pixel most sets it): blurred
// (style 0: two passes, across then down (`direction`), each a Gaussian
// with sigma half the reach, out to the reach each way), pixelated (style 1: blocks the reach wide on the
// frame's grid, each the mean of 4 x 4 whole-pixel taps) or painted black
// (style 2), fading out over the outer `feather` of the oval. Pixels
// outside every oval are left exactly as they are. Checked against
// `fixtures/face-blur/cases.json`.

fn face_cover(pixel: vec2f, oval: vec4f, size: f32, feather: f32) -> f32 {
    if (oval.z <= 0.0 || oval.w <= 0.0) {
        return 0.0;
    }
    let d = length((pixel - oval.xy) / (oval.zw * size));
    return 1.0 - smoothstep(1.0 - max(feather, 0.001), 1.0, d);
}

fn tap(pixel: vec2f) -> vec4f {
    let uv = clamp(pixel / uniforms.resolution, vec2f(0.0), vec2f(1.0));
    // Level 0 explicitly: the loops run after a per-pixel branch.
    return textureSampleLevel(input_texture, input_sampler, uv, 0.0);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let style = round(uniforms.s0.x);
    let strength = clamp(uniforms.s0.y, 0.0, 2.0);
    let size = max(0.1, uniforms.s0.z);
    let feather = clamp(uniforms.s0.w, 0.0, 1.0);
    let centre = textureSample(input_texture, input_sampler, input.tex_coord);
    let pixel = input.tex_coord * uniforms.resolution;
    var cover = face_cover(pixel, uniforms.s1, size, feather);
    var oval = uniforms.s1;
    let cover2 = face_cover(pixel, uniforms.s2, size, feather);
    if (cover2 > cover) {
        cover = cover2;
        oval = uniforms.s2;
    }
    let cover3 = face_cover(pixel, uniforms.s3, size, feather);
    if (cover3 > cover) {
        cover = cover3;
        oval = uniforms.s3;
    }
    if (cover <= 0.0) {
        return centre;
    }
    let amount = max(1.0, strength * 0.5 * min(oval.z, oval.w) * size);
    var hidden = vec3f(0.0);
    if (style < 0.5) {
        // One direction of a separable Gaussian (sigma = reach / 2) out to
        // the reach each way, at most 32 filtered taps a side.
        let side = min(ceil(amount), 32.0);
        var total = vec3f(0.0);
        var weights = 0.0;
        for (var k = -32; k <= 32; k = k + 1) {
            if (abs(f32(k)) > side) {
                continue;
            }
            let offset = f32(k) * amount / side;
            let weight = exp(-2.0 * offset * offset / (amount * amount));
            total = total + tap(pixel + uniforms.direction * offset).rgb * weight;
            weights = weights + weight;
        }
        hidden = total / weights;
    } else if (style < 1.5) {
        let block = max(2.0, round(amount));
        let origin = floor(pixel / block) * block;
        var total = vec3f(0.0);
        for (var i = 0; i < 4; i = i + 1) {
            for (var j = 0; j < 4; j = j + 1) {
                let inside = floor(vec2f(f32(i), f32(j)) * block * 0.25) + vec2f(0.5);
                total = total + tap(origin + inside).rgb;
            }
        }
        hidden = total / 16.0;
    }
    return vec4f(mix(centre.rgb, hidden, cover), centre.a);
}
