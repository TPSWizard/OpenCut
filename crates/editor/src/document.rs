//! The project document.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::{FrameRate, MediaTime};

use crate::animation::{Animations, resolve_object_params, resolve_params};
use crate::params::{Params, audio_defaults, text_defaults, visual_defaults};

pub const PROJECT_FORMAT_VERSION: u32 = 1;

/// Milliseconds since the Unix epoch.
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
}

impl CanvasSize {
    pub const HD_1080: Self = Self {
        width: 1920,
        height: 1080,
    };

    pub fn aspect_ratio(self) -> f64 {
        self.width as f64 / self.height.max(1) as f64
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Background {
    Color { color: String },
    Blur { blur_intensity: f64 },
}

impl Default for Background {
    fn default() -> Self {
        Self::Color {
            color: "#000000".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub fps: FrameRate,
    pub canvas: CanvasSize,
    #[serde(default)]
    pub background: Background,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            fps: FrameRate::FPS_30,
            canvas: CanvasSize::HD_1080,
            background: Background::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}

/// A file the project refers to. Media is referenced by path, never copied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub kind: MediaKind,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<MediaTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(default)]
    pub has_audio: bool,
}

impl MediaItem {
    pub fn is_visual(&self) -> bool {
        matches!(self.kind, MediaKind::Video | MediaKind::Image)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackKind {
    Video,
    Text,
    Audio,
}

impl TrackKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Text => "text",
            Self::Audio => "audio",
        }
    }
}

/// Where a track sits: overlays stack above the main video track, audio
/// tracks sit below it. Tracks are stored top to bottom in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lane {
    Overlay,
    Main,
    Audio,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub lane: Lane,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub elements: Vec<Element>,
}

impl Track {
    pub fn new(kind: TrackKind, lane: Lane, name: impl Into<String>) -> Self {
        Self {
            id: crate::new_id(),
            name: name.into(),
            kind,
            lane,
            muted: false,
            hidden: false,
            elements: Vec::new(),
        }
    }

    pub fn accepts(&self, kind: &ElementKind) -> bool {
        matches!(
            (self.kind, kind),
            (TrackKind::Video, ElementKind::Video { .. })
                | (TrackKind::Video, ElementKind::Image { .. })
                | (TrackKind::Text, ElementKind::Text)
                | (TrackKind::Audio, ElementKind::Audio { .. })
        )
    }

    /// Whether `[start, end)` is free of elements other than `ignore`.
    pub fn is_free(&self, start: MediaTime, end: MediaTime, ignore: Option<&str>) -> bool {
        self.elements.iter().all(|element| {
            ignore == Some(element.id.as_str())
                || end <= element.start_time
                || start >= element.end_time()
        })
    }

    pub fn end_time(&self) -> MediaTime {
        self.elements
            .iter()
            .map(Element::end_time)
            .max()
            .unwrap_or(MediaTime::ZERO)
    }

    pub fn sort_elements(&mut self) {
        self.elements.sort_by_key(|element| element.start_time);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ElementKind {
    Video { media_id: String },
    Image { media_id: String },
    Audio { media_id: String },
    Text,
}

impl ElementKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Video { .. } => "video",
            Self::Image { .. } => "image",
            Self::Audio { .. } => "audio",
            Self::Text => "text",
        }
    }

    pub fn media_id(&self) -> Option<&str> {
        match self {
            Self::Video { media_id } | Self::Image { media_id } | Self::Audio { media_id } => {
                Some(media_id)
            }
            Self::Text => None,
        }
    }

    pub fn track_kind(&self) -> TrackKind {
        match self {
            Self::Video { .. } | Self::Image { .. } => TrackKind::Video,
            Self::Text => TrackKind::Text,
            Self::Audio { .. } => TrackKind::Audio,
        }
    }

    pub fn is_visual(&self) -> bool {
        !matches!(self, Self::Audio { .. })
    }
}

fn yes() -> bool {
    true
}

