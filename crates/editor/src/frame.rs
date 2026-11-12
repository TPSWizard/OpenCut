//! What is on screen at a point in time, independent of how it is drawn.

use time::MediaTime;

use crate::document::{Background, CaptionWord, Element, ElementKind, Project, Track};
use crate::params::Params;
use crate::transition::{next_adjacent, transition_window, window_into};

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
    /// Bottom item first.
    pub items: Vec<FrameItemPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FrameItemPlan {
    Layer(LayerPlan),
    Transition(TransitionPlan),
}

/// Two clips mid-transition: `from` is the outgoing clip drawn past its end
/// (or nothing), `to` the incoming one drawn before its start (or nothing).
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionPlan {
    pub element_id: String,
    pub track_id: String,
    pub kind: String,
    pub progress: f32,
    pub params: Params,
    pub from: Option<LayerPlan>,
    pub to: Option<LayerPlan>,
}

impl FramePlan {
    /// Every layer drawn this frame, transition sides included.
    pub fn layers(&self) -> Vec<&LayerPlan> {
        self.items
            .iter()
            .flat_map(|item| match item {
                FrameItemPlan::Layer(layer) => vec![layer],
                FrameItemPlan::Transition(transition) => {
                    transition.from.iter().chain(transition.to.iter()).collect()
                }
            })
            .collect()
    }
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
    /// Time into the element, for effects that move (grain, glitch).
    pub local_time: MediaTime,
    /// The element's first mask, if any.
    pub mask: Option<MaskPlan>,
    pub cutout: Option<CutoutPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectPlan {
    pub id: String,
    pub kind: String,
    pub params: Params,
}

/// A cut-out to apply to a video layer: the matte's frame at `time`
/// (matte seconds) is the layer's coverage, multiplied into its mask.
#[derive(Clone, Debug, PartialEq)]
pub struct CutoutPlan {
    pub matte_id: String,
    pub time: f64,
    pub inverted: bool,
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
    /// A sticker by id, with the intrinsic size the web app stored (SVG
    /// size when none).
    Sticker {
        sticker_id: String,
        intrinsic_width: Option<f64>,
        intrinsic_height: Option<f64>,
    },
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

/// How a caption's spoken word stands out (the web app's `caption.highlight`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightMode {
    /// The word being spoken takes the active colour.
    Word,
    /// The word being spoken sits on a rounded box and takes the active colour.
    Box,
    /// Every word spoken so far takes the active colour (karaoke).
    Fill,
}

impl HighlightMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "word" => Some(Self::Word),
            "box" => Some(Self::Box),
            "fill" => Some(Self::Fill),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Box => "box",
            Self::Fill => "fill",
        }
    }
}

/// Which words of a caption to emphasise at the planned time. The words are
/// the whitespace-separated tokens of the content, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct TextHighlight {
    pub mode: HighlightMode,
    pub active_color: [f32; 4],
    pub box_color: [f32; 4],
    /// The word being spoken, if the time falls inside one.
    pub active_index: Option<usize>,
    /// How many words have started (the active one included).
    pub spoken_count: usize,
}

impl TextHighlight {
    /// The indices of the words that take the active colour.
    pub fn highlighted_indices(&self) -> Vec<usize> {
        match self.mode {
            HighlightMode::Word | HighlightMode::Box => self.active_index.into_iter().collect(),
            HighlightMode::Fill => (0..self.spoken_count).collect(),
        }
    }
}

/// The word spoken at `local_time` and how many words have started by then
/// (the web app's `getActiveWordState`).
pub fn active_word(words: &[CaptionWord], local_time: MediaTime) -> (Option<usize>, usize) {
    let mut active = None;
    let mut spoken = 0;
    for (index, word) in words.iter().enumerate() {
        if word.start > local_time {
            break;
        }
        spoken = index + 1;
        if local_time < word.end {
            active = Some(index);
        }
    }
    (active, spoken)
}

