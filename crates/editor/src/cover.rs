//! Cover maker: a frame of the project with a title, as a PNG and as the
//! MP4's cover picture (the web app's `src/cover/layout.ts`, checked against
//! the same `fixtures/cover/cases.json`): the cover sizes, the title's layout
//! for each style, the score that suggests a frame, and the cover project —
//! the project re-fitted to the cover's size with the title as a text clip,
//! so the normal renderer draws it.

use serde::{Deserialize, Serialize};
use time::MediaTime;

use crate::{CanvasSize, Element, Lane, Project, ResizeMode, Track, TrackKind, resize_project};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverStyle {
    /// Big white words with a dark outline, low in the frame.
    #[default]
    Bold,
    /// White words on a dark box, low in the frame.
    Band,
    /// Yellow words with a dark outline, at the top.
    Top,
}

impl CoverStyle {
    pub const ALL: [CoverStyle; 3] = [CoverStyle::Bold, CoverStyle::Band, CoverStyle::Top];

    pub fn label(self) -> &'static str {
        match self {
            CoverStyle::Bold => "Bold",
            CoverStyle::Band => "Band",
            CoverStyle::Top => "Top",
        }
    }

    fn font_share(self) -> f64 {
        match self {
            CoverStyle::Bold => 0.11,
            CoverStyle::Band => 0.075,
            CoverStyle::Top => 0.09,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoverShape {
    #[default]
    Canvas,
    Landscape,
    Portrait,
}

impl CoverShape {
    pub const ALL: [CoverShape; 3] = [CoverShape::Canvas, CoverShape::Landscape, CoverShape::Portrait];

    pub fn label(self) -> &'static str {
        match self {
            CoverShape::Canvas => "Canvas",
            CoverShape::Landscape => "16:9",
            CoverShape::Portrait => "9:16",
        }
    }
}

/// The cover a project keeps in its settings (the web app's `CoverSettings`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverSettings {
    /// Timeline seconds of the frame.
    #[serde(default)]
    pub time: f64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub style: CoverStyle,
    /// Embed it as the cover picture of MP4 exports (at the canvas shape).
    #[serde(default = "yes")]
    pub embed: bool,
}

fn yes() -> bool {
    true
}

impl Default for CoverSettings {
    fn default() -> Self {
        Self { time: 0.0, title: String::new(), style: CoverStyle::Bold, embed: true }
    }
}

/// The pixel size of a cover shape.
pub fn cover_size(shape: CoverShape, canvas: CanvasSize) -> CanvasSize {
    match shape {
        CoverShape::Landscape => CanvasSize { width: 1280, height: 720 },
        CoverShape::Portrait => CanvasSize { width: 1080, height: 1920 },
        CoverShape::Canvas => canvas,
    }
}

pub const MAX_TITLE_LINES: usize = 3;

/// Words wrapped greedily to `max_chars`; a longer word keeps its own line;
/// past `max_lines` the last line ends with an ellipsis.
pub fn wrap_title(text: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(last) if last.chars().count() + 1 + word.chars().count() <= max_chars => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    if lines.len() > max_lines {
        let rest = lines[max_lines - 1..].join(" ");
        let cut: String = rest.chars().take(max_chars.saturating_sub(1).max(1)).collect();
        lines.truncate(max_lines - 1);
        lines.push(format!("{}\u{2026}", cut.trim_end()));
    }
    lines
}

/// The dark box behind a band title, in cover pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverBackground {
    pub color: &'static str,
    pub padding_x: f64,
    pub padding_y: f64,
    pub corner_radius: f64,
}

/// Where and how the title goes.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverTitle {
    pub lines: Vec<String>,
    pub max_chars: usize,
    pub font_px: f64,
    /// The text element's `fontSize` (a share of the height / 90).
    pub font_size: f64,
    /// The text element's `transform.positionY` (from the centre).
    pub position_y: f64,
    pub color: &'static str,
    pub outline_width: f64,
    pub background: Option<CoverBackground>,
}

