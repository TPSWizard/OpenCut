//! Auto-reframe: a clip shot for one shape (16:9) is scaled to cover a
//! canvas of another (9:16) and panned so the subject stays in view. The
//! caller samples the clip, finds the subject in each sample (faces on the
//! web; movement and skin tone where no face detector runs), and this module
//! turns those samples into a camera path: per shot, a fixed crop when the
//! subject stays put, otherwise a smoothed pan (a constant-velocity Kalman
//! filter run forwards and backwards), cut sharply at shot changes. The path
//! becomes linear `transform.positionX` / `positionY` keys. The web app's
//! `src/reframe/path.ts` is the same arithmetic; keep the two in step.

/// A detected subject box, as fractions of the frame (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detection {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub score: f64,
}

/// Where the subject is in one sample: its centre as fractions of the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Subject {
    pub x: f64,
    pub y: f64,
}

/// One sample of the clip: clip seconds (from the element's start) and the
/// subject found there, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReframeSample {
    pub time: f64,
    pub subject: Option<Subject>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReframeCamera {
    /// One fixed crop per shot.
    Steady,
    /// A slow pan that follows the subject.
    #[default]
    Smooth,
    /// A quicker pan that keeps up with fast movement.
    Follow,
}

impl ReframeCamera {
    pub const ALL: [ReframeCamera; 3] = [ReframeCamera::Steady, ReframeCamera::Smooth, ReframeCamera::Follow];

    pub fn id(self) -> &'static str {
        match self {
            ReframeCamera::Steady => "steady",
            ReframeCamera::Smooth => "smooth",
            ReframeCamera::Follow => "follow",
        }
    }

    pub fn from_id(id: &str) -> Option<ReframeCamera> {
        ReframeCamera::ALL.into_iter().find(|camera| camera.id() == id)
    }

    /// Kalman process noise: how freely the virtual camera may accelerate.
    fn process_noise(self) -> f64 {
        match self {
            ReframeCamera::Steady => 0.0,
            ReframeCamera::Smooth => 2e-3,
            ReframeCamera::Follow => 3e-2,
        }
    }
}

/// Measurement noise of a subject centre (a standard deviation of 2% of the frame).
const MEASUREMENT_NOISE: f64 = 4e-4;
/// A subject that moves less than this share of the visible width keeps a fixed crop.
const STEADY_SHARE: f64 = 0.3;
/// Keys may stray this far (fraction of the frame) from the sampled path.
const KEY_TOLERANCE: f64 = 0.004;
/// The old crop holds until this long before a shot change.
const CUT_GAP_SECONDS: f64 = 0.001;
/// Faces sit this share of the visible height above the crop's centre.
const HEADROOM: f64 = 0.12;

/// The subject a frame's detections point at: the larger boxes (at least
/// 40% of the largest), averaged by area and confidence.
pub fn subject_from_detections(detections: &[Detection], min_score: f64) -> Option<Subject> {
    let kept: Vec<&Detection> = detections
        .iter()
        .filter(|d| d.score >= min_score && d.width > 0.0 && d.height > 0.0)
        .collect();
    let largest = kept.iter().map(|d| d.width * d.height).fold(0.0, f64::max);
    if largest <= 0.0 {
        return None;
    }
    let (mut sum_x, mut sum_y, mut sum_w) = (0.0, 0.0, 0.0);
    for d in kept.into_iter().filter(|d| d.width * d.height >= largest * 0.4) {
        let weight = d.width * d.height * d.score;
        sum_x += (d.x + d.width / 2.0) * weight;
        sum_y += (d.y + d.height / 2.0) * weight;
        sum_w += weight;
    }
    Some(Subject { x: (sum_x / sum_w).clamp(0.0, 1.0), y: (sum_y / sum_w).clamp(0.0, 1.0) })
}

/// The share of the source's width and height the canvas shows once the
/// source is scaled to cover it.
pub fn visible_fraction(source_width: f64, source_height: f64, canvas_width: f64, canvas_height: f64) -> (f64, f64) {
    let source = source_width / source_height;
    let canvas = canvas_width / canvas_height;
    ((canvas / source).min(1.0), (source / canvas).min(1.0))
}