/// The highlight for a caption at `local_time`, or `None` without words or
/// with `caption.highlight` set to `none`.
pub fn caption_highlight(params: &Params, words: &[CaptionWord], local_time: MediaTime) -> Option<TextHighlight> {
    if words.is_empty() {
        return None;
    }
    let mode = HighlightMode::parse(params.text("caption.highlight", "none").as_ref())?;
    let (active_index, spoken_count) = active_word(words, local_time);
    Some(TextHighlight {
        mode,
        active_color: parse_hex_color(&params.text("caption.activeColor", "#ffd60a")).unwrap_or([1.0, 0.84, 0.04, 1.0]),
        box_color: parse_hex_color(&params.text("caption.boxColor", "#e5484d")).unwrap_or([0.9, 0.28, 0.3, 1.0]),
        active_index,
        spoken_count,
    })
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
    /// Caption emphasis at the planned time; `None` for plain text.
    pub highlight: Option<TextHighlight>,
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
            highlight: None,
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
    // Compound clips are drawn as their children.
    let expanded = crate::expand_compounds(project);
    let project: &Project = &expanded;
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
    let mut items = Vec::new();
    // Tracks are stored top to bottom; draw bottom first.
    for track in project.tracks.iter().rev() {
        if track.hidden {
            continue;
        }
        for element in &track.elements {
            if element.hidden || !element.kind.is_visual() {
                continue;
            }
            // Mid-transition, the clip is drawn by the transition it is part
            // of: as the incoming side of the previous clip's transition,
            // or as the outgoing side of its own.
            if window_into(track, element).is_some_and(|window| window.contains(time)) {
                continue;
            }
            let next = next_adjacent(track, element);
            if let Some(window) = transition_window(element, next)
                && window.contains(time)
            {
                let transition = element.transition.as_ref().expect("has a window");
                items.push(FrameItemPlan::Transition(TransitionPlan {
                    element_id: element.id.clone(),
                    track_id: track.id.clone(),
                    kind: transition.kind.clone(),
                    progress: window.progress(time),
                    params: transition.params.clone(),
                    from: layer_plan(track, element, time, canvas.height as f32, true),
                    to: next.and_then(|next| layer_plan(track, next, time, canvas.height as f32, true)),
                }));
                continue;
            }
            if !element.contains(time) {
                continue;
            }
            if let Some(layer) = layer_plan(track, element, time, canvas.height as f32, false) {
                items.push(FrameItemPlan::Layer(layer));
            }
        }
    }
    FramePlan {
        time,
        width: canvas.width,
        height: canvas.height,
        background,
        backdrops,
        items,
    }
}

/// A visual element as a layer at `time`. With `extended`, the time may fall
/// outside the clip (a transition's handles): the source keeps playing from
/// the material beyond the trim and holds its edge frame when there is none,
/// while keyframes hold their edge values.
fn layer_plan(
    track: &Track,
    element: &Element,
    time: MediaTime,
    canvas_height: f32,
    extended: bool,
) -> Option<LayerPlan> {
    let param_time = if extended {
        time.clamp(element.start_time, element.end_time() - MediaTime::ONE_TICK)
    } else {
        time
    };
    let params = element.params_at(param_time);
    let source = match &element.kind {
        ElementKind::Video { media_id } => LayerSource::Video {
            media_id: media_id.clone(),
            time: if extended { extended_source_time(element, time) } else { element.source_time(time) },
        },
        ElementKind::Image { media_id } => LayerSource::Image {
            media_id: media_id.clone(),
        },
        ElementKind::Sticker { sticker_id, intrinsic_width, intrinsic_height } => LayerSource::Sticker {
            sticker_id: sticker_id.clone(),
            intrinsic_width: *intrinsic_width,
            intrinsic_height: *intrinsic_height,
        },
        ElementKind::Text => {
            let mut style = TextStyle::from_params(&params, canvas_height);
            style.highlight = caption_highlight(&params, &element.words, element.local_time(param_time));
            LayerSource::Text(style)
        }
        // Compounds are expanded before planning; audio draws nothing.
        ElementKind::Audio { .. } | ElementKind::Compound { .. } => return None,
    };
    let effects = element
        .effects
        .iter()
        .filter(|effect| effect.enabled)
        .map(|effect| EffectPlan {
            id: effect.id.clone(),
            kind: effect.kind.clone(),
            params: element.effect_params_at(effect, param_time),
        })
        .collect();
    let mask = element
        .can_be_masked()
        .then(|| element.masks.first())
        .flatten()
        .map(|mask| MaskPlan {
            id: mask.id.clone(),
            kind: mask.kind.clone(),
            params: element.mask_params_at(mask, param_time),
        });
    let cutout = match (&source, &element.cutout) {
        (LayerSource::Video { time, .. }, Some(cutout)) => Some(CutoutPlan {
            matte_id: cutout.matte_id.clone(),
            time: (time.to_seconds_f64() - cutout.offset).max(0.0),
            inverted: cutout.inverted,
        }),
        _ => None,
    };
    Some(LayerPlan {
        element_id: element.id.clone(),
        track_id: track.id.clone(),
        source,
        transform: LayerTransform::from_params(&params),
        opacity: params.number("opacity", 1.0).clamp(0.0, 1.0) as f32,
        blend_mode: params.text("blendMode", "normal").into_owned(),
        effects,
        local_time: element.local_time(param_time),
        mask,
        cutout,
    })
}