/// The title's layout on a cover of `width` × `height`.
pub fn cover_title(width: f64, height: f64, style: CoverStyle, title: &str) -> CoverTitle {
    let font_px = width.min(height) * style.font_share();
    let max_chars = ((width * 0.86 / (font_px * 0.56)).floor() as usize).max(8);
    let lines = wrap_title(&title.split_whitespace().collect::<Vec<_>>().join(" "), max_chars, MAX_TITLE_LINES);
    let block = lines.len() as f64 * font_px * 1.2;
    let position_y = match style {
        CoverStyle::Bold => height / 2.0 - 0.1 * height - block / 2.0,
        CoverStyle::Band => height / 2.0 - 0.08 * height - block / 2.0,
        CoverStyle::Top => -height / 2.0 + 0.08 * height + block / 2.0,
    };
    let (color, outline) = match style {
        CoverStyle::Bold => ("#ffffff", 0.08),
        CoverStyle::Band => ("#ffffff", 0.0),
        CoverStyle::Top => ("#ffd60a", 0.1),
    };
    CoverTitle {
        lines,
        max_chars,
        font_px,
        font_size: font_px * 90.0 / height,
        position_y,
        color,
        outline_width: font_px * outline,
        background: (style == CoverStyle::Band).then(|| CoverBackground {
            color: "#000000",
            padding_x: font_px * 0.6,
            padding_y: font_px * 0.3,
            corner_radius: font_px * 0.2,
        }),
    }
}

/// How good a frame is as a cover.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CoverScore {
    pub sharpness: f64,
    pub colourfulness: f64,
    pub exposure: f64,
    pub contrast: f64,
    pub score: f64,
}

/// Scores pixels given as `(r, g, b)` in reading order: sharp, colourful,
/// well exposed, with contrast.
fn score_pixels(pixels: &[[f64; 3]], width: usize, height: usize) -> CoverScore {
    let n = width * height;
    if n == 0 || pixels.len() < n {
        return CoverScore::default();
    }
    let mut lum = Vec::with_capacity(n);
    let (mut sum_y, mut sum_y2, mut sum_rg, mut sum_rg2, mut sum_yb, mut sum_yb2) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for [r, g, b] in &pixels[..n] {
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        lum.push(y);
        sum_y += y;
        sum_y2 += y * y;
        let rg = r - g;
        let yb = 0.5 * (r + g) - b;
        sum_rg += rg;
        sum_rg2 += rg * rg;
        sum_yb += yb;
        sum_yb2 += yb * yb;
    }
    let nf = n as f64;
    let mean_y = sum_y / nf;
    let std_y = (sum_y2 / nf - mean_y * mean_y).max(0.0).sqrt();
    let mut sharpness = 0.0;
    if width >= 3 && height >= 3 {
        let (mut sum, mut sum2, mut count) = (0.0, 0.0, 0.0);
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let i = y * width + x;
                let lap = 4.0 * lum[i] - lum[i - 1] - lum[i + 1] - lum[i - width] - lum[i + width];
                sum += lap;
                sum2 += lap * lap;
                count += 1.0;
            }
        }
        let mean = sum / count;
        sharpness = ((sum2 / count - mean * mean).max(0.0).sqrt() / 40.0).min(1.0);
    }
    let (mu_rg, mu_yb) = (sum_rg / nf, sum_yb / nf);
    let var = (sum_rg2 / nf - mu_rg * mu_rg).max(0.0) + (sum_yb2 / nf - mu_yb * mu_yb).max(0.0);
    let colourfulness = ((var.sqrt() + 0.3 * (mu_rg * mu_rg + mu_yb * mu_yb).sqrt()) / 100.0).min(1.0);
    let exposure = (1.0 - (mean_y - 128.0).abs() / 128.0).max(0.0);
    let contrast = (std_y / 50.0).min(1.0);
    CoverScore {
        sharpness,
        colourfulness,
        exposure,
        contrast,
        score: exposure * (0.4 * sharpness + 0.35 * colourfulness + 0.25 * contrast),
    }
}