/// The element scale (over the editor's contain fit) that covers the canvas.
pub fn cover_scale(source_width: f64, source_height: f64, canvas_width: f64, canvas_height: f64) -> f64 {
    let fit_x = canvas_width / source_width;
    let fit_y = canvas_height / source_height;
    fit_x.max(fit_y) / fit_x.min(fit_y)
}

/// A crop centre on the camera path, as fractions of the source frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint {
    pub time: f64,
    pub x: f64,
    pub y: f64,
}

/// The camera path through the samples: shots split at `cuts` (clip
/// seconds), each panned or held on its own.
pub fn plan_path(samples: &[ReframeSample], cuts: &[f64], visible: (f64, f64), camera: ReframeCamera) -> Vec<Vec<PathPoint>> {
    let mut sorted: Vec<ReframeSample> = samples.iter().copied().filter(|s| s.time.is_finite()).collect();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    let mut cuts: Vec<f64> = cuts.iter().copied().filter(|c| c.is_finite()).collect();
    cuts.sort_by(f64::total_cmp);
    let mut shots: Vec<Vec<ReframeSample>> = vec![Vec::new()];
    let mut next_cut = cuts.iter().peekable();
    for sample in sorted {
        while next_cut.peek().is_some_and(|cut| sample.time >= **cut) {
            next_cut.next();
            if !shots.last().is_some_and(Vec::is_empty) {
                shots.push(Vec::new());
            }
        }
        shots.last_mut().expect("one shot").push(sample);
    }
    shots
        .into_iter()
        .filter(|shot| !shot.is_empty())
        .map(|shot| {
            let times: Vec<f64> = shot.iter().map(|s| s.time).collect();
            let xs: Vec<Option<f64>> = shot.iter().map(|s| s.subject.map(|p| p.x)).collect();
            let ys: Vec<Option<f64>> =
                shot.iter().map(|s| s.subject.map(|p| p.y + HEADROOM * visible.1)).collect();
            let x = plan_axis(&times, &xs, visible.0, camera);
            let y = plan_axis(&times, &ys, visible.1, camera);
            times.iter().enumerate().map(|(i, &time)| PathPoint { time, x: x[i], y: y[i] }).collect()
        })
        .collect()
}

/// One axis of one shot: gaps filled, then held or smoothed, then kept
/// inside the frame.
fn plan_axis(times: &[f64], values: &[Option<f64>], visible: f64, camera: ReframeCamera) -> Vec<f64> {
    let half = visible / 2.0;
    if half >= 0.5 - 1e-9 {
        return vec![0.5; times.len()];
    }
    let clamp = |value: f64| value.clamp(half, 1.0 - half);
    let Some(filled) = fill_gaps(values) else { return vec![0.5; times.len()] };
    let low = filled.iter().copied().fold(f64::INFINITY, f64::min);
    let high = filled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if camera == ReframeCamera::Steady || high - low <= visible * STEADY_SHARE {
        let centre = if high - low <= visible { (low + high) / 2.0 } else { median(&filled) };
        return vec![clamp(centre); times.len()];
    }
    smooth(times, &filled, camera.process_noise()).into_iter().map(clamp).collect()
}

/// Missing values interpolated between their neighbours and held at the ends.
fn fill_gaps(values: &[Option<f64>]) -> Option<Vec<f64>> {
    let known: Vec<(usize, f64)> = values.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v))).collect();
    let (first, last) = (known.first()?, known.last()?);
    let mut out = Vec::with_capacity(values.len());
    let mut segment = 0;
    for i in 0..values.len() {
        if i <= first.0 {
            out.push(first.1);
        } else if i >= last.0 {
            out.push(last.1);
        } else {
            while known[segment + 1].0 < i {
                segment += 1;
            }
            let (a, b) = (known[segment], known[segment + 1]);
            let t = (i - a.0) as f64 / (b.0 - a.0) as f64;
            out.push(a.1 + (b.1 - a.1) * t);
        }
    }
    Some(out)
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

