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

// ---------------------------------------------------------------------
// Text on a path (row 91): the flat raster laid along a path of cubic
// Bézier segments instead of an arc. The path is scaled so its length is
// the raster's width (the text fills it end to end); its origin is the
// text's anchor, so the bent raster comes back centred there. Each output
// pixel finds its nearest point on the path (sampled into short straight
// pieces): how far along the path that is gives the flat x, and its side
// distance the flat y. Checked against `fixtures/text-path/cases.json`.

/// Path shapes offered (`textPath` param); `custom` reads `textPathPoints`.
pub const PATH_PRESETS: [&str; 5] = ["wave", "rise", "swoosh", "circle", "custom"];
/// Straight pieces each Bézier segment is sampled into.
pub const SAMPLES_PER_SEGMENT: usize = 32;

/// The control points (cubic segments: 3k + 1 points, units of the text's
/// width around its anchor, y down) of a path shape at `amount` (0 to 100),
/// or `None` for no path. `custom` is JSON `[[x, y], ...]`.
pub fn path_points(preset: &str, amount: f64, custom: &str) -> Option<Vec<[f64; 2]>> {
    let a = if amount.is_finite() { amount.clamp(0.0, 100.0) / 100.0 } else { 0.5 };
    match preset {
        "wave" => Some(vec![[-0.5, 0.0], [-0.15, -0.45 * a], [0.15, 0.45 * a], [0.5, 0.0]]),
        "rise" => Some(vec![[-0.5, 0.25 * a], [-0.2, 0.25 * a], [0.2, -0.25 * a], [0.5, -0.25 * a]]),
        "swoosh" => Some(vec![[-0.5, -0.05 * a], [-0.2, 0.35 * a], [0.25, 0.3 * a], [0.5, -0.35 * a]]),
        "circle" => Some(circle_points(a.max(0.05))),
        "custom" => {
            let points: Vec<[f64; 2]> = serde_json::from_str(custom).ok()?;
            (points.len() >= 4 && (points.len() - 1) % 3 == 0 && points.iter().flatten().all(|v| v.is_finite())).then_some(points)
        }
        _ => None,
    }
}

/// An arc over the top of a unit circle centred on the anchor, `share` of
/// a full turn long and centred on the top, clockwise so the text reads
/// upright over it: cubic pieces of at most a quarter turn.
fn circle_points(share: f64) -> Vec<[f64; 2]> {
    let span = std::f64::consts::TAU * share.min(1.0);
    let pieces = (span / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let at = |theta: f64| [theta.sin(), -theta.cos()];
    let tangent = |theta: f64| [theta.cos(), theta.sin()];
    let start = -span / 2.0;
    let step = span / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut points = vec![at(start)];
    for piece in 0..pieces {
        let (t0, t1) = (start + step * piece as f64, start + step * (piece + 1) as f64);
        let (p0, p3) = (at(t0), at(t1));
        let (d0, d1) = (tangent(t0), tangent(t1));
        points.push([p0[0] + k * d0[0], p0[1] + k * d0[1]]);
        points.push([p3[0] - k * d1[0], p3[1] - k * d1[1]]);
        points.push(p3);
    }
    points
}

/// The path sampled into straight pieces and scaled about the anchor so
/// its length is `width`; `None` when it has no length.
pub fn path_polyline(points: &[[f64; 2]], width: f64) -> Option<Vec<[f64; 2]>> {
    if points.len() < 4 || !(width > 0.0) {
        return None;
    }
    let mut line = vec![points[0]];
    for segment in 0..(points.len() - 1) / 3 {
        let (p0, p1, p2, p3) = (points[3 * segment], points[3 * segment + 1], points[3 * segment + 2], points[3 * segment + 3]);
        for step in 1..=SAMPLES_PER_SEGMENT {
            let t = step as f64 / SAMPLES_PER_SEGMENT as f64;
            let u = 1.0 - t;
            let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            line.push([
                b0 * p0[0] + b1 * p1[0] + b2 * p2[0] + b3 * p3[0],
                b0 * p0[1] + b1 * p1[1] + b2 * p2[1] + b3 * p3[1],
            ]);
        }
    }
    let length: f64 = line.windows(2).map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])).sum();
    if !(length > 1e-9) {
        return None;
    }
    let scale = width / length;
    Some(line.into_iter().map(|[x, y]| [x * scale, y * scale]).collect())
}

/// The size of a `width` x `height` raster laid along `points`, centred on
/// the anchor (the flat size when the path has no length).
pub fn path_size(points: &[[f64; 2]], width: u32, height: u32) -> (u32, u32) {
    let Some(line) = path_polyline(points, width as f64) else { return (width, height) };
    let band = height as f64 / 2.0 + 1.0;
    let reach_x = line.iter().map(|p| p[0].abs()).fold(0.0, f64::max) + band;
    let reach_y = line.iter().map(|p| p[1].abs()).fold(0.0, f64::max) + band;
    let side = |reach: f64| ((2.0 * reach.ceil()) as u32).clamp(1, MAX_SIDE);
    (side(reach_x), side(reach_y))
}