/// Scores an RGBA frame.
pub fn frame_score_rgba(rgba: &[u8], width: usize, height: usize) -> CoverScore {
    let pixels: Vec<[f64; 3]> = rgba.chunks_exact(4).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    score_pixels(&pixels, width, height)
}

/// Scores a BGRA frame (the renderer's output).
pub fn frame_score_bgra(bgra: &[u8], width: usize, height: usize) -> CoverScore {
    let pixels: Vec<[f64; 3]> = bgra.chunks_exact(4).map(|p| [p[2] as f64, p[1] as f64, p[0] as f64]).collect();
    score_pixels(&pixels, width, height)
}

/// Frames looked at when suggesting one.
pub const COVER_SAMPLES: usize = 16;

/// Evenly spaced sample times, centred in their slots.
pub fn cover_sample_times(duration: f64, count: usize) -> Vec<f64> {
    (0..count).map(|index| (index as f64 + 0.5) / count as f64 * duration).collect()
}

/// The best-scoring time, away from the first and last 5 % when there is one there.
pub fn pick_cover_time(samples: &[(f64, f64)], duration: f64) -> f64 {
    let inside: Vec<&(f64, f64)> =
        samples.iter().filter(|(time, _)| *time >= 0.05 * duration && *time <= 0.95 * duration).collect();
    let pool: Vec<&(f64, f64)> = if inside.is_empty() { samples.iter().collect() } else { inside };
    let mut best: Option<&(f64, f64)> = None;
    for sample in pool {
        if best.is_none_or(|current| sample.1 > current.1) {
            best = Some(sample);
        }
    }
    best.map_or(0.0, |(time, _)| *time)
}