/// A constant-velocity Kalman filter over the measurements, then the
/// Rauch–Tung–Striebel pass backwards: a smooth path with no lag.
fn smooth(times: &[f64], measured: &[f64], q: f64) -> Vec<f64> {
    let n = measured.len();
    if n < 2 {
        return measured.to_vec();
    }
    let r = MEASUREMENT_NOISE;
    // Filtered and predicted states ([position, velocity]) and covariances ([[a, b], [b, d]]).
    let mut xf = vec![[0.0f64; 2]; n];
    let mut pf = vec![[0.0f64; 3]; n];
    let mut xp = vec![[0.0f64; 2]; n];
    let mut pp = vec![[0.0f64; 3]; n];
    xp[0] = [measured[0], 0.0];
    pp[0] = [r, 0.0, 1.0];
    for k in 0..n {
        if k > 0 {
            let dt = (times[k] - times[k - 1]).max(1e-3);
            let [x, v] = xf[k - 1];
            let [a, b, d] = pf[k - 1];
            xp[k] = [x + v * dt, v];
            pp[k] = [
                a + 2.0 * dt * b + dt * dt * d + q * dt.powi(3) / 3.0,
                b + dt * d + q * dt * dt / 2.0,
                d + q * dt,
            ];
        }
        let [a, b, d] = pp[k];
        let innovation = measured[k] - xp[k][0];
        let s = a + r;
        let (k0, k1) = (a / s, b / s);
        xf[k] = [xp[k][0] + k0 * innovation, xp[k][1] + k1 * innovation];
        pf[k] = [(1.0 - k0) * a, (1.0 - k0) * b, d - k1 * b];
    }
    let mut xs = xf.clone();
    let mut ps = pf.clone();
    for k in (0..n - 1).rev() {
        let dt = (times[k + 1] - times[k]).max(1e-3);
        let [a, b, d] = pf[k];
        // C = Pf F^T Pp^-1
        let (fa, fb, fc, fd) = (a + dt * b, b, b + dt * d, d);
        let [pa, pb, pd] = pp[k + 1];
        let det = pa * pd - pb * pb;
        if det.abs() < 1e-18 {
            continue;
        }
        let (ia, ib, id) = (pd / det, -pb / det, pa / det);
        let c = [fa * ia + fb * ib, fa * ib + fb * id, fc * ia + fd * ib, fc * ib + fd * id];
        let dx = [xs[k + 1][0] - xp[k + 1][0], xs[k + 1][1] - xp[k + 1][1]];
        xs[k] = [xf[k][0] + c[0] * dx[0] + c[1] * dx[1], xf[k][1] + c[2] * dx[0] + c[3] * dx[1]];
        let dp = [ps[k + 1][0] - pa, ps[k + 1][1] - pb, ps[k + 1][2] - pd];
        // P += C dP C^T
        let t = [c[0] * dp[0] + c[1] * dp[1], c[0] * dp[1] + c[1] * dp[2], c[2] * dp[0] + c[3] * dp[1], c[2] * dp[1] + c[3] * dp[2]];
        ps[k] = [a + t[0] * c[0] + t[1] * c[1], b + t[0] * c[2] + t[1] * c[3], d + t[2] * c[2] + t[3] * c[3]];
    }
    xs.into_iter().map(|[x, _]| x).collect()
}

/// Keys for one axis of the path, `(clip seconds, value)`: the fewest
/// points within [`KEY_TOLERANCE`] of each shot's path, with a sharp step
/// at each shot change. Empty when the value never moves.
pub fn axis_keys(shots: &[Vec<PathPoint>], axis: fn(&PathPoint) -> f64) -> Vec<(f64, f64)> {
    let mut keys: Vec<(f64, f64)> = Vec::new();
    for (index, shot) in shots.iter().enumerate() {
        let points: Vec<(f64, f64)> = shot.iter().map(|p| (p.time, axis(p))).collect();
        let Some(&(first_time, first_value)) = points.first() else { continue };
        if let Some(&(_, previous)) = keys.last() {
            keys.push(((first_time - CUT_GAP_SECONDS).max(keys.last().unwrap().0), previous));
        }
        let start = if index == 0 { 0.0f64.min(first_time) } else { first_time };
        keys.push((start, first_value));
        for (time, value) in simplify(&points) {
            if time > start + 1e-9 {
                keys.push((time, value));
            }
        }
    }
    let first = keys.first().map(|k| k.1).unwrap_or(0.0);
    if keys.iter().all(|k| (k.1 - first).abs() < 1e-6) {
        return Vec::new();
    }
    // Two keys at one time (a cut on a sample) keep the later one.
    keys.dedup_by(|later, earlier| {
        if (later.0 - earlier.0).abs() < 1e-9 {
            earlier.1 = later.1;
            true
        } else {
            false
        }
    });
    prune(keys)
}

