// Tone curves: a master curve through five points at 0, 25, 50, 75 and
// 100 % input, and a curve per channel through three points at 25, 50
// and 75 % (its ends pinned to 0 and 1). The channel curves apply first,
// then the master curve. s0 = master (0 %, 25 %, 50 %, 75 %),
// s1 = (master 100 %, red 25 %, red 50 %, red 75 %),
// s2 = (green 25 %, green 50 %, green 75 %, blue 25 %),
// s3 = (blue 50 %, blue 75 %, unused, unused). A straight line
// (0, .25, .5, .75, 1) leaves the picture alone.
// Catmull-Rom between the points, with the ends extended, then clamped.

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5 * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
}

// A curve through y0..y4 at x = 0, .25, .5, .75, 1.
fn curve5(x: f32, y0: f32, y1: f32, y2: f32, y3: f32, y4: f32) -> f32 {
    let xc = clamp(x, 0.0, 1.0) * 4.0;
    let segment = min(floor(xc), 3.0);
    let t = xc - segment;
    // Extend the ends by mirroring the first and last slopes.
    let ym1 = 2.0 * y0 - y1;
    let y5 = 2.0 * y4 - y3;
    var p0: f32;
    var p1: f32;
    var p2: f32;
    var p3: f32;
    if (segment < 0.5) {
        p0 = ym1; p1 = y0; p2 = y1; p3 = y2;
    } else if (segment < 1.5) {
        p0 = y0; p1 = y1; p2 = y2; p3 = y3;
    } else if (segment < 2.5) {
        p0 = y1; p1 = y2; p2 = y3; p3 = y4;
    } else {
        p0 = y2; p1 = y3; p2 = y4; p3 = y5;
    }
    return clamp(catmull(p0, p1, p2, p3, t), 0.0, 1.0);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let rgb = clamp(source.rgb, vec3f(0.0), vec3f(1.0));
    let r = curve5(rgb.r, 0.0, uniforms.s1.y, uniforms.s1.z, uniforms.s1.w, 1.0);
    let g = curve5(rgb.g, 0.0, uniforms.s2.x, uniforms.s2.y, uniforms.s2.z, 1.0);
    let b = curve5(rgb.b, 0.0, uniforms.s2.w, uniforms.s3.x, uniforms.s3.y, 1.0);
    let m0 = uniforms.s0.x;
    let m1 = uniforms.s0.y;
    let m2 = uniforms.s0.z;
    let m3 = uniforms.s0.w;
    let m4 = uniforms.s1.x;
    let out = vec3f(
        curve5(r, m0, m1, m2, m3, m4),
        curve5(g, m0, m1, m2, m3, m4),
        curve5(b, m0, m1, m2, m3, m4),
    );
    return vec4f(out, source.a);
}