/// A GPU effect on a visual element. `kind` names it (`blur`); `params`
/// are its settings with the web app's keys (`intensity`, 0–100, for
/// blur). Effects apply in order, before the element's mask.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub params: Params,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Effect {
    pub const BLUR: &'static str = "blur";

    /// A blur at the web app's default strength.
    pub fn blur() -> Self {
        let mut params = Params::new();
        params.set("intensity", 15.0);
        Self {
            id: crate::new_id(),
            kind: Self::BLUR.to_owned(),
            params,
            enabled: true,
        }
    }

    pub fn label(&self) -> &str {
        match crate::effect_catalogue::effect_spec(&self.kind) {
            Some(spec) => spec.label,
            None => self.kind.as_str(),
        }
    }
}

/// A shape that limits where a video or image element shows. `centerX`
/// and `centerY` are offsets from the element's centre and `width` and
/// `height` its size, all as fractions of the element's own size;
/// `rotation` is in degrees, `scale` a multiplier, `feather` the softness
/// of the edge in canvas pixels, and `inverted` shows the outside instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mask {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub params: Params,
}

/// Which size parameters a mask shape uses (the web app's `sizeMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSize {
    /// The shape has no size of its own (a split line).
    None,
    /// Only `height` applies; the shape spans the element's width.
    HeightOnly,
    WidthHeight,
}

impl Mask {
    pub const RECTANGLE: &'static str = "rectangle";
    pub const ELLIPSE: &'static str = "ellipse";
    pub const DIAMOND: &'static str = "diamond";
    pub const HEART: &'static str = "heart";
    pub const STAR: &'static str = "star";
    /// Shows one side of a line through the element.
    pub const SPLIT: &'static str = "split";
    /// A band across the whole element (letterbox when inverted).
    pub const CINEMATIC_BARS: &'static str = "cinematic-bars";
    /// A closed Bézier path drawn with the web app's pen tool: `path` is a
    /// list of `{x, y, inX, inY, outX, outY}` points in units of the
    /// element's size, `closed` says whether it is filled.
    pub const FREEFORM: &'static str = "freeform";
    /// The glyphs of `content`, sized like a text element but against the
    /// element's own height, centred, then scaled and rotated.
    pub const TEXT: &'static str = "text";

    /// The shapes that can be added and rendered natively, in menu order.
    pub const SHAPES: &'static [&'static str] = &[
        Self::RECTANGLE,
        Self::ELLIPSE,
        Self::DIAMOND,
        Self::HEART,
        Self::STAR,
        Self::SPLIT,
        Self::CINEMATIC_BARS,
        Self::TEXT,
    ];

    pub fn rectangle() -> Self {
        Self::shape(Self::RECTANGLE)
    }

    pub fn ellipse() -> Self {
        Self::shape(Self::ELLIPSE)
    }

    /// A mask of the given shape with the web app's defaults: centred, and
    /// covering 60% of the element where the shape has a size. Cinematic
    /// bars start wide enough to span the element at any rotation.
    pub fn shape(kind: &str) -> Self {
        let mut params = Params::new();
        params
            .set("centerX", 0.0)
            .set("centerY", 0.0)
            .set("rotation", 0.0)
            .set("feather", 0.0)
            .set("inverted", false)
            .set("strokeColor", "#ffffff")
            .set("strokeWidth", 0.0)
            .set("strokeAlign", "center");
        match kind {
            Self::SPLIT => {}
            Self::TEXT => {
                params
                    .set("content", "Mask")
                    .set("fontSize", 15.0)
                    .set("fontFamily", "Arial")
                    .set("fontWeight", "normal")
                    .set("fontStyle", "normal")
                    .set("textDecoration", "none")
                    .set("letterSpacing", 0.0)
                    .set("lineHeight", 1.2)
                    .set("scale", 1.0);
            }
            Self::FREEFORM => {
                params
                    .set("path", serde_json::Value::Array(Vec::new()))
                    .set("closed", false)
                    .set("scale", 1.0);
            }
            _ => {
                let width = if kind == Self::CINEMATIC_BARS { std::f64::consts::SQRT_2 } else { 0.6 };
                params.set("width", width).set("height", 0.6).set("scale", 1.0);
            }
        }
        Self {
            id: crate::new_id(),
            kind: kind.to_owned(),
            params,
        }
    }

    pub fn label(&self) -> &str {
        match self.kind.as_str() {
            Self::RECTANGLE => "Rectangle",
            Self::ELLIPSE => "Ellipse",
            Self::DIAMOND => "Diamond",
            Self::HEART => "Heart",
            Self::STAR => "Star",
            Self::SPLIT => "Split",
            Self::CINEMATIC_BARS => "Cinematic bars",
            Self::FREEFORM => "Freeform",
            Self::TEXT => "Text",
            other => other,
        }
    }

    pub fn size_mode(&self) -> MaskSize {
        match self.kind.as_str() {
            Self::SPLIT | Self::FREEFORM | Self::TEXT => MaskSize::None,
            Self::CINEMATIC_BARS => MaskSize::HeightOnly,
            _ => MaskSize::WidthHeight,
        }
    }
}

