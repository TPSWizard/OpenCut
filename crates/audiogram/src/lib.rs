//! Audiograms (row 100): a picture that moves with a clip's sound, for
//! podcasts and music. Both editors use this crate (the web through WASM),
//! so an audiogram analyses and draws the same in each.
//!
//! **Analysis** ([`analyse`]): the clip's sound (mono, any sample rate; both
//! editors decode at [`SAMPLE_RATE`]) is looked at once per video frame
//! (at [`FPS`]): a Hann-windowed FFT about [`WINDOW_SECONDS`] long centred
//! on the frame's source time, its peak in each of [`BANDS`] log-spaced
//! bands from [`MIN_HZ`] to [`MAX_HZ`] in decibels (a full-scale sine is
//! 0 dB), mapped from [`FLOOR_DB`]..0 dB to 0..1, then held and let fall
//! at [`FALL_PER_SECOND`] so bars drop smoothly. Kept on the element as
//! the `levels` param ([`Levels::encode`]: `ag1:<fps>:<bands>:<base64>`,
//! one byte per band per frame).
//!
//! **Drawing** ([`render`]): at a time into the element the levels are
//! read between frames, spread over the bar count, and drawn anti-aliased
//! into a straight-alpha RGBA raster the shape of the canvas, in one of
//! four [`Style`]s. The look is the element's params ([`Look`]). Checked
//! against `fixtures/audiogram/cases.json` (an independent Python
//! version).

/// The graphic definition id of an audiogram element.
pub const DEFINITION: &str = "audiogram";
/// The sample rate both editors decode the sound at for analysis.
pub const SAMPLE_RATE: u32 = 24_000;
/// Level frames a second.
pub const FPS: f64 = 30.0;
/// Frequency bands.
pub const BANDS: usize = 32;
pub const MIN_HZ: f64 = 50.0;
pub const MAX_HZ: f64 = 10_000.0;
/// About this long a window per frame (rounded to a power of two samples).
pub const WINDOW_SECONDS: f64 = 0.04;
/// Levels at or below this are 0.
pub const FLOOR_DB: f64 = -60.0;
/// How fast a level falls back (full scale a second).
pub const FALL_PER_SECOND: f64 = 1.5;
/// The longest stretch of sound one audiogram analyses.
pub const MAX_SECONDS: f64 = 600.0;
/// The largest side a raster is drawn at.
pub const MAX_RASTER_SIDE: u32 = 1920;

/// Levels by frame and band, 0 to 255.
#[derive(Clone, Debug, PartialEq)]
pub struct Levels {
    pub fps: f64,
    pub bands: usize,
    /// Frame-major: `data[frame * bands + band]`.
    pub data: Vec<u8>,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(BASE64[(n >> 18) as usize & 63] as char);
        out.push(BASE64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64[n as usize & 63] as char } else { '=' });
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| BASE64.iter().position(|&b| b == c).map(|p| p as u32);
    let bytes: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = n << 6 | value(c)?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

impl Levels {
    pub fn frames(&self) -> usize {
        self.data.len().checked_div(self.bands).unwrap_or(0)
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / self.fps
    }

    pub fn encode(&self) -> String {
        format!("ag1:{}:{}:{}", self.fps, self.bands, base64_encode(&self.data))
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().splitn(4, ':');
        if parts.next()? != "ag1" {
            return None;
        }
        let fps: f64 = parts.next()?.parse().ok()?;
        let bands: usize = parts.next()?.parse().ok()?;
        let data = base64_decode(parts.next()?)?;
        (fps.is_finite() && fps > 0.0 && (1..=256).contains(&bands) && !data.is_empty() && data.len() % bands == 0)
            .then_some(Self { fps, bands, data })
    }

    /// Each band's level (0 to 1) `seconds` into the element, read between
    /// frames; the first or last frame outside them.
    pub fn at(&self, seconds: f64) -> Vec<f64> {
        let frames = self.frames();
        if frames == 0 {
            return vec![0.0; self.bands];
        }
        let position = if seconds.is_finite() { (seconds * self.fps).clamp(0.0, (frames - 1) as f64) } else { 0.0 };
        let first = position.floor() as usize;
        let next = (first + 1).min(frames - 1);
        let t = position - first as f64;
        (0..self.bands)
            .map(|band| {
                let a = self.data[first * self.bands + band] as f64 / 255.0;
                let b = self.data[next * self.bands + band] as f64 / 255.0;
                a + (b - a) * t
            })
            .collect()
    }
}

