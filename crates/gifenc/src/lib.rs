//! Animated GIF export (row 96), the same bytes in both editors (the web
//! through WASM). Own code, no encoder library:
//!
//! - **Palette**: one global palette for the whole GIF, from a histogram of
//!   5-bit colour bins over sampled frames, by median cut (the box with the
//!   widest channel range splits at its weighted median along that channel).
//! - **Mapping**: nearest palette colour through a 6-bit lookup; optional
//!   ordered (4×4 Bayer) dithering, which stays put from frame to frame so
//!   still areas do not shimmer.
//! - **Frames**: the first frame whole; each later one only the rectangle
//!   that changed, with unchanged pixels transparent (disposal "leave in
//!   place"). A frame with no change adds its delay to the one before.
//! - **LZW**: as Unix `compress` (codes grow after the next free code passes
//!   the width's maximum; a clear code when the 4096-entry table is full).
//!
//! The GIF loops forever. Checked against `fixtures/gif/cases.json`, made
//! by an independent Python reference whose output Pillow decodes back to
//! the expected colours.

/// Most colours in a palette (one table entry stays for transparency).
pub const MAX_COLOURS: usize = 255;

const BAYER: [[usize; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
/// `(2m - 15) * 8 / 15` rounded toward zero, for m = 0..15.
const OFFSETS: [i32; 16] = [-8, -6, -5, -4, -3, -2, -1, 0, 0, 1, 2, 3, 4, 5, 6, 8];

/// A colour histogram over 5-bit bins, filled from sampled frames.
#[derive(Clone)]
pub struct Histogram {
    count: Vec<u64>,
    sums: Vec<[u64; 3]>,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    pub fn new() -> Self {
        Self { count: vec![0; 32768], sums: vec![[0; 3]; 32768] }
    }

    /// Counts every pixel of an RGBA frame (alpha is ignored).
    pub fn add_rgba(&mut self, rgba: &[u8]) {
        for pixel in rgba.chunks_exact(4) {
            let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
            let bin = bin5(r, g, b);
            self.count[bin] += 1;
            let sums = &mut self.sums[bin];
            sums[0] += u64::from(r);
            sums[1] += u64::from(g);
            sums[2] += u64::from(b);
        }
    }

    /// Up to `wanted` colours (at most [`MAX_COLOURS`]) by median cut; one
    /// black colour when nothing was counted.
    pub fn palette(&self, wanted: usize) -> Vec<[u8; 3]> {
        let wanted = wanted.clamp(1, MAX_COLOURS);
        let bins: Vec<usize> = (0..32768).filter(|&bin| self.count[bin] > 0).collect();
        if bins.is_empty() {
            return vec![[0, 0, 0]];
        }
        let mut boxes = vec![bins];
        while boxes.len() < wanted {
            // The widest box (ties: more pixels, then the earlier box).
            let mut best: Option<((u32, u64), usize)> = None;
            for (index, cube) in boxes.iter().enumerate() {
                if cube.len() < 2 {
                    continue;
                }
                let key = (*ranges(cube).iter().max().unwrap(), self.total(cube));
                if best.is_none_or(|(best_key, _)| key > best_key) {
                    best = Some((key, index));
                }
            }
            let Some((_, index)) = best else {
                break;
            };
            let widths = ranges(&boxes[index]);
            let max = *widths.iter().max().unwrap();
            let channel = widths.iter().position(|&width| width == max).unwrap();
            let mut sorted = boxes[index].clone();
            sorted.sort_by_key(|&bin| (channel_of(bin, channel), bin));
            let total = self.total(&sorted);
            let mut cumulative = 0;
            let mut cut = sorted.len() - 1;
            for (position, &bin) in sorted[..sorted.len() - 1].iter().enumerate() {
                cumulative += self.count[bin];
                if cumulative * 2 >= total {
                    cut = position + 1;
                    break;
                }
            }
            let right = sorted.split_off(cut);
            boxes[index] = sorted;
            boxes.push(right);
        }
        boxes
            .iter()
            .map(|cube| {
                let count = self.total(cube);
                let mut colour = [0u8; 3];
                for (channel, value) in colour.iter_mut().enumerate() {
                    let sum: u64 = cube.iter().map(|&bin| self.sums[bin][channel]).sum();
                    *value = ((sum + count / 2) / count) as u8;
                }
                colour
            })
            .collect()
    }

    fn total(&self, cube: &[usize]) -> u64 {
        cube.iter().map(|&bin| self.count[bin]).sum()
    }
}

fn bin5(r: u8, g: u8, b: u8) -> usize {
    (usize::from(r >> 3) << 10) | (usize::from(g >> 3) << 5) | usize::from(b >> 3)
}

fn channel_of(bin: usize, channel: usize) -> u32 {
    ((bin >> (10 - 5 * channel)) & 31) as u32
}

fn ranges(cube: &[usize]) -> [u32; 3] {
    let mut out = [0; 3];
    for (channel, width) in out.iter_mut().enumerate() {
        let values = cube.iter().map(|&bin| channel_of(bin, channel));
        let (low, high) = values.fold((u32::MAX, 0), |(low, high), value| (low.min(value), high.max(value)));
        *width = high - low;
    }
    out
}

/// A GIF's frame size for a canvas: `width` (16 px at least, no wider than
/// the canvas), the height in proportion; both even (as the desktop
/// renderer draws frames).
pub fn gif_size(canvas_width: u32, canvas_height: u32, width: u32) -> (u32, u32) {
    let width = width.clamp(16, canvas_width.max(16)) & !1;
    let height = ((f64::from(canvas_height) * f64::from(width) / f64::from(canvas_width.max(1))).round() as u32).max(2) & !1;
    (width, height)
}

/// Frame delays in centiseconds for `count` frames at `fps`, rounded so
/// they add up to the right length.
pub fn frame_delays(count: usize, fps: f64) -> Vec<u16> {
    let at = |index: usize| (index as f64 * 100.0 / fps + 0.5).floor() as i64;
    (0..count).map(|index| (at(index + 1) - at(index)).clamp(0, i64::from(u16::MAX)) as u16).collect()
}

struct Pending {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    indices: Vec<u8>,
    transparent: bool,
    delay: u32,
}

/// Writes a looping GIF frame by frame.
pub struct GifWriter {
    width: usize,
    height: usize,
    palette: Vec<[u8; 3]>,
    dither: bool,
    bits: u32,
    transparent: u8,
    lookup: Vec<u16>,
    previous: Option<Vec<u8>>,
    pending: Option<Pending>,
    out: Vec<u8>,
}

impl GifWriter {
    /// A GIF of `width` × `height` with `palette` (from [`Histogram::palette`]).
    pub fn new(width: u32, height: u32, palette: Vec<[u8; 3]>, dither: bool) -> Self {
        let width = width.clamp(1, 65535) as usize;
        let height = height.clamp(1, 65535) as usize;
        let mut palette = palette;
        palette.truncate(MAX_COLOURS);
        if palette.is_empty() {
            palette.push([0, 0, 0]);
        }
        let mut bits = 1;
        while (1usize << bits) < palette.len() + 1 {
            bits += 1;
        }
        let mut out = b"GIF89a".to_vec();
        out.extend((width as u16).to_le_bytes());
        out.extend((height as u16).to_le_bytes());
        out.extend([0x80 | 0x70 | (bits as u8 - 1), 0, 0]);
        for index in 0..(1usize << bits) {
            out.extend(palette.get(index).copied().unwrap_or([0, 0, 0]));
        }
        // Loop forever (the NETSCAPE2.0 application extension).
        out.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
        Self {
            width,
            height,
            transparent: palette.len() as u8,
            palette,
            dither,
            bits,
            lookup: vec![u16::MAX; 1 << 18],
            previous: None,
            pending: None,
            out,
        }
    }

    fn nearest(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let key = (usize::from(r >> 2) << 12) | (usize::from(g >> 2) << 6) | usize::from(b >> 2);
        let cached = self.lookup[key];
        if cached != u16::MAX {
            return cached as u8;
        }
        let centre = |value: u8| i32::from((value >> 2) << 2 | 2);
        let (cr, cg, cb) = (centre(r), centre(g), centre(b));
        let mut best = (i32::MAX, 0usize);
        for (index, colour) in self.palette.iter().enumerate() {
            let (dr, dg, db) = (cr - i32::from(colour[0]), cg - i32::from(colour[1]), cb - i32::from(colour[2]));
            let distance = dr * dr + dg * dg + db * db;
            if distance < best.0 {
                best = (distance, index);
            }
        }
        self.lookup[key] = best.1 as u16;
        best.1 as u8
    }

    /// The palette index of each pixel of an RGBA frame.
    fn map(&mut self, rgba: &[u8]) -> Vec<u8> {
        let mut indices = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let at = (y * self.width + x) * 4;
                let (mut r, mut g, mut b) = match rgba.get(at..at + 3) {
                    Some(pixel) => (pixel[0], pixel[1], pixel[2]),
                    None => (0, 0, 0),
                };
                if self.dither {
                    let offset = OFFSETS[BAYER[y & 3][x & 3]];
                    let shift = |value: u8| (i32::from(value) + offset).clamp(0, 255) as u8;
                    (r, g, b) = (shift(r), shift(g), shift(b));
                }
                indices.push(self.nearest(r, g, b));
            }
        }
        indices
    }

    /// Adds an RGBA frame (`width * height * 4` bytes) shown for `delay`
    /// centiseconds.
    pub fn add_rgba(&mut self, rgba: &[u8], delay: u16) {
        let indices = self.map(rgba);
        let frame = match &self.previous {
            None => Pending {
                x: 0,
                y: 0,
                width: self.width as u16,
                height: self.height as u16,
                indices: indices.clone(),
                transparent: false,
                delay: u32::from(delay),
            },
            Some(previous) => {
                let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
                for (at, (now, before)) in indices.iter().zip(previous).enumerate() {
                    if now != before {
                        let (x, y) = (at % self.width, at / self.width);
                        x0 = x0.min(x);
                        x1 = x1.max(x);
                        y0 = y0.min(y);
                        y1 = y1.max(y);
                    }
                }
                if x0 == usize::MAX {
                    if let Some(pending) = &mut self.pending {
                        pending.delay += u32::from(delay);
                    }
                    return;
                }
                let mut sub = Vec::with_capacity((x1 - x0 + 1) * (y1 - y0 + 1));
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let at = y * self.width + x;
                        sub.push(if indices[at] == previous[at] { self.transparent } else { indices[at] });
                    }
                }
                Pending {
                    x: x0 as u16,
                    y: y0 as u16,
                    width: (x1 - x0 + 1) as u16,
                    height: (y1 - y0 + 1) as u16,
                    indices: sub,
                    transparent: true,
                    delay: u32::from(delay),
                }
            }
        };
        self.flush();
        self.pending = Some(frame);
        self.previous = Some(indices);
    }

    fn flush(&mut self) {
        let Some(frame) = self.pending.take() else {
            return;
        };
        let out = &mut self.out;
        out.extend([0x21, 0xF9, 4, (1 << 2) | u8::from(frame.transparent)]);
        out.extend((frame.delay.min(65535) as u16).to_le_bytes());
        out.extend([if frame.transparent { self.transparent } else { 0 }, 0]);
        out.push(0x2C);
        for value in [frame.x, frame.y, frame.width, frame.height] {
            out.extend(value.to_le_bytes());
        }
        out.push(0);
        let min_size = self.bits.max(2);
        out.push(min_size as u8);
        lzw(&frame.indices, min_size, out);
    }

    /// The finished GIF.
    pub fn finish(mut self) -> Vec<u8> {
        self.flush();
        self.out.push(0x3B);
        self.out
    }

    /// Bytes written so far (frames are written one behind).
    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }
}