/// The source time for a timeline time that may lie outside the clip,
/// clamped to the source so an exhausted handle holds its edge frame.
fn extended_source_time(element: &Element, time: MediaTime) -> MediaTime {
    let raw = element.source_time(time);
    let last = element
        .source_duration
        .map(|source| (source - MediaTime::ONE_TICK).max(MediaTime::ZERO))
        .unwrap_or(raw);
    raw.clamp(MediaTime::ZERO, last.max(MediaTime::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, ElementPatch, TrackPlacement, apply};
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
        assert_eq!(plan.layers().len(), 2, "the backdrop does not replace the layers");

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
        assert_eq!(plan.layers().len(), 1);
        assert!(matches!(plan.layers()[0].source, LayerSource::Image { .. }));

        let plan = plan_frame(&project, seconds(2.0));
        assert_eq!(plan.layers().len(), 2);
        assert!(matches!(plan.layers()[0].source, LayerSource::Image { .. }));
        let LayerSource::Text(style) = &plan.layers()[1].source else {
            panic!("expected text on top");
        };
        assert_eq!(style.content, "Hi");
        assert_eq!(plan.layers()[1].opacity, 0.5);
        assert_eq!(plan.layers()[1].transform.position_x, 20.0);

        let plan = plan_frame(&project, seconds(6.0));
        assert!(plan.layers().is_empty());
        assert_eq!((plan.width, plan.height), (1920, 1080));
    }

    #[test]
    fn captions_highlight_the_spoken_word() {
        let mut project = Project::new("Captions");
        let mut text = Element::text("Hello there world", seconds(1.0));
        text.words = vec![
            CaptionWord { text: "Hello".into(), start: seconds(0.0), end: seconds(0.4) },
            CaptionWord { text: "there".into(), start: seconds(0.5), end: seconds(0.9) },
            CaptionWord { text: "world".into(), start: seconds(1.2), end: seconds(1.6) },
        ];
        text.params.set("caption.highlight", "box").set("caption.activeColor", "#ff0000");
        apply(&mut project, &Command::InsertElement { element: text, placement: TrackPlacement::Auto }).unwrap();

        fn style_at(project: &Project, time: f64) -> TextStyle {
            let plan = plan_frame(project, seconds(time));
            let LayerSource::Text(style) = &plan.layers()[0].source else { panic!("text") };
            style.clone()
        }
        let element_id = project.tracks[0].elements[0].id.clone();
        let highlight = style_at(&project, 1.6).highlight.expect("highlighted");
        assert_eq!(highlight.mode, HighlightMode::Box);
        assert_eq!(highlight.active_index, Some(1));
        assert_eq!(highlight.spoken_count, 2);
        assert_eq!(highlight.active_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(highlight.highlighted_indices(), vec![1]);
        // In the pause after "there" nothing is active, but two words have been spoken.
        let pause = style_at(&project, 2.0).highlight.unwrap();
        assert_eq!((pause.active_index, pause.spoken_count), (None, 2));
        // At the very start the first word is active.
        assert_eq!(active_word(&project.tracks[0].elements[0].words, MediaTime::ZERO), (Some(0), 1));

        // `fill` colours everything spoken so far; `none` drops the highlight.
        let set_highlight = |project: &mut Project, mode: &str| {
            let mut params = Params::new();
            params.set("caption.highlight", mode);
            apply(
                project,
                &Command::UpdateElement {
                    element_id: element_id.clone(),
                    patch: ElementPatch { params: Some(params), ..Default::default() },
                },
            )
            .unwrap();
        };
        set_highlight(&mut project, "fill");
        assert_eq!(style_at(&project, 2.0).highlight.unwrap().highlighted_indices(), vec![0, 1]);
        set_highlight(&mut project, "none");
        assert!(style_at(&project, 2.0).highlight.is_none());
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
        let layer = &plan.layers()[0];
        assert_eq!(layer.opacity, 0.5, "half way between the keys");
        assert_eq!(layer.effects.len(), 1, "disabled effects are left out");
        assert_eq!(layer.effects[0].params.number("intensity", 0.0), 20.0);
        assert!(layer.mask.is_none(), "text cannot be masked");

        let plan = plan_frame(&project, seconds(5.0));
        assert_eq!(plan.layers()[0].opacity, 1.0, "held after the last key");
    }
    #[test]
    fn transitions_replace_both_clips_inside_their_window() {
        use crate::document::Transition;
        let mut project = Project::new("Transitions");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "vid".to_owned(),
                name: "vid.mp4".to_owned(),
                kind: MediaKind::Video,
                path: "vid.mp4".into(),
                duration: Some(seconds(10.0)),
                width: Some(10),
                height: Some(10),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        // a: source 0..4 s, on the timeline 0..4 s, with 6 s of handle after it.
        let mut a = Element::from_media(project.media_item("vid").unwrap(), seconds(0.0));
        a.id = "a".to_owned();
        a.duration = seconds(4.0);
        a.trim_end = seconds(6.0);
        a.transition = Some(Transition {
            kind: "DirectionalWipe".to_owned(),
            duration: seconds(1.0),
            params: Params::new(),
        });
        // b: source 2..6 s, on the timeline 4..8 s, with a 2 s handle before it.
        let mut b = Element::from_media(project.media_item("vid").unwrap(), seconds(4.0));
        b.id = "b".to_owned();
        b.duration = seconds(4.0);
        b.trim_start = seconds(2.0);
        b.trim_end = seconds(4.0);
        for element in [a, b] {
            apply(
                &mut project,
                &Command::InsertElement {
                    element,
                    placement: TrackPlacement::Auto,
                },
            )
            .unwrap();
        }
        assert_eq!(project.tracks.len(), 1, "adjacent clips share the main track");

        // Before the window: a alone.
        let plan = plan_frame(&project, seconds(3.0));
        assert!(matches!(plan.items.as_slice(), [FrameItemPlan::Layer(layer)] if layer.element_id == "a"));

        // Inside the window: one transition item, a past its end and b before its start.
        let plan = plan_frame(&project, seconds(4.25));
        let [FrameItemPlan::Transition(transition)] = plan.items.as_slice() else {
            panic!("expected one transition, got {:?}", plan.items);
        };
        assert_eq!(transition.kind, "DirectionalWipe");
        assert_eq!(transition.progress, 0.75);
        let from = transition.from.as_ref().unwrap();
        let to = transition.to.as_ref().unwrap();
        assert_eq!(from.element_id, "a");
        assert_eq!(to.element_id, "b");
        assert_eq!(from.source, LayerSource::Video { media_id: "vid".to_owned(), time: seconds(4.25) });
        assert_eq!(to.source, LayerSource::Video { media_id: "vid".to_owned(), time: seconds(2.25) });

        // After the window: b alone.
        let plan = plan_frame(&project, seconds(4.5));
        assert!(matches!(plan.items.as_slice(), [FrameItemPlan::Layer(layer)] if layer.element_id == "b"));

        // A handle that runs out holds the edge frame: shrink b's lead-in to nothing.
        for track in &mut project.tracks {
            for element in &mut track.elements {
                if element.id == "b" {
                    element.trim_start = seconds(0.0);
                }
            }
        }
        let plan = plan_frame(&project, seconds(3.6));
        let [FrameItemPlan::Transition(transition)] = plan.items.as_slice() else { panic!() };
        assert_eq!(transition.to.as_ref().unwrap().source, LayerSource::Video { media_id: "vid".to_owned(), time: seconds(0.0) });

        // Remove b: the transition fades a out over its last second.
        apply(&mut project, &Command::DeleteElements(vec!["b".to_owned()])).unwrap();
        let plan = plan_frame(&project, seconds(3.5));
        let [FrameItemPlan::Transition(transition)] = plan.items.as_slice() else { panic!() };
        assert_eq!(transition.progress, 0.5);
        assert!(transition.to.is_none());
        assert_eq!(plan_frame(&project, seconds(2.5)).layers().len(), 1);
    }
}
