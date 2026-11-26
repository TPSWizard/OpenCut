//! Word art (row 99): text styles beyond one colour, applied to a flat
//! text raster by both editors. The desktop renderer styles its
//! cosmic-text glyph raster with [`WordArt::compose`]; the web editor
//! styles its canvas glyphs the same way through WASM, so a style looks
//! the same in both.
//!
//! A style has up to three parts, all optional:
//!
//! - **Gradient fill**: a linear gradient of two or three colours across
//!   the glyphs' ink box at an angle (0 degrees runs left to right, 90 top
//!   to bottom). Without one the glyphs keep the colours they were drawn
//!   in (the text colour, rich-text colours, caption highlights).
//! - **Outlines**: up to three, stacked outwards from the glyph edge; each
//!   width is a share of the font size (percent of the em), and each band
//!   starts where the one inside it ends. Built from an exact Euclidean
//!   distance transform of the glyphs.
//! - **Inner shadow**: the colour inside the glyphs near the edge facing
//!   away from the shadow's angle (as CSS `inset` shadows), at a distance
//!   and blur that are shares of the em.
//!
//! The parameters are flat text-clip params (`wordArt.fill`,
//! `wordArt.stroke1Width`, ...: [`keys`]), the gallery is
//! `styles.json` (our own designs). Checked against
//! `fixtures/word-art/cases.json`, from an independent Python version.

use serde::{Deserialize, Serialize};

/// The style gallery (our own designs).
pub const STYLES_JSON: &str = include_str!("../styles.json");
/// Most outlines a style has.
pub const MAX_STROKES: usize = 3;
/// Widths, distances and blurs are percent of the em, at most this.
pub const MAX_SHARE: f64 = 30.0;

/// The text-clip param keys.
pub mod keys {
    pub const STYLE: &str = "wordArt.style";
    pub const FILL: &str = "wordArt.fill";
    pub const GRADIENT_FROM: &str = "wordArt.gradientFrom";
    pub const GRADIENT_VIA: &str = "wordArt.gradientVia";
    pub const GRADIENT_TO: &str = "wordArt.gradientTo";
    pub const GRADIENT_ANGLE: &str = "wordArt.gradientAngle";
    pub const STROKE_COLOR: [&str; 3] = ["wordArt.stroke1Color", "wordArt.stroke2Color", "wordArt.stroke3Color"];
    pub const STROKE_WIDTH: [&str; 3] = ["wordArt.stroke1Width", "wordArt.stroke2Width", "wordArt.stroke3Width"];
    pub const SHADOW_OPACITY: &str = "wordArt.innerShadowOpacity";
    pub const SHADOW_COLOR: &str = "wordArt.innerShadowColor";
    pub const SHADOW_DISTANCE: &str = "wordArt.innerShadowDistance";
    pub const SHADOW_ANGLE: &str = "wordArt.innerShadowAngle";
    pub const SHADOW_BLUR: &str = "wordArt.innerShadowBlur";
}

/// Every word-art param and its value when word art is off, in order.
pub fn default_params() -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    let mut out = vec![
        (keys::STYLE, json!("none")),
        (keys::FILL, json!("solid")),
        (keys::GRADIENT_FROM, json!("#ffffff")),
        (keys::GRADIENT_VIA, json!("")),
        (keys::GRADIENT_TO, json!("#000000")),
        (keys::GRADIENT_ANGLE, json!(90)),
    ];
    for index in 0..MAX_STROKES {
        out.push((keys::STROKE_COLOR[index], json!("#000000")));
        out.push((keys::STROKE_WIDTH[index], json!(0)));
    }
    out.extend([
        (keys::SHADOW_OPACITY, json!(0)),
        (keys::SHADOW_COLOR, json!("#000000")),
        (keys::SHADOW_DISTANCE, json!(4)),
        (keys::SHADOW_ANGLE, json!(90)),
        (keys::SHADOW_BLUR, json!(4)),
    ]);
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub from: [f64; 4],
    pub via: Option<[f64; 4]>,
    pub to: [f64; 4],
    /// Degrees; 0 runs left to right, 90 top to bottom.
    pub angle: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: [f64; 4],
    /// Percent of the em (0 to [`MAX_SHARE`]).
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InnerShadow {
    /// Its alpha is the colour's alpha times the opacity.
    pub color: [f64; 4],
    /// Percent of the em.
    pub distance: f64,
    /// Degrees the shadow falls towards (90 = down: the top edges shade).
    pub angle: f64,
    /// Percent of the em.
    pub blur: f64,
}

