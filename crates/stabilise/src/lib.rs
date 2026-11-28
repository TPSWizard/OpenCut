//! Stabilisation (row 107), without rewriting the file.
//!
//! A video clip's frames are measured once: for each pair of neighbouring
//! frames, the camera's movement between them ([`frame_step`]) is fitted as
//! a similarity (shift, turn and zoom) to the dense motion of the shared
//! `jumpcut` motion estimate, with Tukey weights so a person walking
//! through the frame does not count. The steps are kept on the clip as a
//! compact string ([`CameraPath::encode`]). When a frame is drawn, the
//! camera path (each frame's position, from the steps) is smoothed with a
//! Gaussian over neighbouring frames, or held still, and the frame is
//! moved by the difference ([`CameraPath::corrections`]), zoomed in just
//! enough that no edge shows (up to [`MAX_ZOOM`]).
//!
//! All geometry is in frame units: centred on the frame's centre and
//! divided by its width, so the measured size does not matter. A
//! similarity `S = (a, b, tx, ty)` maps `p` to
//! `(a·px − b·py + tx, b·px + a·py + ty)`.
//!
//! Both editors use this crate (the web through WASM); checked against
//! `fixtures/stabilise/stabilise.json`, written by an independent Python
//! reference.

use std::f64::consts::PI;

/// Frames are measured this wide (the height follows the picture).
pub const MEASURE_WIDTH: u32 = 192;
/// Frames measured a second when the file's own rate is unknown or out of range.
pub const DEFAULT_RATE: f64 = 30.0;
/// The share of each side left out of the fit (new picture comes in
/// there): a point counts only when it is clear of it in both frames.
pub const MARGIN: f64 = 0.06;
const STRIDE: usize = 2;
const ROBUST_ITERATIONS: usize = 6;
/// Points further than this many robust deviations from the fit do not count.
const TUKEY: f64 = 4.0;
/// The least deviation, in pixels of the measured picture.
const SIGMA_FLOOR_PX: f64 = 0.05;
const MIN_POINTS: usize = 16;
/// A step turning or zooming more than this between two frames is a cut,
/// or a frame that could not be matched, and counts as no movement.
const MAX_STEP_ANGLE: f64 = 10.0 * PI / 180.0;
const MAX_STEP_SCALE: f64 = 1.2;
/// The most the frame is zoomed in to hide its edges.
pub const MAX_ZOOM: f64 = 1.5;

/// The rate frames are measured at: the file's own when known (10 to 60
/// a second), else [`DEFAULT_RATE`].
pub fn measure_rate(source_fps: Option<f64>) -> f64 {
    match source_fps {
        Some(fps) if fps.is_finite() && (10.0..=60.0).contains(&fps) => fps,
        Some(fps) if fps.is_finite() && fps > 60.0 => 60.0,
        _ => DEFAULT_RATE,
    }
}

/// The measured picture's size for a source frame size.
pub fn measure_size(width: u32, height: u32) -> (u32, u32) {
    let width = width.max(1) as f64;
    let height = height.max(1) as f64;
    let measured = ((MEASURE_WIDTH as f64 * height / width / 2.0).round() * 2.0).max(2.0);
    (MEASURE_WIDTH, measured as u32)
}