/// A transition out of a clip into the next one on its track, centred on
/// the cut (see [`crate::transition`]). `kind` names a gl-transitions
/// shader (`Fade`, `DirectionalWipe`, …); `params` are its uniforms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    #[serde(rename = "type")]
    pub kind: String,
    pub duration: MediaTime,
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub params: Params,
}

/// One spoken word of a caption: `start` and `end` are relative to the
/// element's start. The words are the whitespace-separated tokens of the
/// element's `content`, in order, as the web app stores them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionWord {
    pub text: String,
    pub start: MediaTime,
    pub end: MediaTime,
}

/// A clip on the timeline. `duration` is what is visible; `trim_start` and
/// `trim_end` are how much of the source is cut off at each end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: ElementKind,
    pub start_time: MediaTime,
    pub duration: MediaTime,
    #[serde(default = "zero")]
    pub trim_start: MediaTime,
    #[serde(default = "zero")]
    pub trim_end: MediaTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_duration: Option<MediaTime>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub params: Params,
    /// Keyframes by parameter path; see [`crate::animation`].
    #[serde(default, skip_serializing_if = "Animations::is_empty")]
    pub animations: Animations,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    /// Only the first mask is applied, as in the web app.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masks: Vec<Mask>,
    /// The transition this clip leads out of, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
    /// A caption's timed words (text elements); see [`CaptionWord`]. The
    /// `caption.*` params say how the spoken word is highlighted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<CaptionWord>,
}

fn zero() -> MediaTime {
    MediaTime::ZERO
}

/// Default length of a new image or text element.
pub const DEFAULT_STILL_DURATION_SECONDS: f64 = 5.0;

impl Element {
    /// An element for a media item, covering the whole source.
    pub fn from_media(media: &MediaItem, start_time: MediaTime) -> Self {
        let still = MediaTime::from_seconds_f64(DEFAULT_STILL_DURATION_SECONDS).unwrap();
        let (kind, duration, source_duration, params) = match media.kind {
            MediaKind::Video => {
                let duration = media.duration.unwrap_or(still);
                let mut params = visual_defaults();
                if media.has_audio {
                    params.merge(&audio_defaults());
                }
                (
                    ElementKind::Video {
                        media_id: media.id.clone(),
                    },
                    duration,
                    Some(duration),
                    params,
                )
            }
            MediaKind::Image => (
                ElementKind::Image {
                    media_id: media.id.clone(),
                },
                still,
                None,
                visual_defaults(),
            ),
            MediaKind::Audio => {
                let duration = media.duration.unwrap_or(still);
                (
                    ElementKind::Audio {
                        media_id: media.id.clone(),
                    },
                    duration,
                    Some(duration),
                    audio_defaults(),
                )
            }
        };
        Self {
            id: crate::new_id(),
            name: media.name.clone(),
            kind,
            start_time,
            duration,
            trim_start: MediaTime::ZERO,
            trim_end: MediaTime::ZERO,
            source_duration,
            hidden: false,
            params,
            animations: Animations::new(),
            effects: Vec::new(),
            masks: Vec::new(),
            transition: None,
            words: Vec::new(),
        }
    }

