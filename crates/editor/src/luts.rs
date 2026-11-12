//! Colour look-up tables: `.cube` and `.3dl` files and HALD CLUT images
//! parsed into a 3D table, resampled to at most 33 points a side, and
//! packed into the `lut` effect parameter as `name|size|base64(RGB8)`
//! (red varies fastest, then green, then blue). The web app's
//! `src/luts/parse.ts` reads and writes the same format.

/// A 3D table: `rgb` holds `size³` triplets in 0..1, red varying fastest.
#[derive(Clone, Debug, PartialEq)]
pub struct LutTable {
    pub size: usize,
    pub rgb: Vec<f32>,
}

/// The largest table kept; bigger files are resampled down.
pub const MAX_LUT_SIZE: usize = 33;

impl LutTable {
    pub fn identity(size: usize) -> Self {
        let mut rgb = Vec::with_capacity(size * size * size * 3);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let n = (size - 1).max(1) as f32;
                    rgb.extend_from_slice(&[r as f32 / n, g as f32 / n, b as f32 / n]);
                }
            }
        }
        Self { size, rgb }
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        let index = ((b * self.size + g) * self.size + r) * 3;
        [self.rgb[index], self.rgb[index + 1], self.rgb[index + 2]]
    }

    /// Trilinear look-up at `(r, g, b)` in 0..1.
    pub fn sample(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        let n = (self.size - 1) as f32;
        let (fr, fg, fb) = (r.clamp(0.0, 1.0) * n, g.clamp(0.0, 1.0) * n, b.clamp(0.0, 1.0) * n);
        let (r0, g0, b0) = (fr.floor() as usize, fg.floor() as usize, fb.floor() as usize);
        let (r1, g1, b1) = ((r0 + 1).min(self.size - 1), (g0 + 1).min(self.size - 1), (b0 + 1).min(self.size - 1));
        let (tr, tg, tb) = (fr - r0 as f32, fg - g0 as f32, fb - b0 as f32);
        let mut out = [0.0f32; 3];
        for channel in 0..3 {
            let c00 = self.at(r0, g0, b0)[channel] * (1.0 - tr) + self.at(r1, g0, b0)[channel] * tr;
            let c10 = self.at(r0, g1, b0)[channel] * (1.0 - tr) + self.at(r1, g1, b0)[channel] * tr;
            let c01 = self.at(r0, g0, b1)[channel] * (1.0 - tr) + self.at(r1, g0, b1)[channel] * tr;
            let c11 = self.at(r0, g1, b1)[channel] * (1.0 - tr) + self.at(r1, g1, b1)[channel] * tr;
            let c0 = c00 * (1.0 - tg) + c10 * tg;
            let c1 = c01 * (1.0 - tg) + c11 * tg;
            out[channel] = c0 * (1.0 - tb) + c1 * tb;
        }
        out
    }

    /// The table resampled to `size` points a side (trilinear).
    pub fn resampled(&self, size: usize) -> Self {
        if size == self.size {
            return self.clone();
        }
        let mut rgb = Vec::with_capacity(size * size * size * 3);
        let n = (size - 1).max(1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    rgb.extend_from_slice(&self.sample(r as f32 / n, g as f32 / n, b as f32 / n));
                }
            }
        }
        Self { size, rgb }
    }

    /// RGBA8 bytes laid out as the shader's 2D texture: width `size²`
    /// (x = r + size·b), height `size` (y = g).
    pub fn to_texture_rgba(&self) -> Vec<u8> {
        let size = self.size;
        let mut out = vec![255u8; size * size * size * 4];
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let [cr, cg, cb] = self.at(r, g, b);
                    let x = r + size * b;
                    let index = (g * size * size + x) * 4;
                    out[index] = (cr.clamp(0.0, 1.0) * 255.0).round() as u8;
                    out[index + 1] = (cg.clamp(0.0, 1.0) * 255.0).round() as u8;
                    out[index + 2] = (cb.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
        out
    }
}

