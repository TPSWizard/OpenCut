//! Curved text: a flat text raster (its glyphs, background box,
//! highlights and underlines, as each editor draws them) bent along a
//! circular arc. The desktop renderer bends its cosmic-text raster with
//! [`warp`]; the web editor bends its canvas text the same way through WASM,
//! so both editors draw the same curve.
//!
//! The curve is the `curve` text param, -100 to 100: the share of a turn
//! the raster's width wraps around (100 is [`MAX_TURN_DEGREES`]); positive
//! bends the line up like an arch (the ends drop), negative like a smile
//! (the ends rise). The raster's centre stays where it was, and the bent
//! raster is returned centred on it, so the text keeps its place. Lines
//! below the middle sit on smaller circles (an arch) or larger ones (a
//! smile), so letters stretch a little on the outside of the bend. Checked
//! against `fixtures/curved-text/cases.json` (an independent reference).

/// The bend at a curve of 100 (short of a full turn so the ends never meet).
pub const MAX_TURN_DEGREES: f64 = 350.0;
/// Curves closer to 0 than this are drawn flat.
pub const MIN_CURVE: f64 = 0.5;
/// Bounded so a wild size cannot ask for gigabytes.
pub const MAX_SIDE: u32 = 16_384;

/// The circle a raster `width` pixels wide is bent around.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    /// Pixels from the circle's centre to the raster's middle line.
    pub radius: f64,
    /// True for an arch (the centre below the text), false for a smile.
    pub up: bool,
}

impl Arc {
    /// The arc for a curve amount (-100 to 100) and raster width, or
    /// `None` when the text is drawn flat.
    pub fn new(curve: f64, width: f64) -> Option<Self> {
        if !curve.is_finite() || curve.abs() < MIN_CURVE || !(width >= 1.0) {
            return None;
        }
        let angle = (curve.abs() / 100.0).min(1.0) * MAX_TURN_DEGREES.to_radians();
        Some(Self { radius: width / angle, up: curve > 0.0 })
    }

    /// A flat point (pixels from the raster's centre, y down) on the arc.
    pub fn bend(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let r = self.radius;
        let angle = x / r;
        if self.up {
            let distance = r - y;
            [distance * angle.sin(), r - distance * angle.cos()]
        } else {
            let distance = r + y;
            [distance * angle.sin(), -r + distance * angle.cos()]
        }
    }

    /// The flat point that lands on a bent point (the inverse of [`bend`]).
    pub fn unbend(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let r = self.radius;
        if self.up {
            let (dx, dy) = (x, y - r);
            let angle = dx.atan2(-dy);
            [angle * r, r - (dx * dx + dy * dy).sqrt()]
        } else {
            let (dx, dy) = (x, y + r);
            let angle = dx.atan2(dy);
            [angle * r, (dx * dx + dy * dy).sqrt() - r]
        }
    }
}

/// The size of the bent raster, centred on the flat raster's centre, for a
/// flat raster `width` by `height`: the flat size when the curve is flat.
pub fn bent_size(curve: f64, width: u32, height: u32) -> (u32, u32) {
    let Some(arc) = Arc::new(curve, width as f64) else {
        return (width, height);
    };
    let (half_width, half_height) = (width as f64 / 2.0, height as f64 / 2.0);
    let (mut reach_x, mut reach_y) = (0.0f64, half_height);
    const STEPS: usize = 512;
    let mut take = |point: [f64; 2]| {
        // Parts of the raster past the circle's centre are not drawn.
        let inside = if arc.up { point[1] <= arc.radius } else { point[1] >= -arc.radius };
        if inside {
            let [x, y] = arc.bend(point);
            reach_x = reach_x.max(x.abs());
            reach_y = reach_y.max(y.abs());
        }
    };
    for step in 0..=STEPS {
        let t = step as f64 / STEPS as f64;
        let x = -half_width + t * width as f64;
        let y = -half_height + t * height as f64;
        take([x, -half_height]);
        take([x, half_height]);
        take([-half_width, y]);
        take([half_width, y]);
    }
    let side = |reach: f64| ((2.0 * (reach.ceil() + 1.0)) as u32).clamp(1, MAX_SIDE);
    (side(reach_x), side(reach_y))
}