/// A similarity: `p ↦ (a·px − b·py + tx, b·px + a·py + ty)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Similarity {
    pub a: f64,
    pub b: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Similarity {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, tx: 0.0, ty: 0.0 };

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x - self.b * y + self.tx, self.b * x + self.a * y + self.ty)
    }

    /// `self ∘ other`: `other` first.
    pub fn then_after(&self, other: &Self) -> Self {
        let (tx, ty) = Self { tx: 0.0, ty: 0.0, ..*self }.apply(other.tx, other.ty);
        Self {
            a: self.a * other.a - self.b * other.b,
            b: self.a * other.b + self.b * other.a,
            tx: tx + self.tx,
            ty: ty + self.ty,
        }
    }

    pub fn inverse(&self) -> Self {
        let det = self.a * self.a + self.b * self.b;
        let (ai, bi) = (self.a / det, -self.b / det);
        Self { a: ai, b: bi, tx: -(ai * self.tx - bi * self.ty), ty: -(bi * self.tx + ai * self.ty) }
    }

    /// Radians, positive from +x towards +y (clockwise on screen).
    pub fn angle(&self) -> f64 {
        self.b.atan2(self.a)
    }

    pub fn scale(&self) -> f64 {
        self.a.hypot(self.b)
    }

    fn params(&self) -> [f64; 4] {
        [self.angle(), self.scale().ln(), self.tx, self.ty]
    }

    fn from_params([angle, log_scale, tx, ty]: [f64; 4]) -> Self {
        let scale = log_scale.exp();
        Self { a: scale * angle.cos(), b: scale * angle.sin(), tx, ty }
    }

    fn zoomed(&self, zoom: f64) -> Self {
        Self { a: self.a * zoom, b: self.b * zoom, tx: self.tx * zoom, ty: self.ty * zoom }
    }
}

/// The similarity that best maps each `(px, py)` to its `(qx, qy)`, by
/// least squares re-weighted with Tukey's biweight (points far from the
/// fit stop counting). `sigma_floor` is the least deviation assumed, in
/// the points' units. `None` with too few points or no spread.
pub fn fit_similarity(points: &[[f64; 4]], sigma_floor: f64) -> Option<Similarity> {
    let n = points.len();
    if n < MIN_POINTS {
        return None;
    }
    let mut weights = vec![1.0; n];
    let mut fitted = Similarity::IDENTITY;
    for iteration in 0..=ROBUST_ITERATIONS {
        let total: f64 = weights.iter().sum();
        if total < MIN_POINTS as f64 * 0.25 {
            return None;
        }
        let mean = |c: usize| points.iter().zip(&weights).map(|(point, w)| w * point[c]).sum::<f64>() / total;
        let (pmx, pmy, qmx, qmy) = (mean(0), mean(1), mean(2), mean(3));
        let (mut den, mut na, mut nb) = (0.0, 0.0, 0.0);
        for (point, w) in points.iter().zip(&weights) {
            let (px, py, qx, qy) = (point[0] - pmx, point[1] - pmy, point[2] - qmx, point[3] - qmy);
            den += w * (px * px + py * py);
            na += w * (px * qx + py * qy);
            nb += w * (px * qy - py * qx);
        }
        if den <= 1e-18 {
            return None;
        }
        let (a, b) = (na / den, nb / den);
        fitted = Similarity { a, b, tx: qmx - (a * pmx - b * pmy), ty: qmy - (b * pmx + a * pmy) };
        if iteration == ROBUST_ITERATIONS {
            break;
        }
        let residuals: Vec<f64> = points
            .iter()
            .map(|point| {
                let (fx, fy) = fitted.apply(point[0], point[1]);
                (fx - point[2]).hypot(fy - point[3])
            })
            .collect();
        let mut ordered = residuals.clone();
        ordered.sort_by(f64::total_cmp);
        let sigma = (1.4826 * ordered[n / 2]).max(sigma_floor);
        let cutoff = TUKEY * sigma;
        for (w, r) in weights.iter_mut().zip(&residuals) {
            *w = if *r < cutoff { (1.0 - (r / cutoff).powi(2)).powi(2) } else { 0.0 };
        }
    }
    Some(fitted)
}

/// A move bigger than this many pixels (of the measured picture) at a
/// corner is measured again on the second frame warped back by the first
/// estimate, so a big move is not under-measured.
const REFINE_PX: f64 = 1.0;