/// A word-art style. `None` everywhere means plain text.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WordArt {
    pub fill: Option<Gradient>,
    /// Innermost first; widths above 0 only.
    pub strokes: Vec<Stroke>,
    pub inner_shadow: Option<InnerShadow>,
}

/// Parses `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa` (straight alpha, 0 to 1).
pub fn parse_color(text: &str) -> Option<[f64; 4]> {
    let hex = text.trim().strip_prefix('#')?;
    let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
    let bytes: Vec<u8> = match digits.len() {
        3 | 4 => digits.iter().map(|d| d * 17).collect(),
        6 | 8 => digits.chunks(2).map(|pair| pair[0] * 16 + pair[1]).collect(),
        _ => return None,
    };
    let alpha = bytes.get(3).copied().unwrap_or(255);
    Some([bytes[0] as f64 / 255.0, bytes[1] as f64 / 255.0, bytes[2] as f64 / 255.0, alpha as f64 / 255.0])
}

fn share(value: Option<f64>, default: f64) -> f64 {
    match value {
        Some(v) if v.is_finite() => v.clamp(0.0, MAX_SHARE),
        _ => default,
    }
}

fn angle(value: Option<f64>, default: f64) -> f64 {
    match value {
        Some(v) if v.is_finite() => v.rem_euclid(360.0),
        _ => default,
    }
}

