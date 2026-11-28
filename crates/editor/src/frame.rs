//! What is on screen at a point in time, independent of how it is drawn.

use time::MediaTime;

use crate::document::{Background, CaptionWord, Element, ElementKind, Project, Track};
use crate::params::Params;
use crate::transition::{in_window, next_adjacent, transition_window, window_into};

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
    /// Rounded corners, outline and drop shadow.
    pub style: crate::LayerStyle,
    /// The clip's corners pinned in perspective (video, picture and sticker
    /// clips), in box units: top left, top right, bottom right, bottom left.
    pub corner_pin: Option<[[f32; 2]; 4]>,
    /// An auto zoom punch-in at this time (video and picture clips); the
    /// renderer scales the drawn frame about the focus point.
    pub zoom: Option<LayerZoom>,
    /// Motion blur: the clip's transform half a shutter before and after
    /// the planned time (`motionBlur` param); the renderer draws the clip
    /// along the way between them.
    pub motion: Option<(LayerTransform, LayerTransform)>,
    /// Deflicker: the frame's brightness gain (video clips with
    /// `deflicker.strength` on and measured levels), applied before effects.
    pub brightness_gain: Option<f32>,
    /// Stabilise (row 107): the frame's correction at its source time
    /// (video clips with `stabilise.strength` on and a measured camera
    /// path); the renderer moves the drawn frame by it (`steady_transform`).
    pub steady: Option<stabilise::Similarity>,
    /// The faces in the drawn frame (video and picture clips with a face
    /// track from this media, `facetrack`), as shares of the source frame;
    /// effects that work on faces only (Retouch) read them.
    pub faces: Option<Vec<facetrack::FaceBox>>,
    /// A text clip's credit roll or crawl (`credit_roll`, row 98) and the
    /// clip's length in seconds; the renderer sets the position along the
    /// roll from the text's drawn size.
    pub roll: Option<(crate::CreditRoll, f64)>,
}

/// An auto zoom at the planned time: `scale` (above 1) about the focus
/// point, a share of the clip's frame from its centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerZoom {
    pub scale: f32,
    pub focus_x: f32,
    pub focus_y: f32,
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
        /// Time within the source, which animated images (Lottie) use;
        /// still images ignore it.
        time: MediaTime,
    },
    Text(TextStyle),
    /// A sticker by id, with the intrinsic size the web app stored (SVG
    /// size when none).
    Sticker {
        sticker_id: String,
        intrinsic_width: Option<f64>,
        intrinsic_height: Option<f64>,
    },
    /// A freehand drawing over the whole canvas (`strokes` as stored), drawn
    /// on up to `progress` (0 to 1; see `strokes::reveal_progress`).
    Drawing { strokes: String, progress: f64 },
    /// An audiogram over the whole canvas (row 100): its `levels` as
    /// stored, its look, and the seconds into the element.
    Audiogram { levels: String, look: crate::AudiogramLook, seconds: f64 },
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

/// The keywords of a caption (words flagged `keyword`) and how they look:
/// the keyword colour (the spoken-word highlight still wins) and a size
/// relative to the rest of the text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextKeywords {
    pub indices: Vec<usize>,
    pub color: [f32; 4],
    pub scale: f32,
}

