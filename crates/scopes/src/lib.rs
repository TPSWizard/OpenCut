//! Video scopes: a waveform, an RGB parade, a vectorscope and a histogram
//! of a frame, drawn as a small RGBA picture. The desktop measures the
//! preview frame its renderer made; the web measures its preview canvas
//! through WASM. Both draw the same picture from the same pixels.
//!
//! The frame is point-sampled on a square grid, every `step`th pixel in
//! both directions, with `step` chosen so at most [`MAX_COLUMNS`] columns
//! are read. Luma is Rec. 709 in whole numbers: `(54 R + 183 G + 19 B +
//! 128) >> 8`. The vectorscope plots Rec. 709 Cb (B − Y) across and Cr
//! (R − Y) up, a fully saturated colour on the outer ring. Brightness of a
//! trace grows with the square root of how many samples land on it.
//!
//! An independent Python reference (`fixtures/scopes/cases.json`) checks
//! the sampling, the counts, the levels and the pictures.

/// Most columns of samples read from a frame.
pub const MAX_COLUMNS: usize = 384;
/// Height of the waveform and the parade: one row per level, 255 at the top.
pub const WAVE_HEIGHT: usize = 256;
/// Widest panel of the parade.
pub const PARADE_PANEL: usize = 128;
/// Gap between the parade's panels.
pub const PARADE_GAP: usize = 4;
/// Side of the vectorscope.
pub const VECTOR_SIZE: usize = 256;
/// The vectorscope's centre (both axes).
pub const VECTOR_CENTRE: i64 = 128;
/// Radius of a fully saturated colour.
pub const VECTOR_RADIUS: f64 = 120.0;
/// Size of the histogram.
pub const HISTOGRAM_WIDTH: usize = 256;
pub const HISTOGRAM_HEIGHT: usize = 160;

/// Rec. 709 Cb and Cr scale: Cb = (B − Y) / 1.8556, Cr = (R − Y) / 1.5748.
const CB_SCALE: f64 = 1.8556 * 255.0;
const CR_SCALE: f64 = 1.5748 * 255.0;
/// The skin tone line, 123° anticlockwise from the Cb axis.
const SKIN_COS: f64 = -0.5446390350150271;
const SKIN_SIN: f64 = 0.838670567945424;

const BACKGROUND: [u8; 3] = [12, 12, 12];
const GRATICULE: [u8; 3] = [60, 60, 60];
const CROSSHAIR: [u8; 3] = [36, 36, 36];
const TARGET: [u8; 3] = [150, 110, 60];
const SKIN: [u8; 3] = [120, 90, 70];
const LUMA_TRACE: [u8; 3] = [150, 255, 170];
const CHANNEL_TRACES: [[u8; 3]; 3] = [[255, 80, 80], [80, 255, 80], [90, 140, 255]];
const VECTOR_TRACE: [u8; 3] = [170, 255, 190];

/// Which scope to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Luma level of every column, left to right as in the frame.
    Waveform,
    /// The waveform of red, green and blue side by side.
    Parade,
    /// Hue and saturation: Cb across, Cr up.
    Vectorscope,
    /// How many pixels sit at each level of red, green and blue.
    Histogram,
}

impl Scope {
    pub const ALL: [Scope; 4] = [Scope::Waveform, Scope::Parade, Scope::Vectorscope, Scope::Histogram];

    /// The name the web and the API use.
    pub fn id(self) -> &'static str {
        match self {
            Scope::Waveform => "waveform",
            Scope::Parade => "parade",
            Scope::Vectorscope => "vectorscope",
            Scope::Histogram => "histogram",
        }
    }

    pub fn parse(id: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.id() == id)
    }
}

/// Channel order of the frame's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelOrder {
    Rgba,
    Bgra,
}

/// Rec. 709 luma of an 8-bit pixel, 0 to 255.
pub fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((54 * r as u32 + 183 * g as u32 + 19 * b as u32 + 128) >> 8) as u8
}

/// The sampling step for a frame this wide: every `step`th pixel in both
/// directions, so at most [`MAX_COLUMNS`] columns are read.
pub fn step_for(width: usize) -> usize {
    width.div_ceil(MAX_COLUMNS).max(1)
}

/// The frame's sampled pixels, row by row: red, green, blue and luma.
#[derive(Clone, Debug, PartialEq)]
pub struct Samples {
    pub columns: usize,
    pub rows: usize,
    pub step: usize,
    pub pixels: Vec<[u8; 4]>,
}