impl WordArt {
    /// Reads the style from a clip's params (`text` and `number` look a key
    /// up); `None` when it has no word art.
    pub fn from_lookup(text: impl Fn(&str) -> Option<String>, number: impl Fn(&str) -> Option<f64>) -> Option<Self> {
        let color = |key: &str, default: [f64; 4]| text(key).and_then(|value| parse_color(&value)).unwrap_or(default);
        let fill = (text(keys::FILL).as_deref() == Some("gradient")).then(|| Gradient {
            from: color(keys::GRADIENT_FROM, [1.0; 4]),
            via: text(keys::GRADIENT_VIA).and_then(|value| parse_color(&value)),
            to: color(keys::GRADIENT_TO, [0.0, 0.0, 0.0, 1.0]),
            angle: angle(number(keys::GRADIENT_ANGLE), 90.0),
        });
        let strokes: Vec<Stroke> = (0..MAX_STROKES)
            .map(|index| Stroke {
                color: color(keys::STROKE_COLOR[index], [0.0, 0.0, 0.0, 1.0]),
                width: share(number(keys::STROKE_WIDTH[index]), 0.0),
            })
            .filter(|stroke| stroke.width > 0.0)
            .collect();
        let opacity = number(keys::SHADOW_OPACITY).filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 100.0) / 100.0;
        let inner_shadow = (opacity > 0.0).then(|| {
            let mut shadow_color = color(keys::SHADOW_COLOR, [0.0, 0.0, 0.0, 1.0]);
            shadow_color[3] *= opacity;
            InnerShadow {
                color: shadow_color,
                distance: share(number(keys::SHADOW_DISTANCE), 4.0),
                angle: angle(number(keys::SHADOW_ANGLE), 90.0),
                blur: share(number(keys::SHADOW_BLUR), 4.0),
            }
        });
        let art = Self { fill, strokes, inner_shadow };
        (!art.is_plain()).then_some(art)
    }

    /// Reads the style from a JSON object of params (the web passes its
    /// clip's word-art params this way); `None` when it has no word art.
    pub fn from_params_json(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let object = value.as_object()?;
        Self::from_lookup(
            |key| object.get(key).and_then(|v| v.as_str().map(str::to_owned)),
            |key| object.get(key).and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())),
        )
    }

    pub fn is_plain(&self) -> bool {
        self.fill.is_none() && self.strokes.is_empty() && self.inner_shadow.is_none()
    }

    /// The outlines' total width in pixels for a font `em_px` tall.
    pub fn stroke_extent(&self, em_px: f64) -> f64 {
        self.strokes.iter().map(|stroke| stroke.width / 100.0 * em_px.max(0.0)).sum()
    }

    /// Whole pixels a raster needs around the glyphs so the outlines fit.
    pub fn outset_px(&self, em_px: f64) -> u32 {
        let extent = self.stroke_extent(em_px);
        if extent > 0.0 { extent.ceil() as u32 + 1 } else { 0 }
    }

    /// Styles a glyph raster (straight-alpha RGBA, `width` x `height`, the
    /// glyphs only: no background box) for a font `em_px` pixels tall.
    /// Returns the styled raster, the same size: outlines behind, then the
    /// fill with its inner shadow.
    pub fn compose(&self, width: usize, height: usize, rgba: &[u8], em_px: f64) -> Vec<u8> {
        let count = width * height;
        if rgba.len() < count * 4 || count == 0 {
            return rgba.to_vec();
        }
        let alpha: Vec<f64> = (0..count).map(|i| rgba[i * 4 + 3] as f64 / 255.0).collect();
        let Some((x0, y0, x1, y1)) = ink_box(width, height, &rgba[..count * 4]) else {
            return rgba.to_vec();
        };
        let em = em_px.max(0.0);

        // The fill: the gradient, or the colours the glyphs were drawn in.
        let mut fill: Vec<[f64; 4]> = (0..count)
            .map(|i| {
                [
                    rgba[i * 4] as f64 / 255.0,
                    rgba[i * 4 + 1] as f64 / 255.0,
                    rgba[i * 4 + 2] as f64 / 255.0,
                    alpha[i],
                ]
            })
            .collect();
        if let Some(gradient) = &self.fill {
            let (cos, sin) = (gradient.angle.to_radians().cos(), gradient.angle.to_radians().sin());
            let (cx, cy) = ((x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0);
            let half = (cos.abs() * (x1 - x0) as f64 + sin.abs() * (y1 - y0) as f64) / 2.0;
            for y in 0..height {
                for x in 0..width {
                    let i = y * width + x;
                    let along = (x as f64 + 0.5 - cx) * cos + (y as f64 + 0.5 - cy) * sin;
                    let t = if half > 0.0 { (along / (2.0 * half) + 0.5).clamp(0.0, 1.0) } else { 0.5 };
                    let c = gradient_at(gradient, t);
                    fill[i] = [c[0], c[1], c[2], c[3] * alpha[i]];
                }
            }
        }

        // The inner shadow: the glyphs' outside, moved by the offset and
        // blurred, shades the fill where it overlaps (keeping its alpha).
        if let Some(shadow) = &self.inner_shadow {
            let distance = shadow.distance / 100.0 * em;
            let (dx, dy) = (
                round_half_up(shadow.angle.to_radians().cos() * distance) as i64,
                round_half_up(shadow.angle.to_radians().sin() * distance) as i64,
            );
            let mut outside: Vec<f64> = (0..count)
                .map(|i| {
                    let (x, y) = ((i % width) as i64 - dx, (i / width) as i64 - dy);
                    if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 { 1.0 } else { 1.0 - alpha[y as usize * width + x as usize] }
                })
                .collect();
            let radius = round_half_up(shadow.blur / 100.0 * em / 2.0) as usize;
            if radius > 0 {
                for _ in 0..3 {
                    outside = box_blur(&outside, width, height, radius, true);
                }
                for _ in 0..3 {
                    outside = box_blur(&outside, width, height, radius, false);
                }
            }
            for i in 0..count {
                let k = (outside[i] * shadow.color[3]).clamp(0.0, 1.0);
                for channel in 0..3 {
                    fill[i][channel] = fill[i][channel] * (1.0 - k) + shadow.color[channel] * k;
                }
            }
        }

        // The outlines, outermost first, then the fill over them.
        let mut out = vec![[0.0f64; 4]; count];
        if !self.strokes.is_empty() {
            let distance = distance_to_ink(width, height, &alpha);
            let mut bands = Vec::with_capacity(self.strokes.len());
            let mut reach = 0.0;
            for stroke in &self.strokes {
                reach += stroke.width / 100.0 * em;
                bands.push((reach, stroke.color));
            }
            for (reach, color) in bands.iter().rev() {
                for i in 0..count {
                    let cover = alpha[i].max((reach + 1.0 - distance[i]).clamp(0.0, 1.0));
                    over(&mut out[i], [color[0], color[1], color[2], color[3] * cover]);
                }
            }
        }
        for i in 0..count {
            over(&mut out[i], fill[i]);
        }
        let mut bytes = vec![0u8; count * 4];
        for (i, pixel) in out.iter().enumerate() {
            for channel in 0..4 {
                bytes[i * 4 + channel] = to_byte(pixel[channel]);
            }
        }
        bytes
    }
}