/// In-place iterative radix-2 FFT (`re.len()` a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (angle * k as f64).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// The window length in samples at a sample rate: the power of two
/// nearest [`WINDOW_SECONDS`].
pub fn window_size(sample_rate: f64) -> usize {
    let wanted = (sample_rate * WINDOW_SECONDS).max(16.0);
    let lower = 1usize << (wanted.log2().floor() as u32);
    if wanted / lower as f64 > 1.5 { lower * 2 } else { lower }
}

/// Analyses mono `samples` at `sample_rate`: one frame of `bands` levels
/// for each source time in `times` (seconds), the frames `fps` apart.
pub fn analyse(samples: &[f32], sample_rate: f64, times: &[f64], bands: usize, fps: f64) -> Levels {
    let bands = bands.clamp(1, 256);
    let n = window_size(sample_rate);
    let window: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
    let top = MAX_HZ.min(sample_rate / 2.0);
    let edges: Vec<f64> = (0..=bands).map(|k| MIN_HZ * (top / MIN_HZ).powf(k as f64 / bands as f64)).collect();
    let bin_hz = sample_rate / n as f64;
    // Each band's bins, or the bin nearest its middle when it has none.
    let band_bins: Vec<(usize, usize)> = (0..bands)
        .map(|k| {
            let first = (edges[k] / bin_hz).ceil() as usize;
            let end = ((edges[k + 1] / bin_hz).ceil() as usize).min(n / 2);
            if first < end {
                (first, end)
            } else {
                let middle = ((edges[k] * edges[k + 1]).sqrt() / bin_hz).round() as usize;
                let middle = middle.min(n / 2 - 1);
                (middle, middle + 1)
            }
        })
        .collect();
    let fall = FALL_PER_SECOND / fps;
    let mut held = vec![0.0f64; bands];
    let mut data = Vec::with_capacity(times.len() * bands);
    let (mut re, mut im) = (vec![0.0; n], vec![0.0; n]);
    for &time in times {
        let centre = (time * sample_rate).round() as i64;
        let start = centre - (n / 2) as i64;
        for i in 0..n {
            let index = start + i as i64;
            re[i] = if index >= 0 && (index as usize) < samples.len() { samples[index as usize] as f64 * window[i] } else { 0.0 };
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for (band, &(first, end)) in band_bins.iter().enumerate() {
            let peak = (first..end).map(|bin| (re[bin] * re[bin] + im[bin] * im[bin]).sqrt()).fold(0.0, f64::max) * 4.0 / n as f64;
            let db = 20.0 * peak.max(1e-12).log10();
            let level = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
            held[band] = level.max(held[band] - fall);
            data.push((held[band] * 255.0 + 0.5).floor() as u8);
        }
    }
    Levels { fps, bands, data }
}

/// How the levels are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Bars rising from a baseline.
    Bars,
    /// Bars reaching up and down from a centre line (a waveform look).
    Mirror,
    /// A line through the bar tops.
    Line,
    /// Bars pointing out from a circle.
    Circle,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Bars, Style::Mirror, Style::Line, Style::Circle];

    pub fn id(self) -> &'static str {
        match self {
            Style::Bars => "bars",
            Style::Mirror => "mirror",
            Style::Line => "line",
            Style::Circle => "circle",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Style::Bars => "Bars",
            Style::Mirror => "Mirrored bars",
            Style::Line => "Line",
            Style::Circle => "Circle",
        }
    }

    pub fn parse(id: &str) -> Option<Style> {
        Style::ALL.into_iter().find(|style| style.id() == id)
    }
}

/// The element's look (its params).
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub style: Style,
    /// Straight RGBA, 0 to 1.
    pub color: [f64; 4],
    /// 4 to 128.
    pub bar_count: usize,
    /// The longest bar, percent of the canvas height (1 to 100).
    pub size: f64,
    /// The row's width, percent of the canvas width (5 to 100); for a
    /// circle, its inner diameter is half this share of the canvas's
    /// smaller side.
    pub width: f64,
    /// The baseline or centre, percent of the canvas height from the top.
    pub y: f64,
    /// The gap between bars, percent of each bar's slot (0 to 90).
    pub gap: f64,
}