/// The keyword look for a caption, or `None` when no word is a keyword.
pub fn caption_keywords(params: &Params, words: &[CaptionWord]) -> Option<TextKeywords> {
    let indices: Vec<usize> = words.iter().enumerate().filter(|(_, word)| word.keyword).map(|(index, _)| index).collect();
    if indices.is_empty() {
        return None;
    }
    let default_color = crate::KEYWORD_COLOR_DEFAULT;
    Some(TextKeywords {
        indices,
        color: parse_hex_color(&params.text("caption.keywordColor", default_color))
            .or_else(|| parse_hex_color(default_color))
            .unwrap_or([1.0; 4]),
        scale: params.number("caption.keywordScale", crate::KEYWORD_SCALE_DEFAULT).clamp(1.0, crate::KEYWORD_SCALE_MAX) as f32,
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
    /// Caption keywords; `None` when no word is one.
    pub keywords: Option<TextKeywords>,
    /// Characters a typewriter or word-by-word animation shows (newlines
    /// not counted), each where it sits in the whole text; `None` for all.
    pub reveal_chars: Option<usize>,
    /// Curved text: the `curve` param, -100 (a smile) to 100 (an arch); 0
    /// is flat. The renderer bends the raster with `textwarp`.
    pub curve: f32,
    /// Text on a path (`textPath`, `textPathAmount`, `textPathPoints`): the
    /// path's control points, which the renderer lays the raster along
    /// with `textwarp::warp_path` instead of the arc.
    pub path: Option<Vec<[f64; 2]>>,
    /// Rich text (`richText` param, `richtext` crate): each character's own
    /// colour, weight, slant and size, already moved with any edits to the
    /// text; `None` when nothing is styled.
    pub rich: Option<Vec<richtext::CharStyle>>,
    /// Word art (row 99, `wordart` crate): gradient fill, outlines and
    /// inner shadow on the glyphs; `None` for plain text.
    pub word_art: Option<wordart::WordArt>,
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
        // The web app's limits for a text clip's size (its Font Size control
        // goes down to 1; 5 is the floor for text masks only, row 98 found
        // the desktop clamping small text up to 5).
        let font_size_param = params.number("fontSize", 15.0).clamp(1.0, 300.0) as f32;
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
            keywords: None,
            reveal_chars: None,
            curve: {
                let curve = params.number("curve", 0.0);
                if curve.is_finite() { curve.clamp(-100.0, 100.0) as f32 } else { 0.0 }
            },
            rich: richtext::styles_for(&params.text(crate::RICH_TEXT_PARAM, ""), &params.text("content", "")),
            path: textwarp::read_path(
                &params.text("textPath", "none"),
                Some(params.number("textPathAmount", 50.0)),
                &params.text("textPathPoints", ""),
            ),
            word_art: read_word_art(params),
        }
    }
}