/// Parses an Adobe/Resolve `.cube` file (3D, or 1D expanded to 3D).
pub fn parse_cube(text: &str) -> Result<LutTable, String> {
    let mut size_3d: Option<usize> = None;
    let mut size_1d: Option<usize> = None;
    let mut domain_min = [0.0f32; 3];
    let mut domain_max = [1.0f32; 3];
    let mut values: Vec<[f32; 3]> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(head) = parts.next() else { continue };
        match head.to_ascii_uppercase().as_str() {
            "TITLE" => {}
            "LUT_3D_SIZE" => size_3d = parts.next().and_then(|v| v.parse().ok()),
            "LUT_1D_SIZE" => size_1d = parts.next().and_then(|v| v.parse().ok()),
            "DOMAIN_MIN" | "DOMAIN_MAX" => {
                let mut triple = [0.0f32; 3];
                for (index, slot) in triple.iter_mut().enumerate() {
                    *slot = parts.nth(0).and_then(|v| v.parse().ok()).ok_or_else(|| format!("bad domain line: {line}"))?;
                    let _ = index;
                }
                if head.eq_ignore_ascii_case("DOMAIN_MIN") { domain_min = triple } else { domain_max = triple }
            }
            _ => {
                let r: f32 = head.parse().map_err(|_| format!("unexpected line: {line}"))?;
                let g: f32 = parts.next().and_then(|v| v.parse().ok()).ok_or_else(|| format!("short row: {line}"))?;
                let b: f32 = parts.next().and_then(|v| v.parse().ok()).ok_or_else(|| format!("short row: {line}"))?;
                values.push([r, g, b]);
            }
        }
    }
    let normalise = |value: [f32; 3]| -> [f32; 3] {
        let mut out = [0.0f32; 3];
        for channel in 0..3 {
            let span = (domain_max[channel] - domain_min[channel]).abs().max(1e-6);
            out[channel] = (value[channel] - domain_min[channel]) / span;
        }
        out
    };
    if let Some(size) = size_3d {
        if size < 2 || values.len() != size * size * size {
            return Err(format!("expected {} rows for LUT_3D_SIZE {size}, found {}", size * size * size, values.len()));
        }
        let rgb = values.into_iter().flat_map(normalise).collect();
        return Ok(LutTable { size, rgb });
    }
    if let Some(size) = size_1d {
        if size < 2 || values.len() != size {
            return Err(format!("expected {size} rows for LUT_1D_SIZE {size}, found {}", values.len()));
        }
        let curve: Vec<[f32; 3]> = values.into_iter().map(normalise).collect();
        let n = (size - 1) as f32;
        let lookup = |x: f32, channel: usize| -> f32 {
            let f = x.clamp(0.0, 1.0) * n;
            let i = f.floor() as usize;
            let j = (i + 1).min(size - 1);
            curve[i][channel] * (1.0 - (f - i as f32)) + curve[j][channel] * (f - i as f32)
        };
        let out_size = size.min(MAX_LUT_SIZE);
        let m = (out_size - 1) as f32;
        let mut rgb = Vec::with_capacity(out_size * out_size * out_size * 3);
        for b in 0..out_size {
            for g in 0..out_size {
                for r in 0..out_size {
                    rgb.extend_from_slice(&[lookup(r as f32 / m, 0), lookup(g as f32 / m, 1), lookup(b as f32 / m, 2)]);
                }
            }
        }
        return Ok(LutTable { size: out_size, rgb });
    }
    Err("no LUT_3D_SIZE or LUT_1D_SIZE line".to_owned())
}

/// Parses an Autodesk `.3dl` file: a grid line of input levels, then one
/// output triplet per line with red varying slowest and blue fastest, in
/// integers whose range the largest value tells (1023 or 4095).
pub fn parse_3dl(text: &str) -> Result<LutTable, String> {
    let mut grid: Option<Vec<f32>> = None;
    let mut rows: Vec<[f32; 3]> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let numbers: Vec<f32> = line.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        if numbers.is_empty() {
            continue;
        }
        if grid.is_none() && numbers.len() > 3 {
            grid = Some(numbers);
            continue;
        }
        if numbers.len() >= 3 {
            rows.push([numbers[0], numbers[1], numbers[2]]);
        }
    }
    let size = match &grid {
        Some(grid) => grid.len(),
        None => (rows.len() as f64).cbrt().round() as usize,
    };
    if size < 2 || rows.len() != size * size * size {
        return Err(format!("expected {} rows for a {size}-point 3dl, found {}", size * size * size, rows.len()));
    }
    let peak = rows.iter().flat_map(|row| row.iter().copied()).fold(0.0f32, f32::max);
    let scale = if peak > 4095.5 { 65535.0 } else if peak > 1023.5 { 4095.0 } else if peak > 255.5 { 1023.0 } else if peak > 1.0 { 255.0 } else { 1.0 };
    let mut rgb = vec![0.0f32; size * size * size * 3];
    for (index, row) in rows.iter().enumerate() {
        // Red slowest, blue fastest in the file; the table wants red fastest.
        let r = index / (size * size);
        let g = (index / size) % size;
        let b = index % size;
        let at = ((b * size + g) * size + r) * 3;
        rgb[at] = row[0] / scale;
        rgb[at + 1] = row[1] / scale;
        rgb[at + 2] = row[2] / scale;
    }
    Ok(LutTable { size, rgb })
}