impl Samples {
    /// Samples a `width` × `height` frame (4 bytes a pixel). Pixels past the
    /// end of a short buffer are left out.
    pub fn take(pixels: &[u8], width: usize, height: usize, order: PixelOrder) -> Samples {
        let step = step_for(width);
        let (columns, rows) = (width.div_ceil(step), height.div_ceil(step));
        let mut out = Vec::with_capacity(columns * rows);
        let mut taken_rows = 0;
        for y in (0..height).step_by(step) {
            let mut row = 0;
            for x in (0..width).step_by(step) {
                let index = (y * width + x) * 4;
                let Some(pixel) = pixels.get(index..index + 4) else { break };
                let (r, g, b) = match order {
                    PixelOrder::Rgba => (pixel[0], pixel[1], pixel[2]),
                    PixelOrder::Bgra => (pixel[2], pixel[1], pixel[0]),
                };
                out.push([r, g, b, luma(r, g, b)]);
                row += 1;
            }
            if row < columns {
                // A short buffer: drop the partial row.
                out.truncate(out.len() - row);
                break;
            }
            taken_rows += 1;
        }
        Samples { columns, rows: taken_rows, step, pixels: out }
    }

    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }
}

/// What the scope measured, for a readout beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopeStats {
    /// Samples read.
    pub samples: usize,
    /// Share of samples (0 to 1) with a channel at 0.
    pub at_black: f64,
    /// Share of samples with a channel at 255.
    pub at_white: f64,
    /// Luma below which the darkest 1% of samples sit (0 to 255).
    pub black_level: u8,
    /// Luma above which the brightest 1% sit.
    pub white_level: u8,
}

/// A drawn scope: `width` × `height` RGBA pixels and what it measured.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeImage {
    pub scope: Scope,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub stats: ScopeStats,
}

/// Measures the samples.
pub fn stats(samples: &Samples) -> ScopeStats {
    let total = samples.len();
    let mut levels = [0usize; 256];
    let (mut black, mut white) = (0usize, 0usize);
    for &[r, g, b, y] in &samples.pixels {
        levels[y as usize] += 1;
        if r == 0 || g == 0 || b == 0 {
            black += 1;
        }
        if r == 255 || g == 255 || b == 255 {
            white += 1;
        }
    }
    let share = |count: usize| if total == 0 { 0.0 } else { count as f64 / total as f64 };
    // The level where the running count first reaches ceil(share × total).
    let level_at = |wanted: usize| -> u8 {
        let mut running = 0;
        for (level, count) in levels.iter().enumerate() {
            running += count;
            if running >= wanted.max(1) {
                return level as u8;
            }
        }
        255
    };
    let (black_level, white_level) =
        if total == 0 { (0, 255) } else { (level_at(total.div_ceil(100)), level_at((total * 99).div_ceil(100))) };
    ScopeStats { samples: total, at_black: share(black), at_white: share(white), black_level, white_level }
}

/// Counts per (column, level) of one channel (0 red, 1 green, 2 blue, 3
/// luma), `width` columns wide: sample column `x` lands in `x × width /
/// columns`. Indexed `column × 256 + level`.
pub fn wave_counts(samples: &Samples, channel: usize, width: usize) -> Vec<u32> {
    let mut counts = vec![0u32; width * 256];
    if samples.columns == 0 {
        return counts;
    }
    for (index, pixel) in samples.pixels.iter().enumerate() {
        let column = (index % samples.columns) * width / samples.columns;
        counts[column * 256 + pixel[channel] as usize] += 1;
    }
    counts
}

/// Where a colour lands on the vectorscope (x right, y down), clamped to it.
pub fn vector_position(r: u8, g: u8, b: u8) -> (usize, usize) {
    let y = luma(r, g, b) as f64;
    let cb = (b as f64 - y) / CB_SCALE;
    let cr = (r as f64 - y) / CR_SCALE;
    let last = VECTOR_SIZE as i64 - 1;
    let x = (VECTOR_CENTRE + (cb * 2.0 * VECTOR_RADIUS + 0.5).floor() as i64).clamp(0, last);
    let y = (VECTOR_CENTRE - (cr * 2.0 * VECTOR_RADIUS + 0.5).floor() as i64).clamp(0, last);
    (x as usize, y as usize)
}