/// The similarity from `p` to `q` fitted to their motion. `q` is the frame
/// warped by `prior` (the identity on the first pass): a point's end in `q`
/// must be clear of the edges once `prior` maps it back to the real frame.
fn measure(p: &[u8], q: &[u8], width: usize, height: usize, prior: &Similarity) -> Option<Similarity> {
    let field = jumpcut::motion_between(p, q, width, height)?;
    let (ww, wh, scale) = (field.width, field.height, field.scale as f64);
    let margin_x = ((MARGIN * ww as f64 + 0.5).floor() as usize).max(1);
    let margin_y = ((MARGIN * wh as f64 + 0.5).floor() as usize).max(1);
    if ww <= 2 * margin_x + 1 || wh <= 2 * margin_y + 1 {
        return None;
    }
    let (w, h) = (width as f64, height as f64);
    let unit_x = |x: f64| ((x + 0.5) * scale - w / 2.0) / w;
    let unit_y = |y: f64| ((y + 0.5) * scale - h / 2.0) / w;
    let (lo_x, hi_x) = (unit_x(margin_x as f64), unit_x((ww - 1 - margin_x) as f64));
    let (lo_y, hi_y) = (unit_y(margin_y as f64), unit_y((wh - 1 - margin_y) as f64));
    let inside = |(x, y): (f64, f64)| lo_x <= x && x <= hi_x && lo_y <= y && y <= hi_y;
    let mut points = Vec::new();
    for y in (margin_y..wh - margin_y).step_by(STRIDE) {
        for x in (margin_x..ww - margin_x).step_by(STRIDE) {
            let k = y * ww + x;
            let (u, v) = (field.u[k] / 2.0, field.v[k] / 2.0);
            let (x, y) = (x as f64, y as f64);
            let a = (unit_x(x - u), unit_y(y - v));
            let b = (unit_x(x + u), unit_y(y + v));
            // Both ends must be clear of the edges, where new picture comes in.
            if !inside(a) || !inside(prior.apply(b.0, b.1)) {
                continue;
            }
            points.push([a.0, a.1, b.0, b.1]);
        }
    }
    fit_similarity(&points, SIGMA_FLOOR_PX * scale / w)
}

fn sample(plane: &[f64], w: usize, h: usize, x: f64, y: f64) -> f64 {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let top = plane[y0 * w + x0] * (1.0 - fx) + plane[y0 * w + x1] * fx;
    let bottom = plane[y1 * w + x0] * (1.0 - fx) + plane[y1 * w + x1] * fx;
    top * (1.0 - fy) + bottom * fy
}

/// `q` seen through `s`: each pixel shows `q` at `s` of its place (frame
/// units), bilinear with the edges held.
fn warp(q: &[u8], w: usize, h: usize, s: &Similarity) -> Vec<u8> {
    let planes: Vec<Vec<f64>> = (0..4).map(|c| (0..w * h).map(|i| f64::from(q[i * 4 + c])).collect()).collect();
    let (wf, hf) = (w as f64, h as f64);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = s.apply((x as f64 + 0.5 - wf / 2.0) / wf, (y as f64 + 0.5 - hf / 2.0) / wf);
            let (px, py) = (fx * wf + wf / 2.0 - 0.5, fy * wf + hf / 2.0 - 0.5);
            for c in 0..4 {
                let value = sample(&planes[c], w, h, px, py);
                out[(y * w + x) * 4 + c] = (value + 0.5).floor().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// The camera's movement from frame `p` to frame `q` (both `width × height`,
/// RGBA or BGRA): where each point of `p` is in `q`, in frame units. `None`
/// when it cannot be measured (sizes wrong, too flat, or a jump bigger
/// than a camera makes between two frames). A move of more than
/// [`REFINE_PX`] at a corner is measured again on `q` warped back by the
/// first estimate, and the two are put together.
pub fn frame_step(p: &[u8], q: &[u8], width: usize, height: usize) -> Option<Similarity> {
    let mut step = measure(p, q, width, height, &Similarity::IDENTITY)?;
    let (w, half) = (width as f64, height as f64 / (2.0 * width as f64));
    let moved = [(-0.5, -half), (0.5, -half), (0.5, half), (-0.5, half)]
        .iter()
        .map(|&(x, y)| {
            let (mx, my) = step.apply(x, y);
            (mx - x).hypot(my - y)
        })
        .fold(0.0, f64::max)
        * w;
    if moved > REFINE_PX
        && let Some(rest) = measure(p, &warp(q, width, height, &step), width, height, &step)
    {
        step = step.then_after(&rest);
    }
    let size = step.scale();
    (step.angle().abs() <= MAX_STEP_ANGLE && size <= MAX_STEP_SCALE && size >= 1.0 / MAX_STEP_SCALE).then_some(step)
}

/// How hard the camera path is smoothed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Strength {
    Off,
    Light,
    Medium,
    Strong,
    /// One fixed camera for the whole clip, as on a tripod.
    Still,
}

impl Strength {
    pub const ALL: [Strength; 5] = [Strength::Off, Strength::Light, Strength::Medium, Strength::Strong, Strength::Still];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|strength| strength.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Strength::Off => "off",
            Strength::Light => "light",
            Strength::Medium => "medium",
            Strength::Strong => "strong",
            Strength::Still => "still",
        }
    }

    /// The Gaussian's spread in frames (0 when off; `None` when still).
    pub fn sigma(self) -> Option<f64> {
        match self {
            Strength::Off => Some(0.0),
            Strength::Light => Some(4.0),
            Strength::Medium => Some(10.0),
            Strength::Strong => Some(25.0),
            Strength::Still => None,
        }
    }
}

