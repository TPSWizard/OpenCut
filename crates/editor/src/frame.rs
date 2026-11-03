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
    /// With a blurred background: the main track's visible video and image
    /// elements, drawn scaled to cover the canvas and blurred, under the
    /// layers. Their transforms, opacity, effects and masks are ignored.
    pub backdrops: Vec<BackdropPlan>,
    /// Bottom layer first.
    pub layers: Vec<LayerPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackdropPlan {
    pub element_id: String,
    pub source: LayerSource,
    /// Blur strength in the web app's units (presets 100, 200 and 500).
    pub blur_intensity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerPlan {
    pub element_id: String,
    pub track_id: String,
    pub source: LayerSource,
    pub transform: LayerTransform,
    pub opacity: f32,
    pub blend_mode: String,
    /// Enabled effects in order, with their parameters evaluated at this time.
    pub effects: Vec<EffectPlan>,
    /// The element's first mask, if any.
    pub mask: Option<MaskPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectPlan {
    pub id: String,
    pub kind: String,
    pub params: Params,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaskPlan {
    pub id: String,
    pub kind: String,
    pub params: Params,
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

/// The web app sizes text as `fontSize` times the canvas height over this.
pub const FONT_SIZE_REFERENCE: f32 = 90.0;
/// Background padding scales with `fontSize` over this.
pub const FONT_SIZE_RATIO_REFERENCE: f32 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecoration {
    None,
    Underline,
    LineThrough,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub content: String,
    pub font_family: String,
    /// Resolved pixels: the `fontSize` parameter times the canvas height
    /// over [`FONT_SIZE_REFERENCE`], as the web app sizes text.
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: TextAlign,
    pub bold: bool,
    pub italic: bool,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub decoration: TextDecoration,
    pub background: Option<TextBackground>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBackground {
    pub color: [f32; 4],
    /// Percent (0–100) of half the box's smaller side.
    pub corner_radius: f32,
    /// Pixels, already scaled by the font size ratio.
    pub padding_x: f32,
    pub padding_y: f32,
    /// Pixels the box is shifted from the text.
    pub offset_x: f32,
    pub offset_y: f32,
}

impl TextStyle {
    /// `canvas_height` is what the text is sized against: the project
    /// canvas for a text element, the element itself for a text mask.
    pub fn from_params(params: &Params, canvas_height: f32) -> Self {
        let align = match params.text("textAlign", "center").as_ref() {
            "left" => TextAlign::Left,
            "right" => TextAlign::Right,
            _ => TextAlign::Center,
        };
        let weight = params.text("fontWeight", "normal");
        let bold = matches!(weight.as_ref(), "bold" | "600" | "700" | "800" | "900");
        // The web app's limits for the parameter.
        let font_size_param = params.number("fontSize", 15.0).clamp(5.0, 300.0) as f32;
        let ratio = font_size_param / FONT_SIZE_RATIO_REFERENCE;
        let background = params.bool("background.enabled", false).then(|| TextBackground {
            color: parse_hex_color(&params.text("background.color", "#000000"))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            corner_radius: params.number("background.cornerRadius", 0.0).clamp(0.0, 100.0) as f32,
            padding_x: params.number("background.paddingX", 30.0) as f32 * ratio,
            padding_y: params.number("background.paddingY", 42.0) as f32 * ratio,
            offset_x: params.number("background.offsetX", 0.0) as f32,
            offset_y: params.number("background.offsetY", 0.0) as f32,
        });
        Self {
            content: params.text("content", "").into_owned(),
            font_family: params.text("fontFamily", "Arial").into_owned(),
            font_size: (font_size_param * canvas_height.max(1.0) / FONT_SIZE_REFERENCE).max(1.0),
            color: parse_hex_color(&params.text("color", "#ffffff")).unwrap_or([1.0; 4]),
            align,
            bold,
            italic: params.text("fontStyle", "normal").as_ref() == "italic",
            line_height: params.number("lineHeight", 1.2).max(0.5) as f32,
            letter_spacing: params.number("letterSpacing", 0.0) as f32,
            decoration: match params.text("textDecoration", "none").as_ref() {
                "underline" => TextDecoration::Underline,
                "line-through" => TextDecoration::LineThrough,
                _ => TextDecoration::None,
            },
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
    let mut backdrops = Vec::new();
    if let Background::Blur { blur_intensity } = &project.settings.background {
        let main = &project.tracks[project.main_track_index()];
        if !main.hidden {
            for element in &main.elements {
                if element.hidden || !element.contains(time) {
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
                    _ => continue,
                };
                backdrops.push(BackdropPlan {
                    element_id: element.id.clone(),
                    source,
                    blur_intensity: blur_intensity.max(0.0) as f32,
                });
            }
        }
    }
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
            let params = element.params_at(time);
            let source = match &element.kind {
                ElementKind::Video { media_id } => LayerSource::Video {
                    media_id: media_id.clone(),
                    time: element.source_time(time),
                },
                ElementKind::Image { media_id } => LayerSource::Image {
                    media_id: media_id.clone(),
                },
                ElementKind::Text => {
                    LayerSource::Text(TextStyle::from_params(&params, canvas.height as f32))
                }
                ElementKind::Audio { .. } => continue,
            };
            let effects = element
                .effects
                .iter()
                .filter(|effect| effect.enabled)
                .map(|effect| EffectPlan {
                    id: effect.id.clone(),
                    kind: effect.kind.clone(),
                    params: element.effect_params_at(effect, time),
                })
                .collect();
            let mask = element
                .can_be_masked()
                .then(|| element.masks.first())
                .flatten()
                .map(|mask| MaskPlan {
                    id: mask.id.clone(),
                    kind: mask.kind.clone(),
                    params: element.mask_params_at(mask, time),
                });
            layers.push(LayerPlan {
                element_id: element.id.clone(),
                track_id: track.id.clone(),
                source,
                transform: LayerTransform::from_params(&params),
                opacity: params.number("opacity", 1.0).clamp(0.0, 1.0) as f32,
                blend_mode: params.text("blendMode", "normal").into_owned(),
                effects,
                mask,
            });
        }
    }
    FramePlan {
        time,
        width: canvas.width,
        height: canvas.height,
        background,
        backdrops,
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
    fn blur_background_plans_main_track_backdrops() {
        let mut project = Project::new("Backdrop");
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
        let image_id = image.id.clone();
        apply(
            &mut project,
            &Command::InsertElement {
                element: image,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        // A copy of the image at the same time lands on a new (overlay)
        // track, and overlay tracks never become backdrops.
        let mut overlay = Element::from_media(project.media_item("img").unwrap(), seconds(0.0));
        overlay.id = "overlay".to_owned();
        apply(
            &mut project,
            &Command::InsertElement {
                element: overlay,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        assert_eq!(project.tracks.len(), 2);

        let plan = plan_frame(&project, seconds(1.0));
        assert!(plan.backdrops.is_empty(), "a color background has no backdrops");

        project.settings.background = Background::Blur { blur_intensity: 200.0 };
        let plan = plan_frame(&project, seconds(1.0));
        assert_eq!(plan.background, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(plan.backdrops.len(), 1);
        assert_eq!(plan.backdrops[0].element_id, image_id);
        assert_eq!(plan.backdrops[0].blur_intensity, 200.0);
        assert_eq!(
            plan.backdrops[0].source,
            LayerSource::Image {
                media_id: "img".to_owned()
            }
        );
        assert_eq!(plan.layers.len(), 2, "the backdrop does not replace the layers");

        // Outside the element, or with the main track hidden, there is none.
        assert!(plan_frame(&project, seconds(30.0)).backdrops.is_empty());
        let main = project.main_track_index();
        project.tracks[main].hidden = true;
        assert!(plan_frame(&project, seconds(1.0)).backdrops.is_empty());
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

    #[test]
    fn plans_animated_values_effects_and_masks() {
        use crate::document::{Effect, Mask};

        let mut project = Project::new("Animated");
        let mut text = Element::text("Hi", seconds(1.0));
        text.params.set("opacity", 1.0);
        text.animations
            .entry("opacity".to_owned())
            .or_default()
            .upsert(seconds(0.0), 0.0, None);
        text.animations
            .entry("opacity".to_owned())
            .or_default()
            .upsert(seconds(2.0), 1.0, None);
        let mut blur = Effect::blur();
        blur.id = "fx".to_owned();
        text.effects.push(blur);
        let mut off = Effect::blur();
        off.enabled = false;
        text.effects.push(off);
        text.animations
            .entry("effects.fx.params.intensity".to_owned())
            .or_default()
            .upsert(seconds(0.0), 0.0, None);
        text.animations
            .entry("effects.fx.params.intensity".to_owned())
            .or_default()
            .upsert(seconds(2.0), 40.0, None);
        text.masks.push(Mask::ellipse());
        apply(
            &mut project,
            &Command::InsertElement {
                element: text,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();

        let plan = plan_frame(&project, seconds(2.0));
        let layer = &plan.layers[0];
        assert_eq!(layer.opacity, 0.5, "half way between the keys");
        assert_eq!(layer.effects.len(), 1, "disabled effects are left out");
        assert_eq!(layer.effects[0].params.number("intensity", 0.0), 20.0);
        assert!(layer.mask.is_none(), "text cannot be masked");

        let plan = plan_frame(&project, seconds(5.0));
        assert_eq!(plan.layers[0].opacity, 1.0, "held after the last key");
    }
}