/// A text clip's word art (row 99) from its params; `None` for plain text.
pub fn read_word_art(params: &Params) -> Option<wordart::WordArt> {
    wordart::WordArt::from_lookup(
        |key| match params.get(key)? {
            crate::ParamValue::Text(text) => Some(text.clone()),
            _ => None,
        },
        |key| params.get(key).and_then(crate::ParamValue::as_number),
    )
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
    let fps = project.settings.fps;
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
                        time: element.source_time(time),
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
            // Its own in-transition: from nothing into the clip.
            if let Some(window) = in_window(track, element)
                && window.contains(time)
            {
                let transition = element.transition_in.as_ref().expect("has a window");
                items.push(FrameItemPlan::Transition(TransitionPlan {
                    element_id: element.id.clone(),
                    track_id: track.id.clone(),
                    kind: transition.kind.clone(),
                    progress: window.progress(time),
                    params: transition.params.clone(),
                    from: None,
                    to: layer_plan(track, element, time, (canvas.width as f32, canvas.height as f32), fps, false),
                }));
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
                    from: layer_plan(track, element, time, (canvas.width as f32, canvas.height as f32), fps, true),
                    to: next.and_then(|next| layer_plan(track, next, time, (canvas.width as f32, canvas.height as f32), fps, true)),
                }));
                continue;
            }
            if !element.contains(time) {
                continue;
            }
            if let Some(layer) = layer_plan(track, element, time, (canvas.width as f32, canvas.height as f32), fps, false) {
                // Background blur: the whole clip blurred underneath the sharp cut-out.
                if let Some(background) = background_blur_layer(element, &layer) {
                    items.push(FrameItemPlan::Layer(background));
                }
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

/// The layer drawn under a cut-out with a background blur (portrait mode):
/// the same clip whole, without its cut-out, through the shared Blur effect
/// at the cut-out's `background_blur` intensity after its own effects. The
/// sharp cut-out is drawn over it. `None` without a background blur. The
/// web editor's scene builder adds the same node.
pub fn background_blur_layer(element: &Element, layer: &LayerPlan) -> Option<LayerPlan> {
    let intensity = element.cutout.as_ref()?.background_blur;
    if !(intensity.is_finite() && intensity > 0.0) || layer.cutout.is_none() {
        return None;
    }
    let mut background = layer.clone();
    background.element_id = format!("{}#background", layer.element_id);
    background.cutout = None;
    let mut params = crate::Params::new();
    params.set("intensity", intensity.min(100.0));
    background.effects.push(EffectPlan { id: "background-blur".to_owned(), kind: "blur".to_owned(), params });
    Some(background)
}

/// A visual element as a layer at `time`. With `extended`, the time may fall
/// outside the clip (a transition's handles): the source keeps playing from
/// the material beyond the trim and holds its edge frame when there is none,
/// while keyframes hold their edge values.
/// A layer plan for one element at `time`, for tests elsewhere in the crate.
#[cfg(test)]
pub(crate) fn layer_plan_for_test(project: &Project, element: &Element, time: MediaTime) -> Option<LayerPlan> {
    let track = project.tracks.iter().find(|track| track.elements.iter().any(|candidate| candidate.id == element.id))?;
    let canvas = project.settings.canvas;
    layer_plan(track, element, time, (canvas.width as f32, canvas.height as f32), project.settings.fps, false)
}

fn layer_plan(
    track: &Track,
    element: &Element,
    time: MediaTime,
    (canvas_width, canvas_height): (f32, f32),
    fps: time::FrameRate,
    extended: bool,
) -> Option<LayerPlan> {
    let param_time = if extended {
        time.clamp(element.start_time, element.end_time() - MediaTime::ONE_TICK)
    } else {
        time
    };
    let params = element.params_at(param_time);
    // One-click in, out and loop animations, on top of the keyframes.
    let animation = crate::read_clip_animations(&element.params).map(|animations| {
        crate::clip_animation_state_at(
            &animations,
            element.local_time(param_time).to_seconds_f64(),
            element.duration.to_seconds_f64(),
            (canvas_width as f64, canvas_height as f64),
            matches!(element.kind, ElementKind::Text),
        )
    });
    let source = match &element.kind {
        ElementKind::Video { media_id } => LayerSource::Video {
            media_id: media_id.clone(),
            time: if extended { extended_source_time(element, time) } else { element.source_time(time) },
        },
        ElementKind::Image { media_id } => LayerSource::Image {
            media_id: media_id.clone(),
            time: if extended {
                element.source_time(time.clamp(element.start_time, element.end_time() - MediaTime::ONE_TICK))
            } else {
                element.source_time(time)
            },
        },
        ElementKind::Sticker { sticker_id, intrinsic_width, intrinsic_height } => LayerSource::Sticker {
            sticker_id: sticker_id.clone(),
            intrinsic_width: *intrinsic_width,
            intrinsic_height: *intrinsic_height,
        },
        ElementKind::Text => {
            let mut style = TextStyle::from_params(&params, canvas_height);
            style.highlight = caption_highlight(&params, &element.words, element.local_time(param_time));
            style.keywords = caption_keywords(&params, &element.words);
            style.reveal_chars = animation.and_then(|state| crate::revealed_char_count(&style.content, state.reveal));
            LayerSource::Text(style)
        }
        ElementKind::Graphic { definition_id } if definition_id == crate::DRAWING_DEFINITION => LayerSource::Drawing {
            strokes: params.text("strokes", "").into_owned(),
            progress: strokes::reveal_progress(params.number("drawOn", 0.0), element.local_time(param_time).to_seconds_f64()),
        },
        ElementKind::Graphic { definition_id } if definition_id == crate::AUDIOGRAM_DEFINITION => LayerSource::Audiogram {
            levels: params.text("levels", "").into_owned(),
            look: crate::read_audiogram_look(&params),
            seconds: element.local_time(param_time).to_seconds_f64(),
        },
        // Compounds are expanded before planning; audio and shapes the
        // desktop does not draw yet draw nothing.
        ElementKind::Audio { .. } | ElementKind::Compound { .. } | ElementKind::Graphic { .. } => return None,
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
        .map(|mask| {
            let mut params = element.mask_params_at(mask, param_time);
            // A tracked mask (row 92) moves by its stored track.
            let track = params.text(tracking::MASK_TRACK_PARAM, "").into_owned();
            if !track.is_empty() {
                let (dx, dy) = tracking::mask_offset(&track, element.local_time(param_time).to_seconds_f64());
                let (x, y) = (params.number("centerX", 0.0), params.number("centerY", 0.0));
                params.set("centerX", x + dx);
                params.set("centerY", y + dy);
            }
            MaskPlan { id: mask.id.clone(), kind: mask.kind.clone(), params }
        });
    let cutout = match (&source, &element.cutout) {
        (LayerSource::Video { time, .. }, Some(cutout)) => Some(CutoutPlan {
            matte_id: cutout.matte_id.clone(),
            time: (time.to_seconds_f64() - cutout.offset).max(0.0),
            inverted: cutout.inverted,
        }),
        _ => None,
    };
    let mut transform = LayerTransform::from_params(&params);
    let mut opacity = params.number("opacity", 1.0).clamp(0.0, 1.0) as f32;
    if let Some(state) = animation {
        state.apply_to(&mut transform);
        opacity = (opacity * state.opacity as f32).clamp(0.0, 1.0);
    }
    // Motion blur: the same transform half a shutter either side.
    let motion = crate::motion_blur_half_shutter(crate::read_motion_blur(&element.params), fps)
        .and_then(MediaTime::from_seconds_f64)
        .filter(|half| *half > MediaTime::ZERO)
        .map(|half| {
            let at = |time: MediaTime| transform_at(element, time, (canvas_width, canvas_height));
            (at(param_time - half), at(param_time + half))
        })
        .filter(|(open, close)| open != close);
    // Faces: the clip's face track at this source time.
    let faces = match &source {
        LayerSource::Video { media_id, time } | LayerSource::Image { media_id, time } => {
            facetrack::FaceTrack::parse(&element.params.text(facetrack::FACE_TRACK_PARAM, ""))
                .filter(|track| &track.media == media_id)
                .map(|track| track.faces_at(time.to_seconds_f64()))
        }
        _ => None,
    };
    // Deflicker: the measured frame's gain at this source time.
    let brightness_gain = match &source {
        LayerSource::Video { media_id, time } => {
            crate::deflicker_gain(&element.params, media_id, time.to_seconds_f64()).map(|gain| gain as f32)
        }
        _ => None,
    };
    // Stabilise: the measured camera path's correction at this source time.
    let steady = match &source {
        LayerSource::Video { media_id, time } => crate::stabilise_correction(&element.params, media_id, time.to_seconds_f64()),
        _ => None,
    };
    Some(LayerPlan {
        element_id: element.id.clone(),
        track_id: track.id.clone(),
        source,
        transform,
        opacity,
        blend_mode: params.text("blendMode", "normal").into_owned(),
        effects,
        local_time: element.local_time(param_time),
        mask,
        cutout,
        style: crate::LayerStyle::from_params(&params, matches!(element.kind, ElementKind::Text)),
        corner_pin: matches!(element.kind, ElementKind::Video { .. } | ElementKind::Image { .. } | ElementKind::Sticker { .. })
            .then(|| crate::read_corner_pin(&params))
            .flatten(),
        zoom: matches!(element.kind, ElementKind::Video { .. } | ElementKind::Image { .. })
            .then(|| crate::read_clip_zoom(&element.params))
            .flatten()
            .and_then(|zoom| {
                let scale = crate::zoom_at(&zoom, element.local_time(param_time).to_seconds_f64());
                ((scale - 1.0).abs() > 1e-6)
                    .then_some(LayerZoom { scale: scale as f32, focus_x: zoom.focus_x as f32, focus_y: zoom.focus_y as f32 })
            }),
        motion,
        brightness_gain,
        steady,
        faces,
        roll: matches!(element.kind, ElementKind::Text)
            .then(|| crate::read_credit_roll(&params))
            .flatten()
            .map(|roll| (roll, element.duration.to_seconds_f64())),
    })
}

impl crate::clip_animation::ClipAnimationState {
    /// Adds the animation's movement to a keyframed transform.
    fn apply_to(&self, transform: &mut LayerTransform) {
        transform.position_x += self.offset_x as f32;
        transform.position_y += self.offset_y as f32;
        transform.scale_x *= self.scale as f32;
        transform.scale_y *= self.scale as f32;
        transform.rotate_degrees += self.rotate as f32;
    }
}

/// The element's transform at a timeline time: its keyframes and its
/// one-click animations (for motion blur, which looks either side of the
/// planned time; keyframes hold their edge values beyond the clip).
fn transform_at(element: &Element, time: MediaTime, (canvas_width, canvas_height): (f32, f32)) -> LayerTransform {
    let mut transform = LayerTransform::from_params(&element.params_at(time));
    if let Some(animations) = crate::read_clip_animations(&element.params) {
        crate::clip_animation_state_at(
            &animations,
            element.local_time(time).to_seconds_f64(),
            element.duration.to_seconds_f64(),
            (canvas_width as f64, canvas_height as f64),
            matches!(element.kind, ElementKind::Text),
        )
        .apply_to(&mut transform);
    }
    transform
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
    fn text_styles_read_word_art() {
        let mut params = Params::new();
        params.set("content", "Hi");
        assert_eq!(TextStyle::from_params(&params, 1080.0).word_art, None);
        params.set("wordArt.fill", "gradient").set("wordArt.gradientFrom", "#ff0000").set("wordArt.stroke2Width", 8.0);
        let art = TextStyle::from_params(&params, 1080.0).word_art.expect("word art");
        assert_eq!(art.fill.unwrap().from, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(art.strokes.len(), 1);
        assert_eq!(art.strokes[0].width, 8.0);
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
    fn lottie_clips_run_for_one_loop_and_plan_their_source_time() {
        let mut project = Project::new("Lottie");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "anim".to_owned(),
                name: "bounce.json".to_owned(),
                kind: MediaKind::Lottie,
                path: "bounce.json".into(),
                duration: Some(seconds(2.0)),
                width: Some(200),
                height: Some(200),
                fps: Some(30.0),
                has_audio: false,
            }),
        )
        .unwrap();
        let mut clip = Element::from_media(project.media_item("anim").unwrap(), seconds(1.0));
        assert!(matches!(&clip.kind, ElementKind::Image { media_id } if media_id == "anim"));
        assert_eq!(clip.duration, seconds(2.0), "one run of the animation");
        clip.trim_start = seconds(0.5);
        apply(&mut project, &Command::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();
        let plan = plan_frame(&project, seconds(1.25));
        assert!(
            matches!(&plan.layers()[0].source, LayerSource::Image { media_id, time } if media_id == "anim" && *time == seconds(0.75)),
            "{:?}",
            plan.layers()[0].source
        );
        assert!(project.media_item("anim").unwrap().is_visual());
    }

    #[test]
    fn auto_zoom_punches_in_only_inside_its_spans() {
        let mut project = Project::new("Zoom");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "talk".to_owned(),
                name: "talk.mp4".to_owned(),
                kind: MediaKind::Video,
                path: "talk.mp4".into(),
                duration: Some(seconds(10.0)),
                width: Some(1920),
                height: Some(1080),
                fps: Some(30.0),
                has_audio: true,
            }),
        )
        .unwrap();
        let mut clip = Element::from_media(project.media_item("talk").unwrap(), MediaTime::ZERO);
        clip.params.set("zoom.punches", "2-4@1.3").set("zoom.ramp", 0.0).set("zoom.focusX", 0.2);
        apply(&mut project, &Command::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();
        assert_eq!(plan_frame(&project, seconds(1.0)).layers()[0].zoom, None);
        let zoom = plan_frame(&project, seconds(3.0)).layers()[0].zoom.expect("zoomed");
        assert_eq!((zoom.scale, zoom.focus_x, zoom.focus_y), (1.3, 0.2, -0.1));
        assert_eq!(plan_frame(&project, seconds(4.5)).layers()[0].zoom, None);
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
        assert!(
            matches!(&plan.backdrops[0].source, LayerSource::Image { media_id, .. } if media_id == "img"),
            "{:?}",
            plan.backdrops[0].source
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
    fn animation_presets_move_fade_and_reveal_layers() {
        let mut project = Project::new("Animations");
        let mut text = Element::text("one two", seconds(1.0));
        text.params
            .set("transform.positionY", 10.0)
            .set("animIn", "typewriter")
            .set("animInDuration", 1.0)
            .set("animOut", "slideDown")
            .set("animOutDuration", 1.0);
        let length = text.duration.to_seconds_f64();
        apply(
            &mut project,
            &Command::InsertElement {
                element: text,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();

        let layer = |time: f64| plan_frame(&project, seconds(time)).layers()[0].clone();
        let LayerSource::Text(style) = layer(1.5).source else { panic!("expected text") };
        assert_eq!(style.reveal_chars, Some(4), "half of the seven characters, rounded up");
        let LayerSource::Text(style) = layer(2.5).source else { panic!("expected text") };
        assert_eq!(style.reveal_chars, None, "all of it after the way in");
        assert_eq!(layer(2.5).transform.position_y, 10.0);

        // The last moment slides down out of view by most of the slide (+y is down).
        let end = layer(1.0 + length - 0.01);
        assert!(end.transform.position_y > 10.0 + 0.9 * 0.3 * 1080.0);
        assert!(end.opacity < 0.05);
    }

    #[test]
    fn captions_highlight_the_spoken_word() {
        let mut project = Project::new("Captions");
        let mut text = Element::text("Hello there world", seconds(1.0));
        text.words = vec![
            CaptionWord { text: "Hello".into(), start: seconds(0.0), end: seconds(0.4), keyword: false },
            CaptionWord { text: "there".into(), start: seconds(0.5), end: seconds(0.9), keyword: false },
            CaptionWord { text: "world".into(), start: seconds(1.2), end: seconds(1.6), keyword: false },
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
    fn motion_blur_plans_the_transform_half_a_shutter_either_side() {
        let mut project = Project::new("Blur");
        let mut text = Element::text("Hi", seconds(1.0));
        for (at, x) in [(0.0, 0.0), (2.0, 300.0)] {
            text.animations
                .entry("transform.positionX".to_owned())
                .or_default()
                .upsert(seconds(at), x, None);
        }
        apply(&mut project, &Command::InsertElement { element: text, placement: TrackPlacement::Auto }).unwrap();
        let start = project.tracks[0].elements[0].start_time.to_seconds_f64();
        assert_eq!(plan_frame(&project, seconds(start + 1.0)).layers()[0].motion, None, "off by default");

        project.tracks[0].elements[0].params.set("motionBlur", 100.0);
        // 30 fps, a whole-frame shutter: 1/60 s either side, 2.5 px at 150 px/s.
        let plan = plan_frame(&project, seconds(start + 1.0));
        let layer = &plan.layers()[0];
        let (open, close) = layer.motion.expect("moving, so blurred");
        assert!((layer.transform.position_x - 150.0).abs() < 1e-3);
        assert!((open.position_x - 147.5).abs() < 1e-3, "{open:?}");
        assert!((close.position_x - 152.5).abs() < 1e-3, "{close:?}");
        assert_eq!(open.position_y, close.position_y);

        // Past the last key it holds still: nothing to blur.
        assert_eq!(plan_frame(&project, seconds(start + 2.5)).layers()[0].motion, None);
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