/// A clip's measured camera steps: which media, the source time of the
/// first frame, frames a second, the frame's height over its width, and
/// the step from each frame to the next (`None` where not measured).
#[derive(Clone, Debug, PartialEq)]
pub struct CameraPath {
    pub media_id: String,
    pub start: f64,
    pub rate: f64,
    pub aspect: f64,
    pub steps: Vec<Option<Similarity>>,
}

/// Each frame's correction (already zoomed) and the zoom.
#[derive(Clone, Debug, PartialEq)]
pub struct Corrections {
    pub start: f64,
    pub rate: f64,
    /// The zoom used (at most [`MAX_ZOOM`]).
    pub zoom: f64,
    /// The zoom that would hide every edge; above `zoom`, edges show at times.
    pub needed: f64,
    pub frames: Vec<Similarity>,
}

impl CameraPath {
    /// Frames measured (one more than the steps).
    pub fn frames(&self) -> usize {
        self.steps.len() + 1
    }

    /// `media;start;rate;aspect;da:b:tx:ty,…` (`x` for a step not measured),
    /// with `da = a − 1` and seven decimals.
    pub fn encode(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|step| match step {
                Some(s) => format!("{}:{}:{}:{}", number(s.a - 1.0), number(s.b), number(s.tx), number(s.ty)),
                None => "x".to_owned(),
            })
            .collect();
        format!("{};{};{};{};{}", self.media_id, number(self.start), number(self.rate), number(self.aspect), steps.join(","))
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(5, ';');
        let media_id = parts.next()?.to_owned();
        let start: f64 = parts.next()?.trim().parse().ok()?;
        let rate: f64 = parts.next()?.trim().parse().ok()?;
        let aspect: f64 = parts.next()?.trim().parse().ok()?;
        let body = parts.next()?;
        let steps = if body.is_empty() {
            Vec::new()
        } else {
            body.split(',')
                .map(|part| {
                    if part.trim() == "x" {
                        return Some(None);
                    }
                    let values: Vec<f64> = part.split(':').map(|v| v.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect::<Option<_>>()?;
                    let [da, b, tx, ty] = values[..] else { return None };
                    let step = Similarity { a: 1.0 + da, b, tx, ty };
                    (step.scale() > 1e-6).then_some(Some(step))
                })
                .collect::<Option<Vec<_>>>()?
        };
        (!media_id.is_empty()
            && start.is_finite()
            && rate.is_finite()
            && rate > 0.0
            && aspect.is_finite()
            && aspect > 0.0)
            .then_some(Self { media_id, start, rate, aspect, steps })
    }