impl Default for Look {
    fn default() -> Self {
        Self { style: Style::Mirror, color: [1.0; 4], bar_count: 48, size: 20.0, width: 70.0, y: 75.0, gap: 30.0 }
    }
}

/// The param keys and their defaults, in order.
pub fn default_params() -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    vec![
        ("style", json!("mirror")),
        ("color", json!("#ffffff")),
        ("barCount", json!(48)),
        ("size", json!(20)),
        ("width", json!(70)),
        ("y", json!(75)),
        ("gap", json!(30)),
    ]
}

/// Parses `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.
pub fn parse_color(text: &str) -> Option<[f64; 4]> {
    let hex = text.trim().strip_prefix('#')?;
    let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
    let bytes: Vec<u8> = match digits.len() {
        3 | 4 => digits.iter().map(|d| d * 17).collect(),
        6 | 8 => digits.chunks(2).map(|pair| pair[0] * 16 + pair[1]).collect(),
        _ => return None,
    };
    Some([bytes[0] as f64 / 255.0, bytes[1] as f64 / 255.0, bytes[2] as f64 / 255.0, *bytes.get(3).unwrap_or(&255) as f64 / 255.0])
}

impl Look {
    /// Reads the look from params (`text` and `number` look a key up).
    pub fn from_lookup(text: impl Fn(&str) -> Option<String>, number: impl Fn(&str) -> Option<f64>) -> Self {
        let d = Self::default();
        let num = |key: &str, default: f64, min: f64, max: f64| match number(key) {
            Some(v) if v.is_finite() => v.clamp(min, max),
            _ => default,
        };
        Self {
            style: text("style").and_then(|id| Style::parse(&id)).unwrap_or(d.style),
            color: text("color").and_then(|c| parse_color(&c)).unwrap_or(d.color),
            bar_count: num("barCount", d.bar_count as f64, 4.0, 128.0).round() as usize,
            size: num("size", d.size, 1.0, 100.0),
            width: num("width", d.width, 5.0, 100.0),
            y: num("y", d.y, 0.0, 100.0),
            gap: num("gap", d.gap, 0.0, 90.0),
        }
    }

    /// Reads the look from a JSON object of params.
    pub fn from_params_json(json: &str) -> Self {
        let value: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
        let object = value.as_object().cloned().unwrap_or_default();
        Self::from_lookup(
            |key| object.get(key).and_then(|v| v.as_str().map(str::to_owned)),
            |key| object.get(key).and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())),
        )
    }
}

/// The raster size for a canvas: its shape, the longer side at most `max_side`.
pub fn raster_size(canvas_width: u32, canvas_height: u32, max_side: u32) -> (u32, u32) {
    let (w, h) = (canvas_width.max(1) as f64, canvas_height.max(1) as f64);
    let scale = (max_side.max(1) as f64 / w.max(h)).min(1.0);
    (((w * scale).round() as u32).max(1), ((h * scale).round() as u32).max(1))
}

