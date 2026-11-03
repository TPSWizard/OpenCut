//! The project document.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::{FrameRate, MediaTime};

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
        }
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