    /// Each frame's position: where its points are in the first frame's view.
    fn worlds(&self) -> Vec<Similarity> {
        let mut worlds = vec![Similarity::IDENTITY];
        for step in &self.steps {
            let last = *worlds.last().unwrap();
            worlds.push(last.then_after(&step.unwrap_or(Similarity::IDENTITY).inverse()));
        }
        worlds
    }

    /// The per-frame corrections for `strength`.
    pub fn corrections(&self, strength: Strength) -> Corrections {
        let n = self.frames();
        let base = Corrections { start: self.start, rate: self.rate, zoom: 1.0, needed: 1.0, frames: vec![Similarity::IDENTITY; n] };
        if strength.sigma() == Some(0.0) {
            return base;
        }
        let worlds = self.worlds();
        let mut params: Vec<[f64; 4]> = worlds.iter().map(Similarity::params).collect();
        for k in 1..n {
            while params[k][0] - params[k - 1][0] > PI {
                params[k][0] -= 2.0 * PI;
            }
            while params[k][0] - params[k - 1][0] < -PI {
                params[k][0] += 2.0 * PI;
            }
        }
        let smoothed = smooth(&params, strength.sigma());
        let corrections: Vec<Similarity> =
            (0..n).map(|k| Similarity::from_params(smoothed[k]).inverse().then_after(&worlds[k])).collect();
        let needed = corrections.iter().map(|c| cover_zoom(c, self.aspect)).fold(1.0, f64::max);
        let zoom = needed.min(MAX_ZOOM);
        Corrections {
            zoom,
            needed: if needed.is_finite() { needed } else { MAX_ZOOM * 10.0 },
            frames: corrections.iter().map(|c| c.zoomed(zoom)).collect(),
            ..base
        }
    }
}

impl Corrections {
    /// The correction of the frame shown at a source time: the measured
    /// frame nearest it, as the desktop's decoder shows the frame nearest a
    /// time (the web looks up by the drawn frame's own time); held at the ends.
    pub fn at(&self, source_seconds: f64) -> Similarity {
        let Some(last) = self.frames.len().checked_sub(1) else {
            return Similarity::IDENTITY;
        };
        let f = ((source_seconds - self.start) * self.rate + 0.5).floor();
        let index = if f > 0.0 { (f as usize).min(last) } else { 0 };
        self.frames[index]
    }
}

fn smooth(params: &[[f64; 4]], sigma: Option<f64>) -> Vec<[f64; 4]> {
    let n = params.len();
    let Some(sigma) = sigma else {
        let mean: [f64; 4] = std::array::from_fn(|c| params.iter().map(|p| p[c]).sum::<f64>() / n as f64);
        return vec![mean; n];
    };
    let reach = (3.0 * sigma).ceil() as usize;
    (0..n)
        .map(|k| {
            let mut total = [0.0; 4];
            let mut weight = 0.0;
            for j in k.saturating_sub(reach)..=(k + reach).min(n - 1) {
                let d = j as f64 - k as f64;
                let g = (-(d * d) / (2.0 * sigma * sigma)).exp();
                weight += g;
                for c in 0..4 {
                    total[c] += g * params[j][c];
                }
            }
            total.map(|t| t / weight)
        })
        .collect()
}

/// The least zoom (about the centre) after which the corrected frame
/// covers the whole frame; infinite when even its centre is off it.
fn cover_zoom(correction: &Similarity, aspect: f64) -> f64 {
    let (hw, hh) = (0.5, aspect / 2.0);
    let inverse = correction.inverse();
    let (cx, cy) = inverse.apply(0.0, 0.0);
    if cx.abs() >= hw || cy.abs() >= hh {
        return f64::INFINITY;
    }
    let mut need: f64 = 1.0;
    for (x, y) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
        let (ex, ey) = inverse.apply(x, y);
        let (dx, dy) = (ex - cx, ey - cy);
        let mut reach: f64 = 1.0;
        if dx > 0.0 {
            reach = reach.min((hw - cx) / dx);
        } else if dx < 0.0 {
            reach = reach.min((-hw - cx) / dx);
        }
        if dy > 0.0 {
            reach = reach.min((hh - cy) / dy);
        } else if dy < 0.0 {
            reach = reach.min((-hh - cy) / dy);
        }
        need = need.max(1.0 / reach);
    }
    need
}

