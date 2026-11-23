//! Motion tracking (row 71): a point picked in one frame of a video,
//! followed through the next frames by template matching, and the
//! position keys that make another clip follow it. The desktop calls this
//! directly; the web through `crates/wasm` (`PointTracker`, `followKeys`),
//! so both editors track the same way.
//!
//! Each step looks for the patch around the last position in the new
//! frame: first on half-size frames within a search radius, then at full
//! size within two pixels of that, scoring by zero-mean normalised
//! cross-correlation and refining the peak to a fraction of a pixel. The
//! patch is taken again from each frame it was found in, so slow changes in
//! look and size are followed; a weak match counts as lost and keeps the
//! last position until the patch is found again.

/// A greyscale frame, luma 0..255 in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Gray {
    fn from_pixels(bytes: &[u8], width: usize, height: usize, r: usize, b: usize) -> Self {
        let data = bytes
            .chunks_exact(4)
            .take(width * height)
            .map(|p| 0.299 * p[r] as f32 + 0.587 * p[1] as f32 + 0.114 * p[b] as f32)
            .collect();
        Self { width, height, data }
    }

    /// From RGBA bytes (canvas `ImageData`).
    pub fn from_rgba(rgba: &[u8], width: usize, height: usize) -> Self {
        Self::from_pixels(rgba, width, height, 0, 2)
    }

    /// From BGRA bytes (decoded video frames on the desktop).
    pub fn from_bgra(bgra: &[u8], width: usize, height: usize) -> Self {
        Self::from_pixels(bgra, width, height, 2, 0)
    }

    fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    /// Half the size, each pixel the mean of a 2×2 block.
    pub fn half(&self) -> Self {
        let (width, height) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let (x0, y0) = ((2 * x).min(self.width - 1), (2 * y).min(self.height - 1));
                let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
                data.push((self.at(x0, y0) + self.at(x1, y0) + self.at(x0, y1) + self.at(x1, y1)) / 4.0);
            }
        }
        Self { width, height, data }
    }
}

/// A patch: its pixels minus their mean, and their spread.
#[derive(Clone, Debug)]
struct Patch {
    half_w: usize,
    half_h: usize,
    values: Vec<f32>,
    norm: f32,
}

impl Patch {
    /// The patch centred on (`cx`, `cy`), or `None` when it would leave the frame.
    fn cut(frame: &Gray, cx: i64, cy: i64, half_w: usize, half_h: usize) -> Option<Self> {
        let (hw, hh) = (half_w as i64, half_h as i64);
        if cx - hw < 0 || cy - hh < 0 || cx + hw >= frame.width as i64 || cy + hh >= frame.height as i64 {
            return None;
        }
        let mut values = Vec::with_capacity((2 * half_w + 1) * (2 * half_h + 1));
        for y in (cy - hh)..=(cy + hh) {
            for x in (cx - hw)..=(cx + hw) {
                values.push(frame.at(x as usize, y as usize));
            }
        }
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let mut norm = 0.0;
        for value in &mut values {
            *value -= mean;
            norm += *value * *value;
        }
        Some(Self { half_w, half_h, values, norm: norm.sqrt() })
    }

    /// Spread of the patch's pixels (0 for a flat patch).
    fn spread(&self) -> f32 {
        self.norm / (self.values.len() as f32).sqrt()
    }