/// A bent raster: 4 bytes a pixel in the flat raster's channel order.
#[derive(Clone, Debug, PartialEq)]
pub struct Bent {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Bends a raster of straight-alpha pixels (RGBA or BGRA: the fourth byte
/// is alpha) along the arc for `curve`. Each output pixel samples the flat
/// raster bilinearly, weighted by alpha so edges keep their colour.
/// Returns `None` when the curve is flat (draw the raster as it is).
pub fn warp(curve: f64, width: u32, height: u32, pixels: &[u8]) -> Option<Bent> {
    let arc = Arc::new(curve, width as f64)?;
    if pixels.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let (out_width, out_height) = bent_size(curve, width, height);
    let mut out = vec![0u8; out_width as usize * out_height as usize * 4];
    let (half_width, half_height) = (width as f64 / 2.0, height as f64 / 2.0);
    let (out_half_width, out_half_height) = (out_width as f64 / 2.0, out_height as f64 / 2.0);
    let texel = |x: i64, y: i64| -> [f64; 4] {
        if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
            return [0.0; 4];
        }
        let index = (y as usize * width as usize + x as usize) * 4;
        let alpha = pixels[index + 3] as f64;
        [pixels[index] as f64 * alpha, pixels[index + 1] as f64 * alpha, pixels[index + 2] as f64 * alpha, alpha]
    };
    for oy in 0..out_height {
        for ox in 0..out_width {
            let point = [ox as f64 + 0.5 - out_half_width, oy as f64 + 0.5 - out_half_height];
            let [fx, fy] = arc.unbend(point);
            // Flat pixel coordinates, with texel centres at .5.
            let sx = fx + half_width - 0.5;
            let sy = fy + half_height - 0.5;
            if sx < -1.0 || sy < -1.0 || sx > width as f64 || sy > height as f64 {
                continue;
            }
            let (x0, y0) = (sx.floor(), sy.floor());
            let (tx, ty) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let mut sum = [0.0f64; 4];
            for (dx, dy, weight) in [
                (0, 0, (1.0 - tx) * (1.0 - ty)),
                (1, 0, tx * (1.0 - ty)),
                (0, 1, (1.0 - tx) * ty),
                (1, 1, tx * ty),
            ] {
                if weight <= 0.0 {
                    continue;
                }
                let sample = texel(x0 + dx, y0 + dy);
                for (total, value) in sum.iter_mut().zip(sample) {
                    *total += value * weight;
                }
            }
            if sum[3] <= 0.0 {
                continue;
            }
            let index = (oy as usize * out_width as usize + ox as usize) * 4;
            out[index] = (sum[0] / sum[3]).round().clamp(0.0, 255.0) as u8;
            out[index + 1] = (sum[1] / sum[3]).round().clamp(0.0, 255.0) as u8;
            out[index + 2] = (sum[2] / sum[3]).round().clamp(0.0, 255.0) as u8;
            out[index + 3] = sum[3].round().clamp(0.0, 255.0) as u8;
        }
    }
    Some(Bent { width: out_width, height: out_height, pixels: out })
}

/// The `curve` text param, -100 to 100 (0 when missing or not a number).
pub fn read_curve(value: Option<f64>) -> f64 {
    match value {
        Some(value) if value.is_finite() => value.clamp(-100.0, 100.0),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/curved-text/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("cases file")).expect("cases json")
    }

