//! What is on screen at a point in time, independent of how it is drawn.

use time::MediaTime;

use crate::document::{Background, ElementKind, Project};
use crate::params::Params;

#[derive(Clone, Debug, PartialEq)]
pub struct FramePlan {
    pub time: MediaTime,
    pub width: u32,
    pub height: u32,
    /// Linear-ish sRGB components, straight alpha.
    pub background: [f32; 4],
    /// Bottom layer first.
    pub layers: Vec<LayerPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerPlan {
    pub element_id: String,
    pub track_id: String,
    pub source: LayerSource,
    pub transform: LayerTransform,
    pub opacity: f32,
    pub blend_mode: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerSource {
    Video {
        media_id: String,
        /// Time within the source file.
        time: MediaTime,
    },
    Image {
        media_id: String,
    },
    Text(TextStyle),
}

/// Offsets are canvas pixels from the canvas centre; scale is a multiplier
/// on the element's natural size after it has been fitted to the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerTransform {
    pub position_x: f32,
    pub position_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotate_degrees: f32,
}

impl LayerTransform {
    pub fn from_params(params: &Params) -> Self {
        Self {
            position_x: params.number("transform.positionX", 0.0) as f32,
            position_y: params.number("transform.positionY", 0.0) as f32,
            scale_x: params.number("transform.scaleX", 1.0) as f32,
            scale_y: params.number("transform.scaleY", 1.0) as f32,
            rotate_degrees: params.number("transform.rotate", 0.0) as f32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub content: String,
    pub font_family: String,
    /// Canvas pixels.
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: TextAlign,
    pub bold: bool,
    pub italic: bool,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub background: Option<TextBackground>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBackground {
    pub color: [f32; 4],
    pub corner_radius: f32,
    pub padding_x: f32,
    pub padding_y: f32,
}

impl TextStyle {
    pub fn from_params(params: &Params) -> Self {
        let align = match params.text("textAlign", "center").as_ref() {
            "left" => TextAlign::Left,
            "right" => TextAlign::Right,
            _ => TextAlign::Center,
        };
        let weight = params.text("fontWeight", "normal");
        let bold = matches!(weight.as_ref(), "bold" | "600" | "700" | "800" | "900");
        let background = params.bool("background.enabled", false).then(|| TextBackground {
            color: parse_hex_color(&params.text("background.color", "#000000"))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            corner_radius: params.number("background.cornerRadius", 0.0) as f32,
            padding_x: params.number("background.paddingX", 30.0) as f32,
            padding_y: params.number("background.paddingY", 42.0) as f32,
        });
        Self {
            content: params.text("content", "").into_owned(),
            font_family: params.text("fontFamily", "Arial").into_owned(),
            font_size: params.number("fontSize", 72.0).max(1.0) as f32,
            color: parse_hex_color(&params.text("color", "#ffffff")).unwrap_or([1.0; 4]),
            align,
            bold,
            italic: params.text("fontStyle", "normal").as_ref() == "italic",
            line_height: params.number("lineHeight", 1.2).max(0.5) as f32,
            letter_spacing: params.number("letterSpacing", 0.0) as f32,
            background,
        }
    }
}

/// Parses `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa` into straight-alpha components.
pub fn parse_hex_color(text: &str) -> Option<[f32; 4]> {
    let hex = text.trim().trim_start_matches('#');
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()?;
    let bytes: Vec<u8> = match digits.len() {
        3 | 4 => digits.iter().map(|d| d * 17).collect(),
        6 | 8 => digits.chunks(2).map(|pair| pair[0] * 16 + pair[1]).collect(),
        _ => return None,
    };
    let alpha = bytes.get(3).copied().unwrap_or(255);
    Some([
        bytes[0] as f32 / 255.0,
        bytes[1] as f32 / 255.0,
        bytes[2] as f32 / 255.0,
        alpha as f32 / 255.0,
    ])
}

/// Scales `(width, height)` to fit inside the canvas, keeping its aspect ratio.
pub fn fit_contain(width: f32, height: f32, canvas_width: f32, canvas_height: f32) -> (f32, f32) {
    if width <= 0.0 || height <= 0.0 {
        return (canvas_width, canvas_height);
    }
    let scale = (canvas_width / width).min(canvas_height / height);
    (width * scale, height * scale)
}

pub fn plan_frame(project: &Project, time: MediaTime) -> FramePlan {
    let canvas = project.settings.canvas;
    let background = match &project.settings.background {
        Background::Color { color } => parse_hex_color(color).unwrap_or([0.0, 0.0, 0.0, 1.0]),
        Background::Blur { .. } => [0.0, 0.0, 0.0, 1.0],
    };
    let mut layers = Vec::new();
    // Tracks are stored top to bottom; draw bottom first.
    for track in project.tracks.iter().rev() {
        if track.hidden {
            continue;
        }
        for element in &track.elements {
            if element.hidden || !element.contains(time) || !element.kind.is_visual() {
                continue;
            }
            let source = match &element.kind {
                ElementKind::Video { media_id } => LayerSource::Video {
                    media_id: media_id.clone(),
                    time: element.source_time(time),
                },
                ElementKind::Image { media_id } => LayerSource::Image {
                    media_id: media_id.clone(),
                },
                ElementKind::Text => LayerSource::Text(TextStyle::from_params(&element.params)),
                ElementKind::Audio { .. } => continue,
            };
            layers.push(LayerPlan {
                element_id: element.id.clone(),
                track_id: track.id.clone(),
                source,
                transform: LayerTransform::from_params(&element.params),
                opacity: element.params.number("opacity", 1.0).clamp(0.0, 1.0) as f32,
                blend_mode: element.params.text("blendMode", "normal").into_owned(),
            });
        }
    }
    FramePlan {
        time,
        width: canvas.width,
        height: canvas.height,
        background,
        layers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, TrackPlacement, apply};
    use crate::document::{Element, MediaItem, MediaKind};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_hex_color("#fff"), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_hex_color("#000000"), Some([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex_color("#ff000080").map(|c| (c[3] * 255.0).round()), Some(128.0));
        assert_eq!(parse_hex_color("nope"), None);
    }

    #[test]
    fn fits_inside_canvas() {
        assert_eq!(fit_contain(400.0, 300.0, 1920.0, 1080.0), (1440.0, 1080.0));
        assert_eq!(fit_contain(1920.0, 1080.0, 1920.0, 1080.0), (1920.0, 1080.0));
        let (width, height) = fit_contain(100.0, 1000.0, 1920.0, 1080.0);
        assert!((width - 108.0).abs() < 0.001 && height == 1080.0);
    }

    #[test]
    fn plans_visible_layers_bottom_first() {
        let mut project = Project::new("Plan");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "img".to_owned(),
                name: "img.png".to_owned(),
                kind: MediaKind::Image,
                path: "img.png".into(),
                duration: None,
                width: Some(10),
                height: Some(10),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let image = Element::from_media(project.media_item("img").unwrap(), seconds(0.0));
        apply(
            &mut project,
            &Command::InsertElement {
                element: image,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        let mut text = Element::text("Hi", seconds(1.0));
        text.params.set("opacity", 0.5).set("transform.positionX", 20.0);
        apply(
            &mut project,
            &Command::InsertElement {
                element: text,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();

        let plan = plan_frame(&project, seconds(0.5));
        assert_eq!(plan.layers.len(), 1);
        assert!(matches!(plan.layers[0].source, LayerSource::Image { .. }));

        let plan = plan_frame(&project, seconds(2.0));
        assert_eq!(plan.layers.len(), 2);
        assert!(matches!(plan.layers[0].source, LayerSource::Image { .. }));
        let LayerSource::Text(style) = &plan.layers[1].source else {
            panic!("expected text on top");
        };
        assert_eq!(style.content, "Hi");
        assert_eq!(plan.layers[1].opacity, 0.5);
        assert_eq!(plan.layers[1].transform.position_x, 20.0);

        let plan = plan_frame(&project, seconds(6.0));
        assert!(plan.layers.is_empty());
        assert_eq!((plan.width, plan.height), (1920, 1080));
    }
}
