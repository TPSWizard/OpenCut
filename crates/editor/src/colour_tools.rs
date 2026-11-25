//! Colour tools both editors draw controls for: the HSL effect's eight
//! hue bands (whose values are also the hue curves) and colour wheels
//! over lift / gamma / gain. The web app's `effects/hsl.ts` and
//! `effects/colour-wheel.ts` match these, checked against
//! `fixtures/hsl/cases.json`.

/// The HSL effect's bands: key prefix, label and centre hue (degrees).
pub const HSL_BANDS: [(&str, &str, f64); 8] = [
    ("red", "Red", 0.0),
    ("orange", "Orange", 30.0),
    ("yellow", "Yellow", 60.0),
    ("green", "Green", 120.0),
    ("aqua", "Aqua", 180.0),
    ("blue", "Blue", 240.0),
    ("purple", "Purple", 270.0),
    ("magenta", "Magenta", 300.0),
];

/// The three things each band moves, as the HSL effect's param key suffixes.
pub const HSL_CONTROLS: [(&str, &str); 3] = [("Hue", "Hue"), ("Saturation", "Saturation"), ("Lightness", "Lightness")];

/// A full hue shift (100) turns the hue this many degrees.
pub const HSL_HUE_RANGE: f64 = 30.0;

/// The HSL param key of a band's control, e.g. `redHue`, `aquaLightness`.
pub fn hsl_param_key(band: usize, control: usize) -> String {
    format!("{}{}", HSL_BANDS[band].0, HSL_CONTROLS[control].0)
}

/// A control's value at `hue` degrees from its eight band values: the two
/// neighbouring bands blended with a smoothstep, as the shader does. This
/// is the hue curve the panels draw.
pub fn hsl_band_value(values: &[f64; 8], hue: f64) -> f64 {
    let hue = hue.rem_euclid(360.0);
    let centre = |index: usize| if index == 8 { 360.0 } else { HSL_BANDS[index].2 };
    let mut index = 7;
    for k in 0..7 {
        if hue >= centre(k) && hue < centre(k + 1) {
            index = k;
        }
    }
    let (start, end) = (centre(index), centre(index + 1));
    let t = ((hue - start) / (end - start)).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    values[index] * (1.0 - w) + values[(index + 1) % 8] * w
}

/// The colour a band shows in a swatch: its centre hue, fully saturated.
pub fn hsl_band_swatch(band: usize) -> [u8; 3] {
    hue_rgb(HSL_BANDS[band].2)
}

fn hue_rgb(hue: f64) -> [u8; 3] {
    let hp = hue.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (hp % 2.0 - 1.0).abs();
    let (r, g, b) = match hp as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * 255.0f64).round() as u8, (g * 255.0f64).round() as u8, (b * 255.0f64).round() as u8]
}

/// How far a colour wheel's rim moves a channel: a dot on the rim adds this
/// much to one channel's lift, gamma or gain offset (the others go down).
pub const COLOUR_WHEEL_RANGE: f64 = 0.5;
/// Where red, green and blue sit on a colour wheel (degrees anticlockwise
/// from the right), close to where the vectorscope shows them.
pub const COLOUR_WHEEL_ANGLES: [f64; 3] = [105.0, 225.0, 345.0];

/// The red, green and blue offsets of a wheel's dot at `(x, y)` (the unit
/// disc, y up; outside it is pulled onto the rim). They sum to zero, so the
/// wheel tints without brightening.
pub fn colour_wheel_offsets(x: f64, y: f64) -> [f64; 3] {
    let mut magnitude = x.hypot(y);
    let theta = y.atan2(x);
    if magnitude > 1.0 {
        magnitude = 1.0;
    }
    COLOUR_WHEEL_ANGLES.map(|angle| magnitude * (theta - angle.to_radians()).cos() * COLOUR_WHEEL_RANGE)
}