/// Counts per vectorscope pixel, indexed `y × size + x`.
pub fn vector_counts(samples: &Samples) -> Vec<u32> {
    let mut counts = vec![0u32; VECTOR_SIZE * VECTOR_SIZE];
    for &[r, g, b, _] in &samples.pixels {
        let (x, y) = vector_position(r, g, b);
        counts[y * VECTOR_SIZE + x] += 1;
    }
    counts
}

/// Samples at each level of red, green, blue and luma.
pub fn histogram(samples: &Samples) -> [[u32; 256]; 4] {
    let mut bins = [[0u32; 256]; 4];
    for pixel in &samples.pixels {
        for channel in 0..4 {
            bins[channel][pixel[channel] as usize] += 1;
        }
    }
    bins
}

/// How bright a trace is where `count` of `total` samples land, with
/// `gain` the share that lights it fully: sqrt(count × gain / total), at
/// most 1.
pub fn trace_level(count: u32, total: u32, gain: f64) -> f64 {
    if count == 0 || total == 0 {
        return 0.0;
    }
    (count as f64 * gain / total as f64).sqrt().min(1.0)
}

struct Canvas {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Canvas {
        let mut rgba = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            rgba.extend_from_slice(&[BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 255]);
        }
        Canvas { width, height, rgba }
    }

    fn set(&mut self, x: usize, y: usize, colour: [u8; 3]) {
        if x < self.width && y < self.height {
            let index = (y * self.width + x) * 4;
            self.rgba[index..index + 3].copy_from_slice(&colour);
        }
    }

    /// Lights a pixel with a trace at `level` (0 to 1), keeping whatever is
    /// brighter in each channel.
    fn light(&mut self, x: usize, y: usize, colour: [u8; 3], level: f64) {
        if level <= 0.0 || x >= self.width || y >= self.height {
            return;
        }
        let index = (y * self.width + x) * 4;
        for channel in 0..3 {
            let value = (colour[channel] as f64 * level + 0.5).floor() as u8;
            self.rgba[index + channel] = self.rgba[index + channel].max(value);
        }
    }
}

/// Rows of the 0, 25, 50, 75 and 100% lines of a waveform.
const WAVE_LINES: [usize; 5] = [255, 191, 127, 64, 0];

fn wave_panel(canvas: &mut Canvas, samples: &Samples, channel: usize, left: usize, width: usize, colour: [u8; 3]) {
    for row in WAVE_LINES {
        for x in left..left + width {
            canvas.set(x, row, GRATICULE);
        }
    }
    let counts = wave_counts(samples, channel, width);
    for column in 0..width {
        let cells = &counts[column * 256..column * 256 + 256];
        let total: u32 = cells.iter().sum();
        for (level, &count) in cells.iter().enumerate() {
            canvas.light(left + column, 255 - level, colour, trace_level(count, total, 32.0));
        }
    }
}

fn draw_waveform(samples: &Samples) -> Canvas {
    let width = samples.columns.max(1);
    let mut canvas = Canvas::new(width, WAVE_HEIGHT);
    wave_panel(&mut canvas, samples, 3, 0, width, LUMA_TRACE);
    canvas
}

fn draw_parade(samples: &Samples) -> Canvas {
    let panel = samples.columns.clamp(1, PARADE_PANEL);
    let mut canvas = Canvas::new(panel * 3 + PARADE_GAP * 2, WAVE_HEIGHT);
    for channel in 0..3 {
        wave_panel(&mut canvas, samples, channel, channel * (panel + PARADE_GAP), panel, CHANNEL_TRACES[channel]);
    }
    canvas
}