    /// Normalised cross-correlation with the frame's patch at (`cx`, `cy`); -1 outside the frame.
    fn score(&self, frame: &Gray, cx: i64, cy: i64) -> f32 {
        let (hw, hh) = (self.half_w as i64, self.half_h as i64);
        if cx - hw < 0 || cy - hh < 0 || cx + hw >= frame.width as i64 || cy + hh >= frame.height as i64 {
            return -1.0;
        }
        let count = self.values.len() as f32;
        let mut sum = 0.0;
        for y in (cy - hh)..=(cy + hh) {
            let row = y as usize * frame.width;
            for x in (cx - hw)..=(cx + hw) {
                sum += frame.data[row + x as usize];
            }
        }
        let mean = sum / count;
        let (mut dot, mut norm) = (0.0, 0.0);
        let mut index = 0;
        for y in (cy - hh)..=(cy + hh) {
            let row = y as usize * frame.width;
            for x in (cx - hw)..=(cx + hw) {
                let value = frame.data[row + x as usize] - mean;
                dot += value * self.values[index];
                norm += value * value;
                index += 1;
            }
        }
        let denominator = norm.sqrt() * self.norm;
        if denominator < 1e-6 { 0.0 } else { dot / denominator }
    }
}

/// Below this match score the point counts as lost for that frame.
pub const LOST_SCORE: f32 = 0.5;
/// A patch flatter than this (luma spread) has nothing to follow.
pub const MIN_PATCH_SPREAD: f32 = 2.0;
/// A patch with at least this spread has enough detail to stop growing the box.
pub const DETAIL_SPREAD: f32 = 20.0;
/// How far the starting box may grow to take in detail.
const BOX_GROWTH: [f64; 4] = [1.0, 1.5, 2.0, 3.0];

/// Where the point is in one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackStep {
    /// Frame pixels.
    pub x: f64,
    pub y: f64,
    /// Match score, -1..1 (1 for the first frame).
    pub score: f64,
    /// The match was too weak; `x`, `y` are the last good position.
    pub lost: bool,
}

/// Follows one point from frame to frame.
#[derive(Clone, Debug)]
pub struct Tracker {
    x: f64,
    y: f64,
    half_w: usize,
    half_h: usize,
    /// Search radius in full-size pixels.
    radius: usize,
    template: Patch,
    template_half: Patch,
    lost_frames: usize,
}

impl Tracker {
    /// Starts at (`x`, `y`) in `frame` (pixels) with a box `box_w` × `box_h`
    /// pixels around it. Fails when the box is flat or leaves the frame.
    pub fn new(frame: &Gray, x: f64, y: f64, box_w: f64, box_h: f64) -> Result<Self, String> {
        // A box inside a flat area (a plain logo, a white card) matches
        // anywhere in it, so the box grows until it takes in some detail,
        // such as the thing's edges.
        let mut chosen = None;
        for grow in BOX_GROWTH {
            let half_w = ((box_w * grow / 2.0).round() as usize).clamp(4, (frame.width / 4).max(4));
            let half_h = ((box_h * grow / 2.0).round() as usize).clamp(4, (frame.height / 4).max(4));
            let cx = (x.round() as i64).clamp(half_w as i64, frame.width as i64 - 1 - half_w as i64);
            let cy = (y.round() as i64).clamp(half_h as i64, frame.height as i64 - 1 - half_h as i64);
            let Some(patch) = Patch::cut(frame, cx, cy, half_w, half_h) else { continue };
            let detailed = patch.spread() >= DETAIL_SPREAD;
            if patch.spread() >= MIN_PATCH_SPREAD {
                chosen = Some((half_w, half_h, cx, cy, patch));
            }
            if detailed {
                break;
            }
        }
        let Some((half_w, half_h, cx, cy, template)) = chosen else {
            return Err("There is nothing to follow there: pick a spot with some detail".to_owned());
        };
        let half = frame.half();
        let template_half = Patch::cut(&half, cx / 2, cy / 2, (half_w / 2).max(2), (half_h / 2).max(2))
            .ok_or_else(|| "The point is too close to the edge".to_owned())?;
        let radius = (half_w.max(half_h) * 3 / 2).max(12);
        Ok(Self { x: cx as f64, y: cy as f64, half_w, half_h, radius, template, template_half, lost_frames: 0 })
    }