/// Where a wheel's dot sits for these offsets (their shared part, which
/// only brightens, is left out).
pub fn colour_wheel_point(offsets: [f64; 3]) -> (f64, f64) {
    let mean = offsets.iter().sum::<f64>() / 3.0;
    let (mut x, mut y) = (0.0, 0.0);
    for (offset, angle) in offsets.iter().zip(COLOUR_WHEEL_ANGLES) {
        x += (offset - mean) * angle.to_radians().cos();
        y += (offset - mean) * angle.to_radians().sin();
    }
    (2.0 / 3.0 * x / COLOUR_WHEEL_RANGE, 2.0 / 3.0 * y / COLOUR_WHEEL_RANGE)
}

/// New offsets for a dot moved to `(x, y)`, keeping the old offsets'
/// shared part.
pub fn colour_wheel_move(old: [f64; 3], x: f64, y: f64) -> [f64; 3] {
    let mean = old.iter().sum::<f64>() / 3.0;
    colour_wheel_offsets(x, y).map(|offset| offset + mean)
}

/// The lift / gamma / gain params a wheel moves: `(label, red, green, blue)`.
pub const COLOUR_WHEELS: [(&str, [&str; 3]); 3] = [
    ("Lift", ["liftRed", "liftGreen", "liftBlue"]),
    ("Gamma", ["gammaRed", "gammaGreen", "gammaBlue"]),
    ("Gain", ["gainRed", "gainGreen", "gainBlue"]),
];

/// A `size` x `size` RGBA picture of a wheel: each point shows mid grey
/// tinted by the offsets a dot there gives; transparent outside the disc.
pub fn colour_wheel_rgba(size: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; size * size * 4];
    let half = size as f64 / 2.0;
    for row in 0..size {
        for column in 0..size {
            let x = (column as f64 + 0.5 - half) / half;
            let y = (half - row as f64 - 0.5) / half;
            let distance = x.hypot(y);
            if distance > 1.0 {
                continue;
            }
            let offsets = colour_wheel_offsets(x, y);
            let index = (row * size + column) * 4;
            for channel in 0..3 {
                rgba[index + channel] = ((0.5 + offsets[channel]).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            // Soften the rim over a pixel.
            rgba[index + 3] = (((1.0 - distance) * half).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/hsl/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn hue_curves_match_the_reference() {
        let fixture = fixture();
        let values: Vec<f64> = fixture["curve"]["values"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let values: [f64; 8] = values.try_into().unwrap();
        for point in fixture["curve"]["points"].as_array().unwrap() {
            let hue = point["hue"].as_f64().unwrap();
            assert!((hsl_band_value(&values, hue) - point["value"].as_f64().unwrap()).abs() < 1e-9, "{hue}");
        }
        let bands: Vec<&str> = fixture["bands"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
        assert_eq!(bands, HSL_BANDS.map(|band| band.0));
        assert_eq!(hsl_param_key(4, 2), "aquaLightness");
    }

    #[test]
    fn colour_wheels_match_the_reference() {
        let fixture = fixture();
        assert_eq!(fixture["wheelRange"].as_f64().unwrap(), COLOUR_WHEEL_RANGE);
        for wheel in fixture["wheels"].as_array().unwrap() {
            let offsets: Vec<f64> = wheel["offsets"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            if let Some(point) = wheel["point"].as_array() {
                let (x, y) = (point[0].as_f64().unwrap(), point[1].as_f64().unwrap());
                let got = colour_wheel_offsets(x, y);
                for channel in 0..3 {
                    assert!((got[channel] - offsets[channel]).abs() < 1e-12, "{wheel}");
                }
            }
            let back = wheel["back"].as_array().unwrap();
            let (x, y) = colour_wheel_point([offsets[0], offsets[1], offsets[2]]);
            assert!((x - back[0].as_f64().unwrap()).abs() < 1e-12 && (y - back[1].as_f64().unwrap()).abs() < 1e-12, "{wheel}");
        }
        // Moving keeps the shared part.
        let moved = colour_wheel_move([0.3, 0.1, 0.1], 0.0, 0.0);
        for value in moved {
            assert!((value - 0.5 / 3.0).abs() < 1e-12);
        }
        assert_eq!(hsl_band_swatch(0), [255, 0, 0]);
        assert_eq!(hsl_band_swatch(4), [0, 255, 255]);
        let picture = colour_wheel_rgba(32);
        assert_eq!(picture.len(), 32 * 32 * 4);
        assert_eq!(picture[3], 0, "outside the disc is clear");
    }
}