fn draw_vectorscope(samples: &Samples) -> Canvas {
    let mut canvas = Canvas::new(VECTOR_SIZE, VECTOR_SIZE);
    let radius = VECTOR_RADIUS as i64;
    // Crosshair, then the outer ring.
    for offset in -radius..=radius {
        let along = (VECTOR_CENTRE + offset) as usize;
        canvas.set(along, VECTOR_CENTRE as usize, CROSSHAIR);
        canvas.set(VECTOR_CENTRE as usize, along, CROSSHAIR);
    }
    for y in 0..VECTOR_SIZE {
        for x in 0..VECTOR_SIZE {
            let (dx, dy) = (x as i64 - VECTOR_CENTRE, y as i64 - VECTOR_CENTRE);
            let distance = ((dx * dx + dy * dy) as f64).sqrt();
            if (distance - VECTOR_RADIUS).abs() < 0.5 {
                canvas.set(x, y, GRATICULE);
            }
        }
    }
    // The skin tone line.
    for t in 0..=radius {
        let x = VECTOR_CENTRE + (t as f64 * SKIN_COS + 0.5).floor() as i64;
        let y = VECTOR_CENTRE - (t as f64 * SKIN_SIN + 0.5).floor() as i64;
        canvas.set(x as usize, y as usize, SKIN);
    }
    // Boxes where 75% colour bars land: red, magenta, blue, cyan, green, yellow.
    for [r, g, b] in [[191, 0, 0], [191, 0, 191], [0, 0, 191], [0, 191, 191], [0, 191, 0], [191, 191, 0]] {
        let (cx, cy) = vector_position(r, g, b);
        for dy in -3i64..=3 {
            for dx in -3i64..=3 {
                if dx.abs() == 3 || dy.abs() == 3 {
                    canvas.set((cx as i64 + dx) as usize, (cy as i64 + dy) as usize, TARGET);
                }
            }
        }
    }
    let counts = vector_counts(samples);
    let total = samples.len() as u32;
    for (index, &count) in counts.iter().enumerate() {
        canvas.light(index % VECTOR_SIZE, index / VECTOR_SIZE, VECTOR_TRACE, trace_level(count, total, 2048.0));
    }
    canvas
}

/// Bar heights of a histogram: linear against the fullest bin of any
/// channel, leaving out levels 0 and 255 so clipped pixels do not flatten
/// the rest (they are drawn to the top instead).
pub fn histogram_heights(bins: &[[u32; 256]; 4]) -> [[usize; 256]; 3] {
    let inner = bins[..3].iter().flat_map(|channel| channel[1..255].iter()).copied().max().unwrap_or(0);
    let all = bins[..3].iter().flat_map(|channel| channel.iter()).copied().max().unwrap_or(0);
    let tallest = if inner > 0 { inner } else { all }.max(1) as u64;
    let mut heights = [[0usize; 256]; 3];
    for channel in 0..3 {
        for level in 0..256 {
            let count = bins[channel][level] as u64;
            heights[channel][level] = ((count * HISTOGRAM_HEIGHT as u64) / tallest).min(HISTOGRAM_HEIGHT as u64) as usize;
        }
    }
    heights
}

fn draw_histogram(samples: &Samples) -> Canvas {
    let mut canvas = Canvas::new(HISTOGRAM_WIDTH, HISTOGRAM_HEIGHT);
    for x in [64, 128, 191] {
        for y in 0..HISTOGRAM_HEIGHT {
            canvas.set(x, y, GRATICULE);
        }
    }
    let heights = histogram_heights(&histogram(samples));
    for level in 0..256 {
        for row in 0..HISTOGRAM_HEIGHT {
            let lit: [bool; 3] = std::array::from_fn(|channel| row >= HISTOGRAM_HEIGHT - heights[channel][level]);
            if lit.iter().any(|&on| on) {
                canvas.set(level, row, lit.map(|on| if on { 220 } else { 40 }));
            }
        }
    }
    canvas
}

/// Draws a scope of a `width` × `height` frame.
pub fn draw(scope: Scope, pixels: &[u8], width: usize, height: usize, order: PixelOrder) -> ScopeImage {
    let samples = Samples::take(pixels, width, height, order);
    draw_samples(scope, &samples)
}