    /// The current position (frame pixels).
    pub fn position(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    /// Finds the point in the next frame.
    pub fn step(&mut self, frame: &Gray) -> TrackStep {
        let half = frame.half();
        // Searching wider while lost, to pick the point up again.
        let radius = self.radius * if self.lost_frames > 0 { 2 } else { 1 };
        let (hx, hy) = ((self.x / 2.0).round() as i64, (self.y / 2.0).round() as i64);
        let r1 = (radius / 2) as i64;
        let mut best = (f32::MIN, hx, hy);
        for dy in -r1..=r1 {
            for dx in -r1..=r1 {
                let score = self.template_half.score(&half, hx + dx, hy + dy);
                if score > best.0 {
                    best = (score, hx + dx, hy + dy);
                }
            }
        }
        let (fx, fy) = (best.1 * 2, best.2 * 2);
        let mut fine = (f32::MIN, fx, fy);
        for dy in -2..=2 {
            for dx in -2..=2 {
                let score = self.template.score(frame, fx + dx, fy + dy);
                if score > fine.0 {
                    fine = (score, fx + dx, fy + dy);
                }
            }
        }
        let (score, bx, by) = fine;
        if score < LOST_SCORE {
            self.lost_frames += 1;
            return TrackStep { x: self.x, y: self.y, score: score as f64, lost: true };
        }
        // Sub-pixel: a parabola through the peak and its neighbours.
        let refine = |minus: f32, plus: f32| {
            let curve = minus - 2.0 * score + plus;
            if curve < -1e-6 { (0.5 * (minus - plus) / curve).clamp(-0.5, 0.5) as f64 } else { 0.0 }
        };
        let sx = refine(self.template.score(frame, bx - 1, by), self.template.score(frame, bx + 1, by));
        let sy = refine(self.template.score(frame, bx, by - 1), self.template.score(frame, bx, by + 1));
        self.x = bx as f64 + sx;
        self.y = by as f64 + sy;
        self.lost_frames = 0;
        // Take the patch again where it was found, so slow changes are followed.
        if let Some(patch) = Patch::cut(frame, bx, by, self.half_w, self.half_h).filter(|patch| patch.spread() >= MIN_PATCH_SPREAD) {
            self.template = patch;
        }
        if let Some(patch) = Patch::cut(&half, bx / 2, by / 2, self.template_half.half_w, self.template_half.half_h) {
            self.template_half = patch;
        }
        TrackStep { x: self.x, y: self.y, score: score as f64, lost: false }
    }
}

/// A tracked sample: time (seconds) and position (shares of the frame, 0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackSample {
    pub time: f64,
    pub x: f64,
    pub y: f64,
}

/// Where the tracked clip sits on the canvas: its centre (canvas pixels
/// from the canvas centre) and drawn size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPlacement {
    pub position_x: f64,
    pub position_y: f64,
    pub drawn_width: f64,
    pub drawn_height: f64,
}