    fn pair(value: &Value) -> [f64; 2] {
        [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
    }

    #[test]
    fn bends_points_like_the_reference() {
        for case in cases()["arcs"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let curve = case["curve"].as_f64().unwrap();
            let width = case["width"].as_f64().unwrap();
            let arc = Arc::new(curve, width);
            match case["radius"].as_f64() {
                None => {
                    assert!(arc.is_none(), "{name}: flat");
                    continue;
                }
                Some(radius) => {
                    let arc = arc.unwrap_or_else(|| panic!("{name}: an arc"));
                    assert!((arc.radius - radius).abs() < 1e-9, "{name}: radius {} vs {radius}", arc.radius);
                    for sample in case["points"].as_array().unwrap() {
                        let flat = pair(&sample["flat"]);
                        let bent = pair(&sample["bent"]);
                        let got = arc.bend(flat);
                        assert!((got[0] - bent[0]).abs() < 1e-6 && (got[1] - bent[1]).abs() < 1e-6, "{name}: {got:?} vs {bent:?}");
                        let back = arc.unbend(bent);
                        assert!((back[0] - flat[0]).abs() < 1e-6 && (back[1] - flat[1]).abs() < 1e-6, "{name}: back {back:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn sizes_the_bent_raster_like_the_reference() {
        for case in cases()["sizes"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let (width, height) = (case["width"].as_u64().unwrap() as u32, case["height"].as_u64().unwrap() as u32);
            let got = bent_size(case["curve"].as_f64().unwrap(), width, height);
            let want = (case["bent"][0].as_u64().unwrap() as u32, case["bent"][1].as_u64().unwrap() as u32);
            assert!(got.0.abs_diff(want.0) <= 2 && got.1.abs_diff(want.1) <= 2, "{name}: {got:?} vs {want:?}");
        }
    }

    /// A raster with a red left half and a blue right half, a white
    /// middle row, fully opaque.
    fn halves(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend(if y == height / 2 {
                    [255, 255, 255, 255]
                } else if x < width / 2 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                });
            }
        }
        pixels
    }

    fn at(bent: &Bent, x: f64, y: f64) -> [u8; 4] {
        let px = (x + bent.width as f64 / 2.0).floor() as usize;
        let py = (y + bent.height as f64 / 2.0).floor() as usize;
        let index = (py * bent.width as usize + px) * 4;
        bent.pixels[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn a_flat_curve_leaves_the_raster_alone() {
        assert!(warp(0.0, 100, 20, &halves(100, 20)).is_none());
        assert!(warp(0.2, 100, 20, &halves(100, 20)).is_none());
        assert_eq!(bent_size(0.0, 100, 20), (100, 20));
    }

    #[test]
    fn an_arch_drops_the_ends_and_keeps_the_middle() {
        let (width, height) = (200, 30);
        let bent = warp(50.0, width, height, &halves(width, height)).unwrap();
        let arc = Arc::new(50.0, width as f64).unwrap();
        // The middle stays put: red just left of centre, blue just right.
        assert_eq!(at(&bent, -3.0, 5.0), [255, 0, 0, 255]);
        assert_eq!(at(&bent, 3.0, 5.0), [0, 0, 255, 255]);
        // A point near each end lands where the arc puts it, lower down.
        for (flat, colour) in [([-90.0, 5.0], [255, 0, 0, 255]), ([90.0, 5.0], [0, 0, 255, 255])] {
            let [x, y] = arc.bend(flat);
            assert!(y > 20.0, "the ends drop: {y}");
            assert_eq!(at(&bent, x, y), colour, "{flat:?} at ({x:.1}, {y:.1})");
        }
        // Above the middle of the arch is empty.
        assert_eq!(at(&bent, 0.0, -(bent.height as f64) / 2.0 + 1.0)[3], 0);
    }

    #[test]
    fn a_smile_raises_the_ends() {
        let (width, height) = (200, 30);
        let bent = warp(-50.0, width, height, &halves(width, height)).unwrap();
        let arc = Arc::new(-50.0, width as f64).unwrap();
        let [x, y] = arc.bend([-90.0, -5.0]);
        assert!(y < -20.0, "the ends rise: {y}");
        assert_eq!(at(&bent, x, y), [255, 0, 0, 255]);
        let [x, y] = arc.bend([0.0, 0.0]);
        assert_eq!(at(&bent, x, y), [255, 255, 255, 255], "the middle row stays at the centre");
    }

    #[test]
    fn transparent_edges_keep_their_colour() {
        // Red text on a clear raster: no dark fringe where it is sampled
        // between a red and a clear pixel.
        let (width, height) = (64u32, 16u32);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 4..12 {
            for x in 8..56 {
                let index = ((y * width + x) * 4) as usize;
                pixels[index..index + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let bent = warp(30.0, width, height, &pixels).unwrap();
        for pixel in bent.pixels.chunks(4).filter(|pixel| pixel[3] > 0) {
            assert_eq!(&pixel[..3], &[255, 0, 0], "edge pixel {pixel:?}");
        }
    }
}