fn round_half_up(value: f64) -> f64 {
    (value + 0.5).floor()
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

/// The box (x0, y0, x1, y1; ends exclusive) of the pixels with any ink.
fn ink_box(width: usize, height: usize, rgba: &[u8]) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if rgba[(y * width + x) * 4 + 3] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 != usize::MAX).then_some((x0, y0, x1, y1))
}

fn gradient_at(gradient: &Gradient, t: f64) -> [f64; 4] {
    let lerp = |a: [f64; 4], b: [f64; 4], t: f64| [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * t);
    match gradient.via {
        Some(via) if t < 0.5 => lerp(gradient.from, via, t * 2.0),
        Some(via) => lerp(via, gradient.to, t * 2.0 - 1.0),
        None => lerp(gradient.from, gradient.to, t),
    }
}

/// Straight-alpha source-over of `src` onto `dst`.
fn over(dst: &mut [f64; 4], src: [f64; 4]) {
    let (sa, da) = (src[3].clamp(0.0, 1.0), dst[3]);
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        *dst = [0.0; 4];
        return;
    }
    for c in 0..3 {
        dst[c] = (src[c] * sa + dst[c] * da * (1.0 - sa)) / out_a;
    }
    dst[3] = out_a;
}

/// One box blur pass (radius `radius`) along rows or columns; samples
/// past the edge count as 1 (outside the glyphs).
fn box_blur(values: &[f64], width: usize, height: usize, radius: usize, rows: bool) -> Vec<f64> {
    let mut out = vec![0.0; values.len()];
    let (lines, length) = if rows { (height, width) } else { (width, height) };
    let at = |line: usize, pos: usize| if rows { line * width + pos } else { pos * width + line };
    let window = (2 * radius + 1) as f64;
    for line in 0..lines {
        let sample = |pos: i64| if pos < 0 || pos >= length as i64 { 1.0 } else { values[at(line, pos as usize)] };
        let mut sum: f64 = (-(radius as i64)..=radius as i64).map(sample).sum();
        for pos in 0..length {
            out[at(line, pos)] = sum / window;
            sum += sample(pos as i64 + radius as i64 + 1) - sample(pos as i64 - radius as i64);
        }
    }
    out
}

/// Each pixel's distance (pixel centres) to the nearest inked pixel: those
/// at least half covered, or any ink when none is.
fn distance_to_ink(width: usize, height: usize, alpha: &[f64]) -> Vec<f64> {
    let solid = alpha.iter().any(|&a| a >= 0.5);
    let seed = |a: f64| if solid { a >= 0.5 } else { a > 0.0 };
    const FAR: f64 = 1e20;
    let mut grid: Vec<f64> = alpha.iter().map(|&a| if seed(a) { 0.0 } else { FAR }).collect();
    // Felzenszwalb and Huttenlocher's exact squared distance transform,
    // columns then rows.
    let mut column = vec![0.0; height];
    for x in 0..width {
        for y in 0..height {
            column[y] = grid[y * width + x];
        }
        let done = transform_1d(&column);
        for y in 0..height {
            grid[y * width + x] = done[y];
        }
    }
    let mut row = vec![0.0; width];
    for y in 0..height {
        row.copy_from_slice(&grid[y * width..(y + 1) * width]);
        let done = transform_1d(&row);
        grid[y * width..(y + 1) * width].copy_from_slice(&done);
    }
    grid.into_iter().map(|d| if d >= FAR / 2.0 { f64::INFINITY } else { d.sqrt() }).collect()
}