/// Keeps the fewest samples that stay within `tolerance` of the path
/// (Ramer–Douglas–Peucker on x and y over time, with straight lines between).
pub fn simplify(samples: &[TrackSample], tolerance: f64) -> Vec<TrackSample> {
    if samples.len() <= 2 {
        return samples.to_vec();
    }
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[samples.len() - 1] = true;
    let mut stack = vec![(0usize, samples.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        if last <= first + 1 {
            continue;
        }
        let (a, b) = (samples[first], samples[last]);
        let span = (b.time - a.time).max(1e-9);
        let mut worst = (0.0f64, first);
        for (index, sample) in samples.iter().enumerate().take(last).skip(first + 1) {
            let t = (sample.time - a.time) / span;
            let (lx, ly) = (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            let distance = (sample.x - lx).hypot(sample.y - ly);
            if distance > worst.0 {
                worst = (distance, index);
            }
        }
        if worst.0 > tolerance {
            keep[worst.1] = true;
            stack.push((first, worst.1));
            stack.push((worst.1, last));
        }
    }
    samples.iter().zip(keep).filter_map(|(sample, kept)| kept.then_some(*sample)).collect()
}

/// The follower's position keys `(time, x, y)` in canvas pixels: the tracked
/// point on the canvas plus `offset` (the follower's distance from it at the
/// first sample), simplified to `tolerance` canvas pixels.
pub fn follow_keys(samples: &[TrackSample], clip: ClipPlacement, offset: (f64, f64), tolerance: f64) -> Vec<(f64, f64, f64)> {
    let on_canvas: Vec<TrackSample> = samples
        .iter()
        .map(|sample| TrackSample {
            time: sample.time,
            x: clip.position_x + (sample.x - 0.5) * clip.drawn_width + offset.0,
            y: clip.position_y + (sample.y - 0.5) * clip.drawn_height + offset.1,
        })
        .collect();
    simplify(&on_canvas, tolerance).into_iter().map(|sample| (sample.time, sample.x, sample.y)).collect()
}

/// Where a share of the clip's frame is on the canvas.
pub fn canvas_point(clip: ClipPlacement, x: f64, y: f64) -> (f64, f64) {
    (clip.position_x + (x - 0.5) * clip.drawn_width, clip.position_y + (y - 0.5) * clip.drawn_height)
}

/// The share of the clip's frame under a canvas point (the inverse of [`canvas_point`]).
pub fn frame_point(clip: ClipPlacement, canvas_x: f64, canvas_y: f64) -> (f64, f64) {
    (
        0.5 + (canvas_x - clip.position_x) / clip.drawn_width.max(1e-9),
        0.5 + (canvas_y - clip.position_y) / clip.drawn_height.max(1e-9),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A textured background (deterministic noise) with a bright textured
    /// square of `size` at (`sx`, `sy`).
    fn scene(width: usize, height: usize, sx: f64, sy: f64, size: usize, seed: u32) -> Gray {
        let mut state = seed.wrapping_mul(2654435761).max(1);
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state % 40) as f32
        };
        let mut data: Vec<f32> = (0..width * height).map(|_| 60.0 + noise()).collect();
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = (x as f64 - sx, y as f64 - sy);
                if dx.abs() <= size as f64 / 2.0 && dy.abs() <= size as f64 / 2.0 {
                    let checker = ((x as f64 - sx + 100.0) as i64 / 4 + (y as f64 - sy + 100.0) as i64 / 4) % 2;
                    data[y * width + x] = if checker == 0 { 230.0 } else { 160.0 };
                }
            }
        }
        Gray { width, height, data }
    }

    #[test]
    fn follows_a_moving_square_without_drifting() {
        let (w, h) = (320, 180);
        let path = |i: usize| (60.0 + i as f64 * 4.0, 50.0 + (i as f64 * 0.3).sin() * 30.0 + i as f64);
        let (x0, y0) = path(0);
        let mut tracker = Tracker::new(&scene(w, h, x0, y0, 24, 1), x0, y0, 24.0, 24.0).unwrap();
        for i in 1..40 {
            let (x, y) = path(i);
            let step = tracker.step(&scene(w, h, x, y, 24, i as u32 + 1));
            assert!(!step.lost, "frame {i}: score {}", step.score);
            // The square is drawn on whole pixels, so its true centre is only known to half a pixel.
            assert!((step.x - x).abs() <= 1.5 && (step.y - y).abs() <= 1.5, "frame {i}: {step:?} vs ({x}, {y})");
        }
    }

    #[test]
    fn reports_lost_when_the_point_disappears_and_finds_it_again() {
        let (w, h) = (200, 120);
        let mut tracker = Tracker::new(&scene(w, h, 100.0, 60.0, 20, 1), 100.0, 60.0, 20.0, 20.0).unwrap();
        // The square is gone (plain noise) for two frames.
        let gone = scene(w, h, -100.0, -100.0, 20, 7);
        let step = tracker.step(&gone);
        assert!(step.lost && step.x == 100.0 && step.y == 60.0, "{step:?}");
        tracker.step(&scene(w, h, -100.0, -100.0, 20, 8));
        let back = tracker.step(&scene(w, h, 108.0, 64.0, 20, 9));
        assert!(!back.lost && (back.x - 108.0).abs() <= 1.0 && (back.y - 64.0).abs() <= 1.0, "{back:?}");
    }

    #[test]
    fn grows_the_box_to_follow_a_flat_object_by_its_edges() {
        // A plain white 24 px square over a fixed noisy background; the
        // starting box (16 px) fits inside the square.
        let (w, h) = (200, 120);
        let frame = |sx: f64| {
            let mut g = scene(w, h, -100.0, -100.0, 1, 3);
            for y in 0..h {
                for x in 0..w {
                    if (x as f64 - sx).abs() <= 12.0 && (y as f64 - 60.0).abs() <= 12.0 {
                        g.data[y * w + x] = 255.0;
                    }
                }
            }
            g
        };
        let mut tracker = Tracker::new(&frame(50.0), 50.0, 60.0, 16.0, 16.0).unwrap();
        for i in 1..=20 {
            let step = tracker.step(&frame(50.0 + i as f64 * 3.0));
            assert!(!step.lost && (step.x - (50.0 + i as f64 * 3.0)).abs() <= 1.0, "frame {i}: {step:?}");
        }
    }

    #[test]
    fn refuses_a_flat_spot_and_the_frame_edge_is_clamped() {
        let flat = Gray { width: 100, height: 100, data: vec![128.0; 10_000] };
        assert!(Tracker::new(&flat, 50.0, 50.0, 16.0, 16.0).is_err());
        let textured = scene(100, 100, 3.0, 3.0, 10, 2);
        let tracker = Tracker::new(&textured, 0.0, 0.0, 16.0, 16.0).unwrap();
        assert_eq!(tracker.position(), (8.0, 8.0));
    }

    #[test]
    fn converts_frames_and_halves_them() {
        let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
        let gray = Gray::from_rgba(&rgba, 2, 1);
        assert!((gray.data[0] - 0.299 * 255.0).abs() < 1e-3 && (gray.data[1] - 0.114 * 255.0).abs() < 1e-3);
        let bgra = [0, 0, 255, 255, 255, 0, 0, 255];
        assert_eq!(Gray::from_bgra(&bgra, 2, 1), gray);
        let half = Gray { width: 2, height: 2, data: vec![0.0, 4.0, 8.0, 12.0] }.half();
        assert_eq!((half.width, half.height, half.data[0]), (1, 1, 6.0));
    }

    #[test]
    fn simplifies_straight_runs_and_keeps_corners() {
        let samples: Vec<TrackSample> = (0..=20)
            .map(|i| {
                let t = i as f64 / 10.0;
                TrackSample { time: t, x: if t <= 1.0 { t } else { 1.0 }, y: if t <= 1.0 { 0.0 } else { t - 1.0 } }
            })
            .collect();
        let kept = simplify(&samples, 0.001);
        assert_eq!(kept.iter().map(|s| s.time).collect::<Vec<_>>(), vec![0.0, 1.0, 2.0]);
        assert_eq!(simplify(&samples[..2], 0.1).len(), 2);
    }

    #[test]
    fn follow_keys_put_the_follower_on_the_point_plus_its_offset() {
        let clip = ClipPlacement { position_x: 100.0, position_y: -50.0, drawn_width: 1000.0, drawn_height: 500.0 };
        let samples = [TrackSample { time: 0.0, x: 0.5, y: 0.5 }, TrackSample { time: 1.0, x: 0.75, y: 0.25 }];
        let keys = follow_keys(&samples, clip, (10.0, 20.0), 0.5);
        assert_eq!(keys, vec![(0.0, 110.0, -30.0), (1.0, 360.0, -155.0)]);
        let (fx, fy) = frame_point(clip, 350.0, -175.0);
        assert!((fx - 0.75).abs() < 1e-12 && (fy - 0.25).abs() < 1e-12);
        assert_eq!(canvas_point(clip, fx, fy), (350.0, -175.0));
    }
}
