// HSL: hue, saturation and lightness per colour band. Eight bands sit at
// red 0°, orange 30°, yellow 60°, green 120°, aqua 180°, blue 240°, purple
// 270° and magenta 300°; a pixel's hue blends its two neighbouring bands
// with a smoothstep, so the eight values per control are also the curve
// hue vs hue, hue vs saturation and hue vs lightness.
// s0, s1 = hue shift per band (-100 to 100 = -30° to 30°),
// s2, s3 = saturation (-100 to 100: -100 is grey, 100 doubles it),
// s4, s5 = lightness (-100 to 100: towards black or white, scaled by the
// pixel's chroma so greys are left alone).

const HSL_HUE_RANGE: f32 = 30.0;

fn hsl_centre(index: u32) -> f32 {
    var centres = array<f32, 9>(0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0, 360.0);
    return centres[index];
}

// A control's value at `hue` (degrees, 0 to 360) from its eight band values.
fn hsl_band(values: array<f32, 8>, hue: f32) -> f32 {
    var bands = values;
    var index: u32 = 7u;
    for (var k: u32 = 0u; k < 7u; k = k + 1u) {
        if (hue >= hsl_centre(k) && hue < hsl_centre(k + 1u)) {
            index = k;
        }
    }
    let start = hsl_centre(index);
    let end = hsl_centre(index + 1u);
    let t = clamp((hue - start) / (end - start), 0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    return bands[index] * (1.0 - w) + bands[(index + 1u) % 8u] * w;
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> vec3f {
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - abs(hp - 2.0 * floor(hp / 2.0) - 1.0));
    let m = l - c * 0.5;
    var rgb: vec3f;
    if (hp < 1.0) {
        rgb = vec3f(c, x, 0.0);
    } else if (hp < 2.0) {
        rgb = vec3f(x, c, 0.0);
    } else if (hp < 3.0) {
        rgb = vec3f(0.0, c, x);
    } else if (hp < 4.0) {
        rgb = vec3f(0.0, x, c);
    } else if (hp < 5.0) {
        rgb = vec3f(x, 0.0, c);
    } else {
        rgb = vec3f(c, 0.0, x);
    }
    return clamp(rgb + vec3f(m), vec3f(0.0), vec3f(1.0));
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let rgb = clamp(source.rgb, vec3f(0.0), vec3f(1.0));
    let high = max(rgb.r, max(rgb.g, rgb.b));
    let low = min(rgb.r, min(rgb.g, rgb.b));
    let chroma = high - low;
    if (chroma <= 0.0) {
        return source;
    }
    let l = (high + low) * 0.5;
    let s = chroma / max(1.0 - abs(2.0 * l - 1.0), 1e-6);
    var h: f32;
    if (high == rgb.r) {
        h = (rgb.g - rgb.b) / chroma;
        if (h < 0.0) {
            h = h + 6.0;
        }
    } else if (high == rgb.g) {
        h = (rgb.b - rgb.r) / chroma + 2.0;
    } else {
        h = (rgb.r - rgb.g) / chroma + 4.0;
    }
    h = h * 60.0;

    let u = uniforms;
    let hues = array<f32, 8>(u.s0.x, u.s0.y, u.s0.z, u.s0.w, u.s1.x, u.s1.y, u.s1.z, u.s1.w);
    let sats = array<f32, 8>(u.s2.x, u.s2.y, u.s2.z, u.s2.w, u.s3.x, u.s3.y, u.s3.z, u.s3.w);
    let lights = array<f32, 8>(u.s4.x, u.s4.y, u.s4.z, u.s4.w, u.s5.x, u.s5.y, u.s5.z, u.s5.w);

    var h2 = h + hsl_band(hues, h) / 100.0 * HSL_HUE_RANGE;
    if (h2 < 0.0) {
        h2 = h2 + 360.0;
    }
    if (h2 >= 360.0) {
        h2 = h2 - 360.0;
    }
    let s2 = clamp(s * (1.0 + hsl_band(sats, h) / 100.0), 0.0, 1.0);
    let dl = hsl_band(lights, h) / 100.0 * chroma;
    var l2 = l;
    if (dl > 0.0) {
        l2 = l + dl * (1.0 - l);
    } else {
        l2 = l + dl * l;
    }
    return vec4f(hsl_to_rgb(h2, s2, clamp(l2, 0.0, 1.0)), source.a);
}