    pub fn text(content: &str, start_time: MediaTime) -> Self {
        let mut params = text_defaults();
        params.set("content", content);
        Self {
            id: crate::new_id(),
            name: "Text".to_owned(),
            kind: ElementKind::Text,
            start_time,
            duration: MediaTime::from_seconds_f64(DEFAULT_STILL_DURATION_SECONDS).unwrap(),
            trim_start: MediaTime::ZERO,
            trim_end: MediaTime::ZERO,
            source_duration: None,
            hidden: false,
            params,
            animations: Animations::new(),
            effects: Vec::new(),
            masks: Vec::new(),
            transition: None,
            words: Vec::new(),
        }
    }

    /// A timeline time relative to this element's start.
    pub fn local_time(&self, timeline_time: MediaTime) -> MediaTime {
        timeline_time - self.start_time
    }

    /// The parameters with every keyframed one evaluated at a timeline time.
    pub fn params_at(&self, timeline_time: MediaTime) -> Params {
        if self.animations.is_empty() {
            return self.params.clone();
        }
        resolve_params(&self.params, &self.animations, self.local_time(timeline_time))
    }

    pub fn is_animated(&self, path: &str) -> bool {
        self.animations
            .get(path)
            .is_some_and(|channel| !channel.is_empty())
    }

    pub fn effect(&self, effect_id: &str) -> Option<&Effect> {
        self.effects.iter().find(|effect| effect.id == effect_id)
    }

    pub fn mask(&self, mask_id: &str) -> Option<&Mask> {
        self.masks.iter().find(|mask| mask.id == mask_id)
    }

    /// An effect's parameters with keyframes (`effects.<id>.params.<key>`)
    /// evaluated at a timeline time.
    pub fn effect_params_at(&self, effect: &Effect, timeline_time: MediaTime) -> Params {
        resolve_object_params(
            &effect.params,
            &self.animations,
            "effects",
            &effect.id,
            self.local_time(timeline_time),
        )
    }

    /// A mask's parameters with keyframes (`masks.<id>.params.<key>`)
    /// evaluated at a timeline time.
    pub fn mask_params_at(&self, mask: &Mask, timeline_time: MediaTime) -> Params {
        resolve_object_params(
            &mask.params,
            &self.animations,
            "masks",
            &mask.id,
            self.local_time(timeline_time),
        )
    }

    pub fn can_be_masked(&self) -> bool {
        matches!(self.kind, ElementKind::Video { .. } | ElementKind::Image { .. })
    }

    pub fn end_time(&self) -> MediaTime {
        self.start_time + self.duration
    }

    pub fn contains(&self, time: MediaTime) -> bool {
        time >= self.start_time && time < self.end_time()
    }

    /// Where in the source a timeline time falls.
    pub fn source_time(&self, timeline_time: MediaTime) -> MediaTime {
        self.trim_start + (timeline_time - self.start_time)
    }

    pub fn media_id(&self) -> Option<&str> {
        self.kind.media_id()
    }

    /// The longest this element can be made by extending its end, given its source.
    pub fn max_duration(&self) -> Option<MediaTime> {
        self.source_duration
            .map(|source| (source - self.trim_start).max(MediaTime::ONE_TICK))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub settings: ProjectSettings,
    #[serde(default)]
    pub media: Vec<MediaItem>,
    pub tracks: Vec<Track>,
    #[serde(default = "zero")]
    pub playhead: MediaTime,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        let now = now_millis();
        Self {
            version: PROJECT_FORMAT_VERSION,
            id: crate::new_id(),
            name: name.into(),
            created_at: now,
            updated_at: now,
            settings: ProjectSettings::default(),
            media: Vec::new(),
            tracks: vec![Track::new(TrackKind::Video, Lane::Main, "Main")],
            playhead: MediaTime::ZERO,
        }
    }

