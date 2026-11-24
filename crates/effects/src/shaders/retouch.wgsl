// Retouch: smooths, brightens and evens skin. A pixel's skin weight comes
// from its chroma (an ellipse around skin tones in Cb/Cr, faded out in the
// dark); an edge-preserving (bilateral) blur over the centre and two rings
// of eight taps, `h` and `2h` pixels out, smooths it; the blur's colour
// evens out blotches while the pixel keeps its own light and shade; skin
// is lifted a little. s0 = (smooth, brighten, even, h). Pixels that are
// not skin are left exactly as they are. Checked against
// `fixtures/retouch/cases.json`.

const SKIN_CB: f32 = 0.42;
const SKIN_CR: f32 = 0.60;
const SKIN_RADIUS_CB: f32 = 0.10;
const SKIN_RADIUS_CR: f32 = 0.08;
// exp(-|difference|^2 * RANGE): colours further apart count less.
const RANGE: f32 = 30.0;
const LIFT: f32 = 0.15;

fn to_ycbcr(c: vec3f) -> vec3f {
    return vec3f(
        0.299 * c.r + 0.587 * c.g + 0.114 * c.b,
        0.5 - 0.168736 * c.r - 0.331264 * c.g + 0.5 * c.b,
        0.5 + 0.5 * c.r - 0.418688 * c.g - 0.081312 * c.b,
    );
}

fn from_ycbcr(y: vec3f) -> vec3f {
    let cb = y.y - 0.5;
    let cr = y.z - 0.5;
    return vec3f(y.x + 1.402 * cr, y.x - 0.344136 * cb - 0.714136 * cr, y.x + 1.772 * cb);
}

fn skin_weight(c: vec4f) -> f32 {
    let y = to_ycbcr(c.rgb);
    let d = length(vec2f((y.y - SKIN_CB) / SKIN_RADIUS_CB, (y.z - SKIN_CR) / SKIN_RADIUS_CR));
    return (1.0 - smoothstep(0.7, 1.0, d)) * smoothstep(0.08, 0.2, y.x) * c.a;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let smooth_amount = uniforms.s0.x;
    let brighten = uniforms.s0.y;
    let even = uniforms.s0.z;
    let h = max(1.0, round(uniforms.s0.w));
    let centre = textureSample(input_texture, input_sampler, input.tex_coord);
    let w = skin_weight(centre);
    if (w <= 0.0) {
        return centre;
    }
    let texel = vec2f(1.0) / uniforms.resolution;
    var dirs = array<vec2f, 8>(
        vec2f(1.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0), vec2f(-1.0, 1.0),
        vec2f(-1.0, 0.0), vec2f(-1.0, -1.0), vec2f(0.0, -1.0), vec2f(1.0, -1.0),
    );
    var total = centre.rgb;
    var weights = 1.0;
    for (var ring = 1; ring <= 2; ring = ring + 1) {
        let reach = h * f32(ring);
        for (var k = 0; k < 8; k = k + 1) {
            let offset = dirs[k] * reach;
            // Level 0 explicitly: the loop runs after a per-pixel branch.
            let tap = textureSampleLevel(input_texture, input_sampler, clamp(input.tex_coord + offset * texel, vec2f(0.0), vec2f(1.0)), 0.0);
            let spread = length(offset) / (2.0 * h);
            let difference = tap.rgb - centre.rgb;
            let weight = exp(-spread * spread) * exp(-dot(difference, difference) * RANGE);
            total = total + tap.rgb * weight;
            weights = weights + weight;
        }
    }
    let blurred = total / weights;
    // Smooth: towards the blur.
    let smoothed = mix(centre.rgb, blurred, smooth_amount * w);
    // Even: the blur's colour under the pixel's own light and shade.
    var ycc = to_ycbcr(smoothed);
    let blurred_ycc = to_ycbcr(blurred);
    ycc = vec3f(ycc.x, mix(ycc.yz, blurred_ycc.yz, even * w));
    // Brighten: a gentle lift.
    ycc.x = ycc.x + brighten * w * LIFT * (1.0 - ycc.x);
    return vec4f(clamp(from_ycbcr(ycc), vec3f(0.0), vec3f(1.0)), centre.a);
}