/// Seven decimals, without trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{:.7}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() { "0".to_owned() } else { text.to_owned() }
}

/// Where a clip's picture is drawn once corrected: the drawn frame's
/// centre offset (in the clip's own unrotated, unflipped frame, canvas
/// pixels), the turn to add (degrees, clockwise) and the scale to multiply.
/// `drawn_width_x` / `drawn_width_y` are the frame's width as drawn along
/// each axis (they differ when the clip is stretched); `flip_x` / `flip_y`
/// when the clip is mirrored.
pub fn drawn_offset(correction: &Similarity, drawn_width_x: f64, drawn_width_y: f64, flip_x: bool, flip_y: bool) -> (f64, f64, f64, f64) {
    let sign = |flip: bool| if flip { -1.0 } else { 1.0 };
    let turn = correction.angle().to_degrees() * sign(flip_x) * sign(flip_y);
    (correction.tx * drawn_width_x * sign(flip_x), correction.ty * drawn_width_y * sign(flip_y), turn, correction.scale())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_and_inverse_undo_each_other() {
        let s = Similarity { a: 1.01, b: -0.02, tx: 0.03, ty: -0.01 };
        let i = s.then_after(&s.inverse());
        assert!((i.a - 1.0).abs() < 1e-12 && i.b.abs() < 1e-12 && i.tx.abs() < 1e-12 && i.ty.abs() < 1e-12, "{i:?}");
        let (x, y) = s.then_after(&s).apply(0.1, 0.2);
        let (x1, y1) = s.apply(0.1, 0.2);
        let (x2, y2) = s.apply(x1, y1);
        assert!((x - x2).abs() < 1e-12 && (y - y2).abs() < 1e-12);
    }

    #[test]
    fn rates_and_sizes() {
        assert_eq!(measure_rate(Some(29.97)), 29.97);
        assert_eq!(measure_rate(Some(120.0)), 60.0);
        assert_eq!(measure_rate(None), DEFAULT_RATE);
        assert_eq!(measure_rate(Some(2.0)), DEFAULT_RATE);
        assert_eq!(measure_size(1920, 1080), (192, 108));
        assert_eq!(measure_size(1080, 1920), (192, 342));
    }

    #[test]
    fn a_shift_comes_back_as_a_shift_once_drawn() {
        // A correction moving the frame right by a tenth of its width,
        // drawn 1000 px wide, mirrored: the drawn offset is to the left.
        let c = Similarity { a: 1.0, b: 0.0, tx: 0.1, ty: 0.0 };
        assert_eq!(drawn_offset(&c, 1000.0, 1000.0, false, false), (100.0, 0.0, 0.0, 1.0));
        assert_eq!(drawn_offset(&c, 1000.0, 1000.0, true, false).0, -100.0);
    }

    #[test]
    fn off_and_unparseable() {
        let path = CameraPath { media_id: "m".into(), start: 0.0, rate: 30.0, aspect: 0.5625, steps: vec![None; 3] };
        let c = path.corrections(Strength::Off);
        assert_eq!((c.zoom, c.frames.len()), (1.0, 4));
        assert_eq!(CameraPath::parse(&path.encode()), Some(path));
        assert_eq!(CameraPath::parse("m;0;0;1;"), None, "no rate");
        assert_eq!(CameraPath::parse("m;0;30;1;1:2"), None, "a short step");
        assert_eq!(Strength::parse("still"), Some(Strength::Still));
        assert_eq!(Strength::parse("bogus"), None);
    }
}