/// LZW-compresses `indices` into GIF data sub-blocks appended to `out`.
fn lzw(indices: &[u8], min_size: u32, out: &mut Vec<u8>) {
    struct Bits {
        bytes: Vec<u8>,
        acc: u64,
        count: u32,
        width: u32,
        next: u32,
        min_size: u32,
        clear: u32,
    }
    impl Bits {
        fn max_code(&self) -> u32 {
            if self.width == 12 { 4096 } else { (1 << self.width) - 1 }
        }
        fn write(&mut self, code: u32) {
            self.acc |= u64::from(code) << self.count;
            self.count += self.width;
            while self.count >= 8 {
                self.bytes.push(self.acc as u8);
                self.acc >>= 8;
                self.count -= 8;
            }
            if code == self.clear {
                self.width = self.min_size + 1;
            } else if self.next > self.max_code() {
                self.width += 1;
            }
        }
    }
    let clear = 1u32 << min_size;
    let mut bits = Bits { bytes: Vec::new(), acc: 0, count: 0, width: min_size + 1, next: clear + 2, min_size, clear };
    // (prefix code, next index) -> code.
    let mut table: std::collections::HashMap<(u32, u8), u32> = std::collections::HashMap::new();
    bits.write(clear);
    if let Some((&first, rest)) = indices.split_first() {
        let mut prefix = u32::from(first);
        for &index in rest {
            if let Some(&code) = table.get(&(prefix, index)) {
                prefix = code;
                continue;
            }
            bits.write(prefix);
            if bits.next < 4096 {
                table.insert((prefix, index), bits.next);
                bits.next += 1;
            } else {
                bits.write(clear);
                table.clear();
                bits.next = clear + 2;
            }
            prefix = u32::from(index);
        }
        bits.write(prefix);
    }
    bits.write(clear + 1);
    if bits.count > 0 {
        bits.bytes.push(bits.acc as u8);
    }
    for block in bits.bytes.chunks(255) {
        out.push(block.len() as u8);
        out.extend_from_slice(block);
    }
    out.push(0);
}