/// Draws a scope of samples already taken.
pub fn draw_samples(scope: Scope, samples: &Samples) -> ScopeImage {
    let canvas = match scope {
        Scope::Waveform => draw_waveform(samples),
        Scope::Parade => draw_parade(samples),
        Scope::Vectorscope => draw_vectorscope(samples),
        Scope::Histogram => draw_histogram(samples),
    };
    ScopeImage { scope, width: canvas.width, height: canvas.height, rgba: canvas.rgba, stats: stats(samples) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/scopes/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    /// The fixture's frame, made the same way as the reference does.
    fn frame(case: &Value) -> (Vec<u8>, usize, usize) {
        let (width, height) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
        let rgba: Vec<u8> = case["rgba"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
        assert_eq!(rgba.len(), width * height * 4);
        (rgba, width, height)
    }

    fn fnv(bytes: &[u8]) -> String {
        let mut hash: u64 = 0xcbf29ce484222325;
        for &byte in bytes {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }

    fn sparse(counts: &[u32], stride: usize) -> Vec<[u64; 3]> {
        counts
            .iter()
            .enumerate()
            .filter(|&(_, &count)| count > 0)
            .map(|(index, &count)| [(index / stride) as u64, (index % stride) as u64, count as u64])
            .collect()
    }

    fn triples(value: &Value) -> Vec<[u64; 3]> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let t = t.as_array().unwrap();
                [t[0].as_u64().unwrap(), t[1].as_u64().unwrap(), t[2].as_u64().unwrap()]
            })
            .collect()
    }

    #[test]
    fn luma_matches_rec_709() {
        assert_eq!(luma(0, 0, 0), 0);
        assert_eq!(luma(255, 255, 255), 255);
        assert_eq!(luma(255, 0, 0), 54);
        assert_eq!(luma(0, 255, 0), 182);
        assert_eq!(luma(0, 0, 255), 19);
    }

    #[test]
    fn sampling_and_counts_match_the_reference() {
        for case in cases()["frames"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let (rgba, width, height) = frame(case);
            let samples = Samples::take(&rgba, width, height, PixelOrder::Rgba);
            assert_eq!(samples.step as u64, case["step"].as_u64().unwrap(), "{name}");
            assert_eq!(samples.columns as u64, case["columns"].as_u64().unwrap(), "{name}");
            assert_eq!(samples.rows as u64, case["rows"].as_u64().unwrap(), "{name}");
            let bgra: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            assert_eq!(Samples::take(&bgra, width, height, PixelOrder::Bgra), samples, "{name}");

            let columns = samples.columns;
            assert_eq!(sparse(&wave_counts(&samples, 3, columns), 256), triples(&case["waveform"]), "{name} waveform");
            assert_eq!(sparse(&vector_counts(&samples), VECTOR_SIZE), triples(&case["vectorscope"]), "{name} vectorscope");
            let bins = histogram(&samples);
            for (channel, key) in ["r", "g", "b", "y"].into_iter().enumerate() {
                assert_eq!(sparse(&bins[channel], 256), triples(&case["histogram"][key]), "{name} histogram {key}");
            }

            let stats = stats(&samples);
            let want = &case["stats"];
            assert_eq!(stats.samples as u64, want["samples"].as_u64().unwrap(), "{name}");
            assert!((stats.at_black - want["atBlack"].as_f64().unwrap()).abs() < 1e-12, "{name}");
            assert!((stats.at_white - want["atWhite"].as_f64().unwrap()).abs() < 1e-12, "{name}");
            assert_eq!(stats.black_level as u64, want["blackLevel"].as_u64().unwrap(), "{name}");
            assert_eq!(stats.white_level as u64, want["whiteLevel"].as_u64().unwrap(), "{name}");

            for scope in Scope::ALL {
                let image = draw(scope, &rgba, width, height, PixelOrder::Rgba);
                let want = &case["images"][scope.id()];
                assert_eq!(image.width as u64, want["width"].as_u64().unwrap(), "{name} {}", scope.id());
                assert_eq!(image.height as u64, want["height"].as_u64().unwrap(), "{name} {}", scope.id());
                assert_eq!(fnv(&image.rgba), want["fnv"].as_str().unwrap(), "{name} {} picture", scope.id());
            }
        }
    }

    #[test]
    fn colour_bars_land_on_their_targets() {
        // A 75% red sits on the red box; grey sits in the centre.
        assert_eq!(vector_position(128, 128, 128), (128, 128));
        let (x, y) = vector_position(191, 0, 0);
        assert!(x < 128 && y < 128, "red is up and left of centre: {x}, {y}");
        let (x, y) = vector_position(0, 0, 255);
        assert!(x > 240 && (y as i64 - 128).abs() < 30, "full blue reaches the ring on the right: {x}, {y}");
    }

    #[test]
    fn short_buffers_and_empty_frames_do_not_panic() {
        let image = draw(Scope::Waveform, &[0; 10], 4, 4, PixelOrder::Rgba);
        assert_eq!(image.stats.samples, 0);
        let image = draw(Scope::Histogram, &[], 0, 0, PixelOrder::Rgba);
        assert_eq!((image.width, image.height), (HISTOGRAM_WIDTH, HISTOGRAM_HEIGHT));
        assert_eq!(Scope::parse("parade"), Some(Scope::Parade));
        assert_eq!(Scope::parse("nope"), None);
    }
}