/// Drops keys that change nothing: those on the line between their
/// neighbours, and trailing keys equal to the one before (values hold).
fn prune(keys: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(keys.len());
    for (index, &key) in keys.iter().enumerate() {
        if let (Some(&previous), Some(&next)) = (out.last(), keys.get(index + 1)) {
            let along = if next.0 > previous.0 { (key.0 - previous.0) / (next.0 - previous.0) } else { 0.0 };
            if (previous.1 + (next.1 - previous.1) * along - key.1).abs() < 1e-9 {
                continue;
            }
        }
        out.push(key);
    }
    while out.len() >= 2 && (out[out.len() - 1].1 - out[out.len() - 2].1).abs() < 1e-9 {
        out.pop();
    }
    out
}

/// Ramer–Douglas–Peucker on (time, value) with a value tolerance.
fn simplify(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (ta, va) = points[a];
        let (tb, vb) = points[b];
        let mut worst = (0.0, a);
        for (i, &(t, v)) in points.iter().enumerate().take(b).skip(a + 1) {
            let along = if tb > ta { (t - ta) / (tb - ta) } else { 0.0 };
            let error = (v - (va + (vb - va) * along)).abs();
            if error > worst.0 {
                worst = (error, i);
            }
        }
        if worst.0 > KEY_TOLERANCE {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    points.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Where the crop lands on the element: its scale, and the position for a
/// crop centre (canvas pixels from the canvas centre, as the renderer reads
/// `transform.positionX` / `positionY`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReframeGeometry {
    pub scale: f64,
    pub visible: (f64, f64),
    /// The covering source's size on the canvas, in canvas pixels.
    pub shown_width: f64,
    pub shown_height: f64,
}

impl ReframeGeometry {
    pub fn new(source_width: f64, source_height: f64, canvas_width: f64, canvas_height: f64) -> Self {
        let cover = (canvas_width / source_width).max(canvas_height / source_height);
        Self {
            scale: cover_scale(source_width, source_height, canvas_width, canvas_height),
            visible: visible_fraction(source_width, source_height, canvas_width, canvas_height),
            shown_width: source_width * cover,
            shown_height: source_height * cover,
        }
    }

    pub fn position_x(&self, centre: f64) -> f64 {
        (0.5 - centre) * self.shown_width
    }

    pub fn position_y(&self, centre: f64) -> f64 {
        (0.5 - centre) * self.shown_height
    }

    /// True when the canvas shows the whole source (nothing to reframe).
    pub fn is_noop(&self) -> bool {
        self.visible.0 > 0.999 && self.visible.1 > 0.999
    }
}

/// The finished reframe for one clip: the scale, static positions for
/// axes that never move, and position keys for those that do.
#[derive(Clone, Debug, PartialEq)]
pub struct ReframePlan {
    pub scale: f64,
    pub position_x: f64,
    pub position_y: f64,
    /// `(clip seconds, canvas pixels)`; empty when the axis holds still.
    pub keys_x: Vec<(f64, f64)>,
    pub keys_y: Vec<(f64, f64)>,
    pub shots: usize,
    pub samples_with_subject: usize,
}

pub fn plan_reframe(samples: &[ReframeSample], cuts: &[f64], geometry: &ReframeGeometry, camera: ReframeCamera) -> ReframePlan {
    let shots = plan_path(samples, cuts, geometry.visible, camera);
    let first = shots.first().and_then(|shot| shot.first()).copied().unwrap_or(PathPoint { time: 0.0, x: 0.5, y: 0.5 });
    let keys_x: Vec<(f64, f64)> = axis_keys(&shots, |p| p.x).into_iter().map(|(t, v)| (t, geometry.position_x(v))).collect();
    let keys_y: Vec<(f64, f64)> = axis_keys(&shots, |p| p.y).into_iter().map(|(t, v)| (t, geometry.position_y(v))).collect();
    ReframePlan {
        scale: geometry.scale,
        position_x: geometry.position_x(first.x),
        position_y: geometry.position_y(first.y),
        keys_x,
        keys_y,
        shots: shots.len(),
        samples_with_subject: samples.iter().filter(|s| s.subject.is_some()).count(),
    }
}

/// Width of the small frames [`shot_subjects`] works on (height follows the source).
pub const SUBJECT_FRAME_WIDTH: usize = 80;

fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64
}