/// Encodes whole frames in one call: the palette from every `sample_every`th
/// frame, then each frame with its delay.
pub fn encode(width: u32, height: u32, frames: &[Vec<u8>], delays: &[u16], colours: usize, dither: bool, sample_every: usize) -> Vec<u8> {
    let mut histogram = Histogram::new();
    for frame in frames.iter().step_by(sample_every.max(1)) {
        histogram.add_rgba(frame);
    }
    let mut writer = GifWriter::new(width, height, histogram.palette(colours), dither);
    for (frame, delay) in frames.iter().zip(delays) {
        writer.add_rgba(frame, *delay);
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn base64(text: &str) -> Vec<u8> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let (mut acc, mut count) = (0u32, 0);
        for byte in text.bytes() {
            let Some(value) = alphabet.iter().position(|&c| c == byte) else {
                continue;
            };
            acc = acc << 6 | value as u32;
            count += 6;
            if count >= 8 {
                count -= 8;
                out.push((acc >> count) as u8);
                acc &= (1 << count) - 1;
            }
        }
        out
    }

    #[test]
    fn encodes_like_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/gif/cases.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let width = case["width"].as_u64().unwrap() as u32;
            let height = case["height"].as_u64().unwrap() as u32;
            let frames: Vec<Vec<u8>> = case["frames"].as_array().unwrap().iter().map(|frame| base64(frame.as_str().unwrap())).collect();
            let delays = frame_delays(frames.len(), case["fps"].as_f64().unwrap());
            let want: Vec<u16> = case["delays"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap() as u16).collect();
            assert_eq!(delays, want, "{name}: delays");
            let every = case["sampleEvery"].as_u64().unwrap() as usize;
            let mut histogram = Histogram::new();
            for frame in frames.iter().step_by(every) {
                histogram.add_rgba(frame);
            }
            let palette = histogram.palette(case["colours"].as_u64().unwrap() as usize);
            let want_palette: Vec<[u8; 3]> = case["palette"]
                .as_array()
                .unwrap()
                .iter()
                .map(|colour| {
                    let c = colour.as_array().unwrap();
                    [c[0].as_u64().unwrap() as u8, c[1].as_u64().unwrap() as u8, c[2].as_u64().unwrap() as u8]
                })
                .collect();
            assert_eq!(palette, want_palette, "{name}: palette");
            let gif = encode(width, height, &frames, &delays, case["colours"].as_u64().unwrap() as usize, case["dither"].as_bool().unwrap(), every);
            let want_gif = base64(case["gif"].as_str().unwrap());
            assert_eq!(gif.len(), want_gif.len(), "{name}: length");
            assert!(gif == want_gif, "{name}: bytes differ");
        }
    }

    #[test]
    fn delays_add_up() {
        let delays = frame_delays(30, 30.0);
        assert_eq!(delays.iter().map(|&d| u32::from(d)).sum::<u32>(), 100);
        assert!(delays.iter().all(|&d| d == 3 || d == 4));
        assert_eq!(Histogram::new().palette(10), vec![[0, 0, 0]]);
    }
}