/// The bars' lengths as shares of the longest (0 to 1) at `seconds`.
pub fn bar_levels(levels: &Levels, bar_count: usize, seconds: f64) -> Vec<f64> {
    let bands = levels.at(seconds);
    let count = bar_count.max(1);
    (0..count)
        .map(|i| {
            if bands.len() < 2 || count < 2 {
                return bands.first().copied().unwrap_or(0.0);
            }
            let position = i as f64 * (bands.len() - 1) as f64 / (count - 1) as f64;
            let first = position.floor() as usize;
            let next = (first + 1).min(bands.len() - 1);
            bands[first] + (bands[next] - bands[first]) * (position - first as f64)
        })
        .collect()
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

/// Draws the audiogram `seconds` into the element: straight-alpha RGBA,
/// `width` x `height` (the canvas's shape).
pub fn render(levels: &Levels, look: &Look, seconds: f64, width: usize, height: usize) -> Vec<u8> {
    let (w, h) = (width as f64, height as f64);
    let mut cover = vec![0.0f64; width * height];
    let bars = bar_levels(levels, look.bar_count, seconds);
    let count = bars.len();
    let min_length = (0.006 * h).max(1.0);
    let longest = look.size / 100.0 * h;
    let lengths: Vec<f64> = bars.iter().map(|level| (level * longest).max(min_length)).collect();
    let base = look.y / 100.0 * h;
    let left = w * (1.0 - look.width / 100.0) / 2.0;
    let slot = w * look.width / 100.0 / count as f64;
    let thickness = slot * (1.0 - look.gap / 100.0);
    let mut put = |x: usize, y: usize, value: f64| {
        let cell = &mut cover[y * width + x];
        *cell = cell.max(value.clamp(0.0, 1.0));
    };
    match look.style {
        Style::Bars | Style::Mirror => {
            for (i, &length) in lengths.iter().enumerate() {
                let x0 = left + i as f64 * slot + (slot - thickness) / 2.0;
                let x1 = x0 + thickness;
                let (y0, y1) = if look.style == Style::Bars { (base - length, base) } else { (base - length / 2.0, base + length / 2.0) };
                let (px0, px1) = (x0.floor().max(0.0) as usize, (x1.ceil().max(0.0) as usize).min(width));
                let (py0, py1) = (y0.floor().max(0.0) as usize, (y1.ceil().max(0.0) as usize).min(height));
                for y in py0..py1 {
                    let oy = overlap(y as f64, y as f64 + 1.0, y0, y1);
                    for x in px0..px1 {
                        put(x, y, overlap(x as f64, x as f64 + 1.0, x0, x1) * oy);
                    }
                }
            }
        }
        Style::Line => {
            let half = (0.005 * h).max(1.5) / 2.0;
            let points: Vec<(f64, f64)> =
                lengths.iter().enumerate().map(|(i, length)| (left + (i as f64 + 0.5) * slot, base - length)).collect();
            for pair in points.windows(2) {
                let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
                let pad = half + 1.0;
                let (px0, px1) = ((ax.min(bx) - pad).floor().max(0.0) as usize, ((ax.max(bx) + pad).ceil().max(0.0) as usize).min(width));
                let (py0, py1) = ((ay.min(by) - pad).floor().max(0.0) as usize, ((ay.max(by) + pad).ceil().max(0.0) as usize).min(height));
                let (dx, dy) = (bx - ax, by - ay);
                let length_sq = dx * dx + dy * dy;
                for y in py0..py1 {
                    for x in px0..px1 {
                        let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                        let t = if length_sq > 0.0 { (((cx - ax) * dx + (cy - ay) * dy) / length_sq).clamp(0.0, 1.0) } else { 0.0 };
                        let distance = ((cx - ax - t * dx).powi(2) + (cy - ay - t * dy).powi(2)).sqrt();
                        put(x, y, half + 0.5 - distance);
                    }
                }
            }
        }
        Style::Circle => {
            let (cx, cy) = (w / 2.0, base);
            let inner = look.width / 100.0 * w.min(h) / 4.0;
            let reach = inner + longest.max(min_length) + 1.0;
            let half_angle = std::f64::consts::PI / count as f64 * (1.0 - look.gap / 100.0);
            let (px0, px1) = ((cx - reach).floor().max(0.0) as usize, ((cx + reach).ceil().max(0.0) as usize).min(width));
            let (py0, py1) = ((cy - reach).floor().max(0.0) as usize, ((cy + reach).ceil().max(0.0) as usize).min(height));
            let turn = 2.0 * std::f64::consts::PI;
            for y in py0..py1 {
                for x in px0..px1 {
                    let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                    let r = (dx * dx + dy * dy).sqrt();
                    if r < inner - 1.0 {
                        continue;
                    }
                    // Bar 0 points up; they go round clockwise.
                    let angle = (dy.atan2(dx) + std::f64::consts::FRAC_PI_2).rem_euclid(turn);
                    let index = ((angle / turn * count as f64).round() as usize) % count;
                    let centre = index as f64 * turn / count as f64;
                    let mut off = (angle - centre).abs();
                    if off > std::f64::consts::PI {
                        off = turn - off;
                    }
                    let across = (0.5 - (off - half_angle) * r).clamp(0.0, 1.0);
                    let along = ((r - inner).min(inner + lengths[index] - r) + 0.5).clamp(0.0, 1.0);
                    put(x, y, across * along);
                }
            }
        }
    }
    let mut out = vec![0u8; width * height * 4];
    let rgb = [to_byte(look.color[0]), to_byte(look.color[1]), to_byte(look.color[2])];
    for (i, &c) in cover.iter().enumerate() {
        if c > 0.0 {
            out[i * 4..i * 4 + 3].copy_from_slice(&rgb);
            out[i * 4 + 3] = to_byte(c * look.color[3]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_round_trip() {
        for len in [32usize, 33, 34, 64 * 7] {
            let levels = Levels { fps: 30.0, bands: if len % 32 == 0 { 32 } else { 1 }, data: (0..len).map(|i| (i * 37 % 256) as u8).collect() };
            assert_eq!(Levels::parse(&levels.encode()), Some(levels));
        }
        assert_eq!(Levels::parse("ag2:30:32:AAAA"), None);
        assert_eq!(Levels::parse("ag1:30:3:AAAA"), Some(Levels { fps: 30.0, bands: 3, data: vec![0, 0, 0] }));
        assert_eq!(Levels::parse("ag1:30:2:AAAA"), None, "3 bytes are not whole frames of 2");
    }

    #[test]
    fn a_tone_lights_its_band() {
        let rate = SAMPLE_RATE as f64;
        // A full-scale 1 kHz tone for one second.
        let samples: Vec<f32> = (0..SAMPLE_RATE).map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / rate).sin() as f32).collect();
        let levels = analyse(&samples, rate, &[0.5], BANDS, FPS);
        let edges: Vec<f64> = (0..=BANDS).map(|k| MIN_HZ * (MAX_HZ / MIN_HZ).powf(k as f64 / BANDS as f64)).collect();
        let band = edges.windows(2).position(|e| e[0] <= 1000.0 && 1000.0 < e[1]).unwrap();
        let loudest = (0..BANDS).max_by_key(|&b| levels.data[b]).unwrap();
        assert_eq!(loudest, band);
        assert!(levels.data[band] > 245, "{}", levels.data[band]);
        assert!(levels.data[0] < 60 && levels.data[BANDS - 1] < 60);
        // Silence falls back at the set rate.
        let quiet = analyse(&samples, rate, &[0.5, 2.0, 2.0 + 1.0 / FPS], BANDS, FPS);
        let drop = quiet.data[BANDS + band] as i32 - quiet.data[2 * BANDS + band] as i32;
        assert!((12..=13).contains(&drop), "fell {drop}");
    }

    #[test]
    fn window_is_about_forty_milliseconds() {
        assert_eq!(window_size(24_000.0), 1024);
        assert_eq!(window_size(48_000.0), 2048);
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        analysis: Vec<AnalysisCase>,
        renders: Vec<RenderCase>,
    }
    #[derive(serde::Deserialize)]
    struct AnalysisCase {
        name: String,
        sample_rate: f64,
        samples: Vec<f32>,
        times: Vec<f64>,
        bands: usize,
        fps: f64,
        levels: Vec<u8>,
    }
    #[derive(serde::Deserialize)]
    struct RenderCase {
        name: String,
        levels: String,
        params: serde_json::Value,
        seconds: f64,
        width: usize,
        height: usize,
        alpha: Vec<u8>,
    }

    #[test]
    fn matches_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audiogram/cases.json");
        let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(fixture.analysis.len() >= 3 && fixture.renders.len() >= 6);
        for case in fixture.analysis {
            let got = analyse(&case.samples, case.sample_rate, &case.times, case.bands, case.fps);
            let worst = got.data.iter().zip(&case.levels).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert_eq!(got.data.len(), case.levels.len(), "{}", case.name);
            assert!(worst <= 1, "{}: off by {worst}", case.name);
        }
        for case in fixture.renders {
            let levels = Levels::parse(&case.levels).unwrap();
            let look = Look::from_params_json(&case.params.to_string());
            let rgba = render(&levels, &look, case.seconds, case.width, case.height);
            let worst = rgba.chunks_exact(4).zip(&case.alpha).map(|(p, a)| (p[3] as i32 - *a as i32).abs()).max().unwrap();
            assert!(worst <= 1, "{}: off by {worst}", case.name);
        }
    }
}