/// Lays a raster of straight-alpha pixels (RGBA or BGRA) along the path.
/// `None` when the path has no length (draw the raster as it is).
pub fn warp_path(points: &[[f64; 2]], width: u32, height: u32, pixels: &[u8]) -> Option<Bent> {
    let line = path_polyline(points, width as f64)?;
    if pixels.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let (out_width, out_height) = path_size(points, width, height);
    let mut out = vec![0u8; out_width as usize * out_height as usize * 4];
    let half_height = height as f64 / 2.0;
    let band = half_height + 1.0;
    let (out_half_width, out_half_height) = (out_width as f64 / 2.0, out_height as f64 / 2.0);
    // Pieces: start, end, length so far.
    let mut along = Vec::with_capacity(line.len());
    let mut total = 0.0;
    for pair in line.windows(2) {
        along.push(total);
        total += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
    }
    let last = line.len() - 2;
    // A grid of cells a band wide, each listing the pieces that pass near.
    let cell = band.max(4.0);
    let columns = ((out_width as f64 / cell).ceil() as usize).max(1);
    let rows = ((out_height as f64 / cell).ceil() as usize).max(1);
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); columns * rows];
    let to_cell = |x: f64, y: f64| (((x + out_half_width) / cell).floor(), ((y + out_half_height) / cell).floor());
    for (index, pair) in line.windows(2).enumerate() {
        let (x0, y0) = to_cell(pair[0][0].min(pair[1][0]) - band, pair[0][1].min(pair[1][1]) - band);
        let (x1, y1) = to_cell(pair[0][0].max(pair[1][0]) + band, pair[0][1].max(pair[1][1]) + band);
        for row in (y0.max(0.0) as usize)..=(y1.max(0.0) as usize).min(rows - 1) {
            for column in (x0.max(0.0) as usize)..=(x1.max(0.0) as usize).min(columns - 1) {
                grid[row * columns + column].push(index);
            }
        }
    }
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
            let q = [ox as f64 + 0.5 - out_half_width, oy as f64 + 0.5 - out_half_height];
            let (column, row) = to_cell(q[0], q[1]);
            let (column, row) = (column.max(0.0) as usize, row.max(0.0) as usize);
            // The nearest piece (ties to the earlier one).
            let mut best: Option<(f64, usize, f64, f64)> = None;
            for &index in &grid[row.min(rows - 1) * columns + column.min(columns - 1)] {
                let (a, b) = (line[index], line[index + 1]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let length2 = dx * dx + dy * dy;
                let raw = if length2 > 0.0 { ((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / length2 } else { 0.0 };
                let t = raw.clamp(0.0, 1.0);
                let (cx, cy) = (a[0] + dx * t, a[1] + dy * t);
                let distance2 = (q[0] - cx) * (q[0] - cx) + (q[1] - cy) * (q[1] - cy);
                let better = match best {
                    None => true,
                    Some((d2, i, _, _)) => distance2 < d2 || (distance2 == d2 && index < i),
                };
                if better {
                    best = Some((distance2, index, t, raw));
                }
            }
            let Some((distance2, index, t, raw)) = best else { continue };
            // Only pixels within a band of the path show text.
            if distance2 > band * band {
                continue;
            }
            // Past either end of the path: nothing there.
            if (index == 0 && raw < 0.0) || (index == last && raw > 1.0) {
                continue;
            }
            let (a, b) = (line[index], line[index + 1]);
            let length = (b[0] - a[0]).hypot(b[1] - a[1]);
            if length <= 0.0 {
                continue;
            }
            let (tx, ty) = ((b[0] - a[0]) / length, (b[1] - a[1]) / length);
            let (cx, cy) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            let side = (q[0] - cx) * -ty + (q[1] - cy) * tx;
            if side.abs() > band {
                continue;
            }
            let sx = along[index] + t * length - 0.5;
            let sy = side + half_height - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let mut sum = [0.0f64; 4];
            for (dx, dy, weight) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
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

/// The path a text's params ask for (`textPath`, `textPathAmount`,
/// `textPathPoints`), or `None` for none.
pub fn read_path(preset: &str, amount: Option<f64>, custom: &str) -> Option<Vec<[f64; 2]>> {
    if preset.is_empty() || preset == "none" {
        return None;
    }
    path_points(preset, amount.unwrap_or(50.0), custom)
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

    #[test]
    fn lays_text_along_paths_like_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/text-path/cases.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let (width, height) = (fixture["width"].as_u64().unwrap() as u32, fixture["height"].as_u64().unwrap() as u32);
        let flat: Vec<u8> = fixture["flat"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
        for case in fixture["cases"].as_array().unwrap() {
            let name = format!("{} {}", case["preset"], case["amount"]);
            let points = path_points(case["preset"].as_str().unwrap(), case["amount"].as_f64().unwrap(), case["custom"].as_str().unwrap()).unwrap();
            let want: Vec<[f64; 2]> = case["points"].as_array().unwrap().iter().map(pair).collect();
            assert_eq!(points.len(), want.len(), "{name}");
            for (got, want) in points.iter().zip(&want) {
                assert!((got[0] - want[0]).abs() < 1e-12 && (got[1] - want[1]).abs() < 1e-12, "{name}: {got:?} vs {want:?}");
            }
            let bent = warp_path(&points, width, height, &flat).unwrap();
            assert_eq!((bent.width as u64, bent.height as u64), (case["width"].as_u64().unwrap(), case["height"].as_u64().unwrap()), "{name}");
            let expected: Vec<u8> = case["rgba"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            let worst = bent.pixels.iter().zip(&expected).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert!(worst <= 1, "{name}: off by {worst}");
        }
        assert_eq!(read_path("none", Some(50.0), ""), None);
        assert_eq!(read_path("custom", Some(50.0), "[[0,0],[1,1]]"), None, "a custom path needs 3k + 1 points");
        assert!(read_path("wave", None, "").is_some());
    }
}