/// A chrominance range that holds across skin tones (lighting, and brown or
/// wooden backgrounds, can still fool it).
fn is_skin(r: u8, g: u8, b: u8) -> bool {
    let (r, g, b) = (r as f64, g as f64, b as f64);
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b;
    let cr = 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b;
    y > 40.0 && (77.0..=127.0).contains(&cb) && (133.0..=173.0).contains(&cr)
}

/// Subject guesses for one shot without a face detector, from small RGB
/// frames of equal size: each pixel's median brightness over the shot is
/// the background, and the subject is the centre of what differs from it
/// (skin-coloured pixels count three times). A shot where nothing stands
/// out (a still talking head) falls back to the centre of the skin-coloured
/// pixels; `None` where neither finds enough.
pub fn shot_subjects(frames: &[&[u8]], width: usize, height: usize) -> Vec<Option<Subject>> {
    let pixels = width * height;
    if pixels == 0 || frames.iter().any(|f| f.len() < pixels * 3) {
        return vec![None; frames.len()];
    }
    let background: Option<Vec<f64>> = (frames.len() >= 3).then(|| {
        let mut column = vec![0.0f64; frames.len()];
        (0..pixels)
            .map(|p| {
                for (slot, frame) in column.iter_mut().zip(frames) {
                    *slot = luma(frame[p * 3], frame[p * 3 + 1], frame[p * 3 + 2]);
                }
                let middle = column.len() / 2;
                *column.select_nth_unstable_by(middle, f64::total_cmp).1
            })
            .collect()
    });
    frames
        .iter()
        .map(|frame| {
            let (mut fx, mut fy, mut fw) = (0.0, 0.0, 0.0);
            let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
            for p in 0..pixels {
                let (r, g, b) = (frame[p * 3], frame[p * 3 + 1], frame[p * 3 + 2]);
                let skin = is_skin(r, g, b);
                let (col, row) = ((p % width) as f64 + 0.5, (p / width) as f64 + 0.5);
                if background.as_ref().is_some_and(|bg| (luma(r, g, b) - bg[p]).abs() > 25.0) {
                    let weight = if skin { 3.0 } else { 1.0 };
                    fx += col * weight;
                    fy += row * weight;
                    fw += weight;
                }
                if skin {
                    sx += col;
                    sy += row;
                    sw += 1.0;
                }
            }
            let centre = |x: f64, y: f64, w: f64| Subject { x: x / w / width as f64, y: y / w / height as f64 };
            if fw >= pixels as f64 * 0.004 {
                Some(centre(fx, fy, fw))
            } else if sw >= pixels as f64 * 0.005 {
                Some(centre(sx, sy, sw))
            } else {
                None
            }
        })
        .collect()
}