fn transform_1d(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut d = vec![0.0; n];
    if n == 0 {
        return d;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        // z[0] is minus infinity, so k never goes below 0.
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
            if s <= z[k] {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let diff = q as f64 - p as f64;
        *out = diff * diff + f[p];
    }
    d
}

/// A gallery entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WordArtStyle {
    pub id: String,
    pub label: String,
    pub description: String,
    /// The params it sets over the defaults (word art and maybe `color`).
    pub params: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Library {
    styles: Vec<WordArtStyle>,
}

/// The gallery, in order (the first is "Plain"); read once.
pub fn styles() -> &'static [WordArtStyle] {
    static STYLES: std::sync::OnceLock<Vec<WordArtStyle>> = std::sync::OnceLock::new();
    STYLES.get_or_init(|| serde_json::from_str::<Library>(STYLES_JSON).map(|library| library.styles).unwrap_or_default())
}

/// The params a gallery style sets on a text clip: every word-art param
/// (its own values over the defaults, so a previous style is cleared),
/// `wordArt.style`, and any other param it names (`color`). `None` for an
/// unknown id.
pub fn style_params(id: &str) -> Option<Vec<(String, serde_json::Value)>> {
    let style = styles().iter().find(|style| style.id == id)?;
    let mut out: Vec<(String, serde_json::Value)> = default_params()
        .into_iter()
        .map(|(key, value)| (key.to_owned(), style.params.get(key).cloned().unwrap_or(value)))
        .collect();
    for entry in out.iter_mut() {
        if entry.0 == keys::STYLE {
            entry.1 = serde_json::Value::String(style.id.clone());
        }
    }
    for (key, value) in &style.params {
        if !out.iter().any(|(existing, _)| existing == key) {
            out.push((key.clone(), value.clone()));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_has_no_word_art() {
        assert_eq!(WordArt::from_params_json("{}"), None);
        assert_eq!(WordArt::from_params_json(r#"{"wordArt.fill":"solid","wordArt.stroke1Width":0}"#), None);
        let art = WordArt::from_params_json(r##"{"wordArt.stroke2Width":5,"wordArt.stroke2Color":"#ff0000"}"##).unwrap();
        assert_eq!(art.strokes, vec![Stroke { color: [1.0, 0.0, 0.0, 1.0], width: 5.0 }]);
        assert_eq!(art.outset_px(100.0), 6);
    }

    #[test]
    fn every_style_reads_and_sets_every_key() {
        let all = styles();
        assert_eq!(all[0].id, "none");
        assert!(all.len() >= 10);
        for style in all {
            let params = style_params(&style.id).unwrap();
            for (key, _) in default_params() {
                assert!(params.iter().any(|(k, _)| k == key), "{} lacks {}", style.id, key);
            }
            let object: serde_json::Map<String, serde_json::Value> = params.into_iter().collect();
            let art = WordArt::from_params_json(&serde_json::Value::Object(object).to_string());
            assert_eq!(art.is_none(), style.id == "none", "{}", style.id);
            for key in style.params.keys() {
                assert!(key.starts_with("wordArt.") || key == "color", "{} sets {}", style.id, key);
            }
        }
        assert_eq!(style_params("nope"), None);
    }

    #[test]
    fn distance_transform_is_exact() {
        let (w, h) = (9, 7);
        let mut alpha = vec![0.0; w * h];
        alpha[3 * w + 4] = 1.0;
        alpha[w + 1] = 1.0;
        let d = distance_to_ink(w, h, &alpha);
        for y in 0..h {
            for x in 0..w {
                let want = [(4.0, 3.0), (1.0, 1.0)]
                    .iter()
                    .map(|(sx, sy)| ((x as f64 - sx).powi(2) + (y as f64 - sy).powi(2)).sqrt())
                    .fold(f64::INFINITY, f64::min);
                assert!((d[y * w + x] - want).abs() < 1e-9, "{x},{y}");
            }
        }
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        width: usize,
        height: usize,
        em: f64,
        params: serde_json::Value,
        input: Vec<u8>,
        output: Vec<u8>,
    }

    #[test]
    fn matches_the_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/word-art/cases.json");
        let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(cases.len() >= 8);
        for case in cases {
            let art = WordArt::from_params_json(&case.params.to_string()).unwrap();
            let got = art.compose(case.width, case.height, &case.input, case.em);
            let worst = got.iter().zip(&case.output).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap_or(0);
            assert!(worst <= 1, "{}: off by {}", case.name, worst);
        }
    }
}