    pub fn duration(&self) -> MediaTime {
        self.tracks
            .iter()
            .map(Track::end_time)
            .max()
            .unwrap_or(MediaTime::ZERO)
    }

    pub fn main_track_index(&self) -> usize {
        self.tracks
            .iter()
            .position(|track| track.lane == Lane::Main)
            .unwrap_or(0)
    }

    pub fn track(&self, track_id: &str) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == track_id)
    }

    pub fn track_mut(&mut self, track_id: &str) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|track| track.id == track_id)
    }

    pub fn track_index(&self, track_id: &str) -> Option<usize> {
        self.tracks.iter().position(|track| track.id == track_id)
    }

    pub fn media_item(&self, media_id: &str) -> Option<&MediaItem> {
        self.media.iter().find(|item| item.id == media_id)
    }

    /// Finds an element and the track it is on.
    pub fn locate(&self, element_id: &str) -> Option<(usize, usize)> {
        self.tracks.iter().enumerate().find_map(|(track_index, track)| {
            track
                .elements
                .iter()
                .position(|element| element.id == element_id)
                .map(|element_index| (track_index, element_index))
        })
    }

    pub fn element(&self, element_id: &str) -> Option<(&Track, &Element)> {
        let (track_index, element_index) = self.locate(element_id)?;
        let track = &self.tracks[track_index];
        Some((track, &track.elements[element_index]))
    }

    pub fn elements(&self) -> impl Iterator<Item = (&Track, &Element)> {
        self.tracks
            .iter()
            .flat_map(|track| track.elements.iter().map(move |element| (track, element)))
    }

    pub fn media_for(&self, element: &Element) -> Option<&MediaItem> {
        element.media_id().and_then(|id| self.media_item(id))
    }

    pub fn touch(&mut self) {
        self.updated_at = now_millis();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_projects_have_one_main_track() {
        let project = Project::new("Demo");
        assert_eq!(project.tracks.len(), 1);
        assert_eq!(project.tracks[0].lane, Lane::Main);
        assert_eq!(project.main_track_index(), 0);
        assert_eq!(project.duration(), MediaTime::ZERO);
    }

    #[test]
    fn project_json_round_trips() {
        let mut project = Project::new("Round trip");
        project.tracks[0]
            .elements
            .push(Element::text("Hello", MediaTime::from_seconds_f64(1.0).unwrap()));
        project.media.push(MediaItem {
            id: "m".to_owned(),
            name: "m.mp4".to_owned(),
            kind: MediaKind::Video,
            path: "m.mp4".into(),
            duration: Some(MediaTime::from_seconds_f64(2.0).unwrap()),
            width: Some(16),
            height: Some(9),
            fps: Some(30.0),
            has_audio: true,
        });
        project.tracks[0].elements.push(Element::from_media(&project.media[0], MediaTime::ZERO));
        project.settings.background = Background::Blur { blur_intensity: 3.0 };
        let json = serde_json::to_string_pretty(&project).unwrap();
        assert!(json.contains("\"type\": \"text\""));
        assert!(json.contains("\"mediaId\": \"m\""), "fields inside variants are camelCase");
        assert!(json.contains("\"blurIntensity\": 3.0"));
        assert!(json.contains("\"startTime\": 0"), "MediaTime is a plain tick count");
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, project);
    }

    #[test]
    fn source_time_accounts_for_trim() {
        let mut element = Element::text("x", MediaTime::from_ticks(1000));
        element.trim_start = MediaTime::from_ticks(50);
        assert_eq!(
            element.source_time(MediaTime::from_ticks(1200)),
            MediaTime::from_ticks(250)
        );
    }
}