/// Clip seconds (from the element's start) at which `source_offset`
/// seconds past the trim start play, following any speed change.
pub fn clip_seconds_for_source(retime: Option<&crate::Retime>, source_offset: f64, clip_duration: f64) -> f64 {
    match retime {
        None => source_offset,
        Some(retime) if retime.reverse && clip_duration > 0.0 => {
            retime.inverse((retime.forward(clip_duration) - source_offset).max(0.0))
        }
        Some(retime) => retime.inverse(source_offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(time: f64, x: Option<f64>) -> ReframeSample {
        ReframeSample { time, subject: x.map(|x| Subject { x, y: 0.5 }) }
    }

    #[test]
    fn geometry_of_a_landscape_clip_on_a_portrait_canvas() {
        let g = ReframeGeometry::new(1920.0, 1080.0, 1080.0, 1920.0);
        assert!((g.scale - (1920.0 / 1080.0) * (1920.0 / 1080.0)).abs() < 1e-9);
        assert!((g.visible.0 - 0.31640625).abs() < 1e-9);
        assert_eq!(g.visible.1, 1.0);
        assert!((g.shown_height - 1920.0).abs() < 1e-9);
        assert!((g.position_x(0.5)).abs() < 1e-9);
        // A subject at the left edge's crop limit moves the picture right.
        assert!(g.position_x(g.visible.0 / 2.0) > 0.0);
        assert!(ReframeGeometry::new(1920.0, 1080.0, 1280.0, 720.0).is_noop());
    }

    #[test]
    fn largest_faces_lead() {
        let big = Detection { x: 0.6, y: 0.2, width: 0.2, height: 0.3, score: 0.9 };
        let small = Detection { x: 0.0, y: 0.0, width: 0.05, height: 0.05, score: 0.95 };
        let weak = Detection { x: 0.1, y: 0.1, width: 0.4, height: 0.4, score: 0.2 };
        let subject = subject_from_detections(&[big, small, weak], 0.5).unwrap();
        assert!((subject.x - 0.7).abs() < 1e-9 && (subject.y - 0.35).abs() < 1e-9);
        assert_eq!(subject_from_detections(&[weak], 0.5), None);
    }

    #[test]
    fn a_still_subject_gets_one_fixed_crop() {
        let samples: Vec<_> = (0..50).map(|i| sample(i as f64 * 0.2, Some(0.7 + (i % 3) as f64 * 0.01))).collect();
        let g = ReframeGeometry::new(1920.0, 1080.0, 1080.0, 1920.0);
        let plan = plan_reframe(&samples, &[], &g, ReframeCamera::Smooth);
        assert!(plan.keys_x.is_empty() && plan.keys_y.is_empty());
        assert!((plan.position_x - g.position_x(0.71)).abs() < 1e-6);
        assert_eq!(plan.samples_with_subject, 50);
    }

    #[test]
    fn a_moving_subject_is_followed_smoothly_and_stays_framed() {
        // Walks from the left third to the right third over 6 s.
        let samples: Vec<_> = (0..=60).map(|i| sample(i as f64 * 0.1, Some(0.2 + 0.6 * i as f64 / 60.0))).collect();
        let visible = visible_fraction(1920.0, 1080.0, 1080.0, 1920.0);
        let shots = plan_path(&samples, &[], visible, ReframeCamera::Smooth);
        assert_eq!(shots.len(), 1);
        let path = &shots[0];
        for pair in path.windows(2) {
            assert!(pair[1].x >= pair[0].x - 1e-3, "no backtracking: {pair:?}");
        }
        for (point, s) in path.iter().zip(&samples) {
            let subject = s.subject.unwrap().x;
            assert!((point.x - subject).abs() < visible.0 / 2.0, "subject in view at {}", point.time);
            assert!(point.x >= visible.0 / 2.0 - 1e-9 && point.x <= 1.0 - visible.0 / 2.0 + 1e-9);
        }
        let keys = axis_keys(&shots, |p| p.x);
        assert!(keys.len() >= 2 && keys.len() < 20, "{} keys", keys.len());
    }

    #[test]
    fn shots_cut_sharply_and_gaps_are_filled() {
        let mut samples: Vec<_> = (0..20).map(|i| sample(i as f64 * 0.1, Some(0.25))).collect();
        samples.extend((20..40).map(|i| sample(i as f64 * 0.1, if i % 4 == 0 { None } else { Some(0.75) })));
        let g = ReframeGeometry::new(1920.0, 1080.0, 1080.0, 1920.0);
        let plan = plan_reframe(&samples, &[2.0], &g, ReframeCamera::Smooth);
        assert_eq!(plan.shots, 2);
        let x = &plan.keys_x;
        assert_eq!(x.len(), 3, "{x:?}");
        assert_eq!(x[0], (0.0, g.position_x(0.25)));
        assert!((x[1].0 - 1.999).abs() < 1e-9 && x[1].1 == g.position_x(0.25));
        assert_eq!(x[2], (2.0, g.position_x(0.75)));
        // Without a cut the change is a pan, not a jump.
        let panned = plan_reframe(&samples, &[], &g, ReframeCamera::Follow);
        assert!(panned.keys_x.len() > 3);
    }

    #[test]
    fn steady_holds_and_no_subject_centres() {
        let samples: Vec<_> = (0..=30).map(|i| sample(i as f64 * 0.1, Some(0.1 + 0.8 * i as f64 / 30.0))).collect();
        let visible = visible_fraction(1920.0, 1080.0, 1080.0, 1920.0);
        let steady = plan_path(&samples, &[], visible, ReframeCamera::Steady);
        assert!(steady[0].iter().all(|p| (p.x - steady[0][0].x).abs() < 1e-12));
        let empty: Vec<_> = (0..10).map(|i| sample(i as f64, None)).collect();
        let centred = plan_path(&empty, &[], visible, ReframeCamera::Smooth);
        assert!(centred[0].iter().all(|p| p.x == 0.5));
    }

    /// Frames of a skin-coloured square walking right over a skin-coloured
    /// floor (the case plain skin detection gets wrong). Mirrored in the
    /// web app's `src/reframe/__tests__/path.test.ts`.
    pub(super) fn walking_frames() -> (Vec<Vec<u8>>, usize, usize) {
        let (w, h) = (40, 24);
        let frames = (0..10)
            .map(|i| {
                let mut f = [96u8, 110, 128].repeat(w * h);
                for row in 0..h {
                    for col in 0..w {
                        let p = (row * w + col) * 3;
                        if row >= 17 {
                            f[p..p + 3].copy_from_slice(&[80, 70, 60]);
                        }
                        let left = 4 + i * 3;
                        if (left..left + 5).contains(&col) && (5..11).contains(&row) {
                            f[p..p + 3].copy_from_slice(&[205, 150, 120]);
                        }
                    }
                }
                f
            })
            .collect();
        (frames, w, h)
    }

    #[test]
    fn the_subject_is_what_stands_out_from_the_shot() {
        let (frames, w, h) = walking_frames();
        let refs: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
        let subjects = shot_subjects(&refs, w, h);
        for (i, subject) in subjects.iter().enumerate() {
            let expected = (4.0 + i as f64 * 3.0 + 2.5) / w as f64;
            let x = subject.unwrap().x;
            assert!((x - expected).abs() < 0.03, "frame {i}: {x} vs {expected}");
        }
        // A still shot falls back to the skin-coloured pixels (here the
        // square and the floor alike: the limit of colour alone).
        let still = vec![refs[2]; 4];
        assert!(shot_subjects(&still, w, h)[0].is_some());
        // A darker skin tone on its own is found too.
        let mut deep = [30u8, 30, 30].repeat(w * h);
        for row in 2..8 {
            for col in 2..8 {
                let p = (row * w + col) * 3;
                deep[p..p + 3].copy_from_slice(&[120, 80, 60]);
            }
        }
        assert!(shot_subjects(&[deep.as_slice()], w, h)[0].unwrap().x < 0.25);
        assert_eq!(shot_subjects(&[[30u8, 30, 30].repeat(w * h).as_slice()], w, h), vec![None]);
    }

    #[test]
    fn source_offsets_map_through_speed_changes() {
        assert_eq!(clip_seconds_for_source(None, 3.0, 10.0), 3.0);
        let double = crate::Retime::constant(2.0);
        assert!((clip_seconds_for_source(Some(&double), 3.0, 10.0) - 1.5).abs() < 1e-9);
        let reverse = crate::Retime { reverse: true, ..crate::Retime::constant(1.0) };
        assert!((clip_seconds_for_source(Some(&reverse), 3.0, 10.0) - 7.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod parity {
    use super::*;

    /// The same input and expected keys are in the web app's
    /// `src/reframe/__tests__/path.test.ts`; both must agree.
    #[test]
    fn matches_the_web_reference() {
        let samples: Vec<ReframeSample> = (0..=40)
            .map(|i| {
                let t = i as f64 * 0.15;
                let subject = if i % 7 == 3 { None } else { Some(Subject { x: 0.3 + 0.4 * (t / 6.0) + 0.02 * (t * 3.0).sin(), y: 0.45 }) };
                ReframeSample { time: t, subject }
            })
            .collect();
        let g = ReframeGeometry::new(1920.0, 1080.0, 1080.0, 1920.0);
        let plan = plan_reframe(&samples, &[3.0], &g, ReframeCamera::Smooth);
        let rounded: Vec<(f64, f64)> = plan.keys_x.iter().map(|(t, v)| ((t * 1000.0).round() / 1000.0, (v * 100.0).round() / 100.0)).collect();
        let expected = [
            (0.0, 652.53),
            (0.6, 522.15),
            (1.65, 333.56),
            (2.85, -33.24),
            (2.999, -33.24),
            (3.0, 3.59),
            (3.75, -148.13),
            (4.8, -434.52),
            (6.0, -615.72),
        ];
        assert_eq!(rounded, expected);
        assert!((plan.scale - 3.160494).abs() < 1e-6);
    }
}