/// The project as the cover draws it: re-fitted to `size` (fill, so the
/// frame is covered) with the title as a text clip on a new top track at
/// `time`. Render it at `time` and `size`.
pub fn cover_project(project: &Project, size: CanvasSize, time: f64, style: CoverStyle, title: &str) -> Project {
    let mut cover = project.clone();
    if cover.settings.canvas != size {
        resize_project(&mut cover, size, ResizeMode::Fill);
    }
    let layout = cover_title(size.width as f64, size.height as f64, style, title);
    if layout.lines.is_empty() {
        return cover;
    }
    let start = MediaTime::from_seconds_f64((time - 0.5).max(0.0)).unwrap_or(MediaTime::ZERO);
    let mut element = Element::text(&layout.lines.join("\n"), start);
    element.name = "Cover title".to_owned();
    element.duration = MediaTime::from_seconds_f64(time + 1.0).unwrap_or(MediaTime::ZERO) - start;
    element
        .params
        .set("fontSize", layout.font_size)
        .set("fontFamily", "Arial")
        .set("fontWeight", "bold")
        .set("textAlign", "center")
        .set("lineHeight", 1.2)
        .set("color", layout.color)
        .set("transform.positionX", 0.0)
        .set("transform.positionY", layout.position_y);
    if layout.outline_width > 0.0 {
        element.params.set("outline.width", layout.outline_width).set("outline.color", "#000000");
    }
    if let Some(background) = &layout.background {
        element
            .params
            .set("background.enabled", true)
            .set("background.color", background.color)
            .set("background.paddingX", background.padding_x)
            .set("background.paddingY", background.padding_y)
            .set("background.cornerRadius", background.corner_radius);
    }
    let mut track = Track::new(TrackKind::Text, Lane::Overlay, "Cover title");
    track.elements.push(element);
    cover.tracks.insert(0, track);
    cover
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_shared_cases_like_the_web() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/cover/cases.json")).unwrap();
        let close = |a: f64, b: &serde_json::Value| (a - b.as_f64().unwrap()).abs() < 1e-9;
        for case in fixture["titles"].as_array().unwrap() {
            let style = serde_json::from_value::<CoverStyle>(case["style"].clone()).unwrap();
            let out = cover_title(case["width"].as_f64().unwrap(), case["height"].as_f64().unwrap(), style, case["title"].as_str().unwrap());
            let lines: Vec<String> = serde_json::from_value(case["lines"].clone()).unwrap();
            assert_eq!(out.lines, lines);
            assert_eq!(out.max_chars as u64, case["maxChars"].as_u64().unwrap());
            assert!(close(out.font_px, &case["fontPx"]) && close(out.font_size, &case["fontSize"]));
            assert!(close(out.position_y, &case["positionY"]) && close(out.outline_width, &case["outlineWidth"]));
            assert_eq!(out.color, case["color"].as_str().unwrap());
            match (&out.background, case.get("background")) {
                (Some(background), Some(expected)) => {
                    assert!(close(background.padding_x, &expected["paddingX"]) && close(background.padding_y, &expected["paddingY"]));
                    assert!(close(background.corner_radius, &expected["cornerRadius"]));
                }
                (None, None) => {}
                other => panic!("background mismatch {other:?}"),
            }
        }
        for case in fixture["wrap"].as_array().unwrap() {
            let lines: Vec<String> = serde_json::from_value(case["lines"].clone()).unwrap();
            assert_eq!(wrap_title(case["text"].as_str().unwrap(), case["maxChars"].as_u64().unwrap() as usize, MAX_TITLE_LINES), lines);
        }
        for case in fixture["scores"].as_array().unwrap() {
            let rgba: Vec<u8> =
                case["pixels"].as_array().unwrap().iter().flat_map(|p| p.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8)).collect();
            let (w, h) = (case["width"].as_u64().unwrap() as usize, case["height"].as_u64().unwrap() as usize);
            let out = frame_score_rgba(&rgba, w, h);
            for (value, key) in [
                (out.sharpness, "sharpness"),
                (out.colourfulness, "colourfulness"),
                (out.exposure, "exposure"),
                (out.contrast, "contrast"),
                (out.score, "score"),
            ] {
                assert!(close(value, &case[key]), "{} {key}", case["name"]);
            }
            let bgra: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            assert!((frame_score_bgra(&bgra, w, h).score - out.score).abs() < 1e-12);
        }
        for case in fixture["pick"].as_array().unwrap() {
            let samples: Vec<(f64, f64)> = case["samples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s["time"].as_f64().unwrap(), s["score"].as_f64().unwrap()))
                .collect();
            assert_eq!(pick_cover_time(&samples, case["duration"].as_f64().unwrap()), case["time"].as_f64().unwrap());
        }
        assert_eq!(pick_cover_time(&[], 10.0), 0.0);
    }

    #[test]
    fn cover_project_adds_the_title_on_top_at_the_cover_size() {
        let mut project = Project::new("Cover");
        project.settings.canvas = CanvasSize { width: 1920, height: 1080 };
        let size = cover_size(CoverShape::Portrait, project.settings.canvas);
        let cover = cover_project(&project, size, 4.0, CoverStyle::Band, "Pasta in 10 minutes");
        assert_eq!(cover.settings.canvas, size);
        let title = &cover.tracks[0].elements[0];
        assert_eq!(title.params.text("content", ""), "Pasta in 10 minutes");
        assert!(title.contains(MediaTime::from_seconds_f64(4.0).unwrap()));
        assert!(title.params.bool("background.enabled", false));
        assert_eq!(project.settings.canvas.width, 1920, "the project itself is untouched");
        assert!(cover_project(&project, project.settings.canvas, 1.0, CoverStyle::Bold, "  ").tracks.len() == project.tracks.len());
        let settings: CoverSettings = serde_json::from_str(r#"{"time":2.5,"title":"Hi","style":"top"}"#).unwrap();
        assert_eq!((settings.style, settings.embed), (CoverStyle::Top, true));
    }
}