/// A HALD CLUT image (a level-`L` square of `L³` pixels a side, the cube
/// flattened red-fastest) as a table of `L²` points a side.
pub fn from_hald(rgba: &[u8], width: u32, height: u32) -> Result<LutTable, String> {
    if width != height || rgba.len() != (width * height * 4) as usize {
        return Err("a HALD CLUT image is square".to_owned());
    }
    let level = (width as f64).cbrt().round() as u32;
    if level < 2 || level * level * level != width {
        return Err(format!("{width}×{height} is not a HALD CLUT size (a level cubed)"));
    }
    let size = (level * level) as usize;
    let rgb = (0..size * size * size)
        .flat_map(|index| {
            let at = index * 4;
            [rgba[at] as f32 / 255.0, rgba[at + 1] as f32 / 255.0, rgba[at + 2] as f32 / 255.0]
        })
        .collect();
    Ok(LutTable { size, rgb })
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(BASE64[(n >> 18) as usize & 63] as char);
        out.push(BASE64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let value = |c: u8| -> Result<u32, String> {
        BASE64.iter().position(|b| *b == c).map(|v| v as u32).ok_or_else(|| format!("bad base64 character {:?}", c as char))
    };
    let clean: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        if chunk.len() < 4 {
            return Err("truncated base64".to_owned());
        }
        let pad = chunk.iter().filter(|b| **b == b'=').count();
        let n = (value(chunk[0])? << 18)
            | (value(chunk[1])? << 12)
            | (if chunk[2] == b'=' { 0 } else { value(chunk[2])? } << 6)
            | if chunk[3] == b'=' { 0 } else { value(chunk[3])? };
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

/// The `lut` effect parameter: `name|size|base64(RGB8)`, at most 33 a side.
pub fn encode_lut_param(name: &str, table: &LutTable) -> String {
    let table = if table.size > MAX_LUT_SIZE { table.resampled(MAX_LUT_SIZE) } else { table.clone() };
    let bytes: Vec<u8> = table.rgb.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    format!("{}|{}|{}", name.replace('|', "/"), table.size, base64_encode(&bytes))
}

/// A decoded `lut` parameter: its name and table.
pub fn decode_lut_param(param: &str) -> Result<(String, LutTable), String> {
    let mut parts = param.splitn(3, '|');
    let name = parts.next().unwrap_or("").to_owned();
    let size: usize = parts.next().and_then(|v| v.parse().ok()).ok_or_else(|| "lut parameter has no size".to_owned())?;
    let bytes = base64_decode(parts.next().ok_or_else(|| "lut parameter has no data".to_owned())?)?;
    if size < 2 || bytes.len() != size * size * size * 3 {
        return Err(format!("lut parameter holds {} bytes, not {} for size {size}", bytes.len(), size * size * size * 3));
    }
    Ok((name, LutTable { size, rgb: bytes.iter().map(|b| *b as f32 / 255.0).collect() }))
}

/// A short stable key for a `lut` parameter (FNV-1a of its text), used to
/// cache the uploaded texture.
pub fn lut_key(param: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in param.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("lut-{hash:016x}-{}", param.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_files_parse_with_domain_and_one_dimensional_curves() {
        let text = "TITLE \"swap\"\n# comment\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n";
        let table = parse_cube(text).unwrap();
        assert_eq!(table.size, 2);
        // Row 1 is red=1: it maps to blue, so red and blue are swapped.
        assert_eq!(table.sample(1.0, 0.0, 0.0), [0.0, 0.0, 1.0]);
        assert_eq!(table.sample(0.0, 0.0, 1.0), [1.0, 0.0, 0.0]);
        let curve = parse_cube("LUT_1D_SIZE 3\n0 0 0\n0.25 0.5 0.75\n1 1 1\n").unwrap();
        assert_eq!(curve.size, 3);
        assert_eq!(curve.sample(0.5, 0.5, 0.5), [0.25, 0.5, 0.75]);
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
    }

    #[test]
    fn three_dl_and_hald_parse_into_the_same_layout() {
        let mut text = String::from("0 1023\n");
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    text.push_str(&format!("{} {} {}\n", b * 1023, g * 1023, r * 1023));
                }
            }
        }
        let table = parse_3dl(&text).unwrap();
        assert_eq!(table.size, 2);
        assert_eq!(table.sample(1.0, 0.0, 0.0), [0.0, 0.0, 1.0]);
        let identity = LutTable::identity(4);
        let hald: Vec<u8> = identity.rgb.iter().flat_map(|v| [(v * 255.0).round() as u8]).collect::<Vec<u8>>();
        let mut rgba = Vec::new();
        for triple in hald.chunks(3) {
            rgba.extend_from_slice(triple);
            rgba.push(255);
        }
        let parsed = from_hald(&rgba, 8, 8).unwrap();
        assert_eq!(parsed.size, 4);
        assert!((parsed.sample(0.5, 0.25, 1.0)[0] - 0.5).abs() < 0.01);
        assert!(from_hald(&rgba, 4, 16).is_err());
    }

    #[test]
    fn the_parameter_round_trips_and_resamples_big_tables() {
        let table = LutTable::identity(65);
        let param = encode_lut_param("Big|LUT", &table);
        assert!(param.starts_with("Big/LUT|33|"));
        let (name, decoded) = decode_lut_param(&param).unwrap();
        assert_eq!(name, "Big/LUT");
        assert_eq!(decoded.size, 33);
        assert!((decoded.sample(0.3, 0.6, 0.9)[2] - 0.9).abs() < 0.01);
        assert_eq!(base64_decode(&base64_encode(b"hello!")).unwrap(), b"hello!");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_ne!(lut_key(&param), lut_key("x|2|AAAA"));
        assert_eq!(table.resampled(2).to_texture_rgba().len(), 2 * 2 * 2 * 4);
        assert!(decode_lut_param("nope").is_err());
    }
}
