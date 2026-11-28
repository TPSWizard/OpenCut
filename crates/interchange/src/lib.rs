//! Project interchange (row 111): a timeline's tracks, clips, trims,
//! constant speed changes and dissolves, to and from three formats other
//! editors read and write:
//!
//! - **OpenTimelineIO** JSON (`.otio`, [`otio`]),
//! - **FCPXML** 1.10 (`.fcpxml`, [`fcpxml`]), Final Cut Pro's format, which
//!   DaVinci Resolve and Premiere also import,
//! - **CMX 3600 EDL** (`.edl`, [`edl`]): one video track and two audio
//!   tracks, the oldest and most widely read.
//!
//! Each editor maps its project to the neutral [`Timeline`] here (all times
//! in whole frames at the timeline's rate) and back. What a format cannot
//! carry (effects, text, keyframes, speed ramps, …) is left out, and every
//! kind of thing left out is listed in [`Timeline::notes`], which the
//! editors show and the writers put in the file as a comment where the
//! format allows.
//!
//! Format facts were taken from the OpenTimelineIO serialized schema and
//! timeline-structure pages, Apple's FCPXML 1.10 DTD, and EDL examples of
//! the CMX 3600 layout; checked against `fixtures/interchange/cases.json`,
//! written by an independent Python reference (stdlib `json` and
//! `xml.etree`). Not checked against the formats' own tools.

use serde::{Deserialize, Serialize};

pub mod edl;
pub mod fcpxml;
pub mod otio;
mod timecode;

pub use timecode::{frames_to_timecode, timecode_to_frames};

/// A frame rate as a fraction: 30000/1001 is 29.97.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rate {
    pub num: u32,
    pub den: u32,
}

impl Rate {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    pub fn fps(self) -> f64 {
        self.num as f64 / self.den.max(1) as f64
    }

    /// The nearest rate of the common ones (23.976 to 60) to `fps`, or `fps`
    /// in thousandths.
    pub fn from_fps(fps: f64) -> Self {
        const COMMON: [(u32, u32); 9] =
            [(24000, 1001), (24, 1), (25, 1), (30000, 1001), (30, 1), (48, 1), (50, 1), (60000, 1001), (60, 1)];
        if let Some(&(num, den)) = COMMON.iter().find(|(num, den)| (*num as f64 / *den as f64 - fps).abs() < 0.005) {
            return Self { num, den };
        }
        if !fps.is_finite() || fps <= 0.0 {
            return Self { num: 30, den: 1 };
        }
        if (fps - fps.round()).abs() < 1e-6 {
            return Self { num: fps.round() as u32, den: 1 };
        }
        Self { num: (fps * 1000.0).round() as u32, den: 1000 }
    }

    /// Whole frames for `seconds`, rounded.
    pub fn frames(self, seconds: f64) -> i64 {
        (seconds * self.fps()).round() as i64
    }

    pub fn seconds(self, frames: i64) -> f64 {
        frames as f64 / self.fps()
    }

    /// The whole number of frames a second timecode counts (30 for 29.97).
    pub fn timecode_base(self) -> u32 {
        self.fps().round().max(1.0) as u32
    }

    /// 29.97 and 59.94 count timecode in drop frame.
    pub fn drop_frame(self) -> bool {
        self.den == 1001 && matches!(self.timecode_base(), 30 | 60)
    }
}

/// A media file the timeline uses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub id: String,
    /// The file name (`take 1.mp4`).
    pub name: String,
    /// Where the file is: a `file://` URL or a path; may be just the name.
    pub url: String,
    /// Length in frames at the timeline's rate, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(default)]
    pub has_video: bool,
    #[serde(default)]
    pub has_audio: bool,
    /// A still picture (no length of its own).
    #[serde(default)]
    pub still: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackKind {
    Video,
    Audio,
}

/// A dissolve (or wipe) out of a clip into the next one on its track,
/// centred on the cut: `frames / 2` before it and the rest after.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub frames: i64,
    /// `dissolve` or `wipe`.
    pub kind: String,
}

impl Transition {
    /// Frames before the cut.
    pub fn before(&self) -> i64 {
        self.frames / 2
    }

    /// Frames after the cut.
    pub fn after(&self) -> i64 {
        self.frames - self.before()
    }
}

/// A clip: `duration` frames on the timeline from `start`, showing the
/// media from `source_start` (frames of the media at the timeline's rate)
/// at `speed`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub name: String,
    pub media_id: String,
    pub start: i64,
    pub duration: i64,
    pub source_start: i64,
    #[serde(default = "one")]
    pub speed: f64,
    /// A video clip's own sound plays (audio clips always sound).
    #[serde(default)]
    pub sound: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Into the next clip, which starts where this one ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
}

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

impl Clip {
    pub fn end(&self) -> i64 {
        self.start + self.duration
    }

    /// Frames of media the clip plays.
    pub fn source_duration(&self) -> i64 {
        (self.duration as f64 * self.speed.abs()).round() as i64
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub kind: TrackKind,
    pub name: String,
    /// In time order, not overlapping.
    pub clips: Vec<Clip>,
}

/// A timeline: video tracks from the bottom up (the first is the main
/// track), then audio tracks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub name: String,
    pub rate: Rate,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    pub media: Vec<Media>,
    pub tracks: Vec<Track>,
    /// What was left out or changed on the way, one line per kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Timeline {
    pub fn media(&self, id: &str) -> Option<&Media> {
        self.media.iter().find(|media| media.id == id)
    }

    pub fn video_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|track| track.kind == TrackKind::Video)
    }

    pub fn audio_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|track| track.kind == TrackKind::Audio)
    }

    /// The end of the last clip.
    pub fn duration(&self) -> i64 {
        self.tracks.iter().flat_map(|track| track.clips.iter().map(Clip::end)).max().unwrap_or(0)
    }

    /// Adds `note` once.
    pub fn note(&mut self, note: impl Into<String>) {
        let note = note.into();
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }

    /// Sorts each track's clips and moves a clip that overlaps an earlier
    /// one on its track to a new track of the same kind above (readers
    /// collect clips by lane and channel, which can overlap).
    pub fn tidy(&mut self) {
        let mut out: Vec<Track> = Vec::new();
        for kind in [TrackKind::Video, TrackKind::Audio] {
            for track in self.tracks.iter().filter(|track| track.kind == kind) {
                let mut clips = track.clips.clone();
                clips.sort_by_key(|clip| (clip.start, clip.duration));
                let mut lanes: Vec<Track> = vec![Track { kind, name: track.name.clone(), clips: Vec::new() }];
                for clip in clips {
                    match lanes.iter_mut().find(|lane| lane.clips.last().is_none_or(|last| last.end() <= clip.start)) {
                        Some(lane) => lane.clips.push(clip),
                        None => {
                            let name = format!("{} ({})", track.name, lanes.len() + 1);
                            lanes.push(Track { kind, name, clips: vec![clip] });
                        }
                    }
                }
                out.extend(lanes);
            }
        }
        // A transition needs the next clip right after it.
        for track in &mut out {
            for i in 0..track.clips.len() {
                let next_start = track.clips.get(i + 1).map(|clip| clip.start);
                let clip = &mut track.clips[i];
                if clip.transition.is_some() && next_start != Some(clip.end()) {
                    clip.transition = None;
                }
            }
        }
        self.tracks = out;
    }
}

/// Audio clips that only repeat a video clip's sound (same file, place,
/// trim and speed) become that clip's sound; video clips with no such twin
/// are silent. Audio tracks left empty are dropped. (OTIO and EDL keep a
/// clip's sound on separate audio tracks.)
pub(crate) fn fold_sound(timeline: &mut Timeline) {
    let mut video: Vec<(usize, usize)> = Vec::new();
    for (t, track) in timeline.tracks.iter().enumerate() {
        if track.kind == TrackKind::Video {
            video.extend((0..track.clips.len()).map(|c| (t, c)));
        }
    }
    for &(t, c) in &video {
        timeline.tracks[t].clips[c].sound = false;
    }
    for a in 0..timeline.tracks.len() {
        if timeline.tracks[a].kind != TrackKind::Audio {
            continue;
        }
        let mut kept = Vec::new();
        for clip in std::mem::take(&mut timeline.tracks[a].clips) {
            let twin = video.iter().copied().find(|&(t, c)| {
                let v = &timeline.tracks[t].clips[c];
                !v.sound
                    && v.media_id == clip.media_id
                    && v.start == clip.start
                    && v.duration == clip.duration
                    && v.source_start == clip.source_start
                    && (v.speed - clip.speed).abs() < 1e-9
            });
            match twin {
                Some((t, c)) => timeline.tracks[t].clips[c].sound = true,
                None => kept.push(clip),
            }
        }
        timeline.tracks[a].clips = kept;
    }
    timeline.tracks.retain(|track| track.kind == TrackKind::Video || !track.clips.is_empty());
}

/// The format of a file, by its extension or its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Otio,
    Fcpxml,
    Edl,
}

impl Format {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "otio" => Some(Self::Otio),
            "fcpxml" => Some(Self::Fcpxml),
            "edl" => Some(Self::Edl),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Otio => "otio",
            Self::Fcpxml => "fcpxml",
            Self::Edl => "edl",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Otio => "application/json",
            Self::Fcpxml => "application/xml",
            Self::Edl => "text/plain",
        }
    }

    /// Guesses from a file name and its text.
    pub fn sniff(file_name: &str, text: &str) -> Option<Self> {
        let extension = file_name.rsplit('.').next().unwrap_or("");
        if let Some(format) = Self::parse(extension) {
            return Some(format);
        }
        let head = text.trim_start_matches('\u{feff}').trim_start();
        if head.starts_with('{') {
            Some(Self::Otio)
        } else if head.starts_with('<') {
            Some(Self::Fcpxml)
        } else if head.lines().any(|line| line.trim_start().starts_with("TITLE:") || line.trim_start().starts_with("001 ")) {
            Some(Self::Edl)
        } else {
            None
        }
    }
}

/// What `format` cannot hold of `timeline`, one line per kind (on top of
/// [`Timeline::notes`]); the writer puts them in the file too.
pub fn format_notes(timeline: &Timeline, format: Format) -> Vec<String> {
    let mut notes = Vec::new();
    match format {
        Format::Otio => {}
        Format::Fcpxml => {
            if timeline.tracks.iter().skip(1).any(|t| t.clips.iter().any(|c| c.transition.is_some())) {
                notes.push("Dissolves on tracks other than the main one were left out.".to_owned());
            }
        }
        Format::Edl => {
            if timeline.video_tracks().count() > 1 {
                notes.push("An EDL holds one video track: only the main track went in.".to_owned());
            }
            let main_sound = timeline
                .video_tracks()
                .next()
                .is_some_and(|t| t.clips.iter().any(|c| c.sound && timeline.media(&c.media_id).is_some_and(|m| m.has_audio)));
            if usize::from(main_sound) + timeline.audio_tracks().count() > 2 {
                notes.push("An EDL holds two sound channels: only the first two went in.".to_owned());
            }
            if timeline.tracks.iter().any(|t| t.clips.iter().any(|c| !c.enabled)) {
                notes.push("Hidden clips were left out.".to_owned());
            }
        }
    }
    notes
}

/// Writes `timeline` in `format`.
pub fn write(timeline: &Timeline, format: Format) -> String {
    match format {
        Format::Otio => otio::write(timeline),
        Format::Fcpxml => fcpxml::write(timeline),
        Format::Edl => edl::write(timeline),
    }
}

/// Reads a timeline in `format`; `rate` is the frame rate to assume for an
/// EDL (which does not say).
pub fn read(text: &str, format: Format, rate: Rate) -> Result<Timeline, String> {
    let mut timeline = match format {
        Format::Otio => otio::read(text)?,
        Format::Fcpxml => fcpxml::read(text)?,
        Format::Edl => edl::read(text, rate)?,
    };
    timeline.tidy();
    Ok(timeline)
}

/// A file name's last part (`/a/b c.mp4` and `C:\a\b c.mp4` give `b c.mp4`),
/// with `file://` URLs decoded.
pub fn file_name(url: &str) -> String {
    let path = url_to_path(url);
    path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned()
}

/// A `file://` URL as a path (percent-decoded); a bare file name
/// (percent-decoded) as it is; any other URL as it is.
pub fn url_to_path(url: &str) -> String {
    let Some(rest) = url.strip_prefix("file://") else {
        return if url.contains("://") || url.contains(['/', '\\']) { url.to_owned() } else { percent_decode(url) };
    };
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest);
    // file:///C:/x is C:/x on Windows.
    let bytes = decoded.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        decoded[1..].to_owned()
    } else {
        decoded
    }
}

/// A path as a `file://` URL (spaces and other unsafe characters
/// percent-encoded); a bare file name (no folder known, as on the web) as a
/// relative URL, so it does not claim a place on the disk; a URL as it is.
pub fn path_to_url(path: &str) -> String {
    if path.contains("://") {
        return path.to_owned();
    }
    let path = path.replace('\\', "/");
    let mut out = String::new();
    if path.contains('/') {
        out.push_str("file://");
        if !path.starts_with('/') {
            out.push('/');
        }
    }
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(value) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates() {
        assert_eq!(Rate::from_fps(29.97), Rate::new(30000, 1001));
        assert_eq!(Rate::from_fps(25.0), Rate::new(25, 1));
        assert_eq!(Rate::from_fps(12.5), Rate::new(12500, 1000));
        assert!(Rate::new(30000, 1001).drop_frame());
        assert!(!Rate::new(24000, 1001).drop_frame());
        assert_eq!(Rate::new(30, 1).frames(1.5), 45);
    }

    #[test]
    fn urls_and_names() {
        assert_eq!(path_to_url("C:\\Clips\\take 1.mp4"), "file:///C:/Clips/take%201.mp4");
        assert_eq!(url_to_path("file:///C:/Clips/take%201.mp4"), "C:/Clips/take 1.mp4");
        assert_eq!(url_to_path("file:///Users/me/a%20b.mov"), "/Users/me/a b.mov");
        assert_eq!(file_name("file:///C:/Clips/take%201.mp4"), "take 1.mp4");
        assert_eq!(file_name("b.mov"), "b.mov");
        assert_eq!(path_to_url("take 1.mp4"), "take%201.mp4", "no folder: a relative URL");
        assert_eq!(url_to_path("take%201.mp4"), "take 1.mp4");
    }

    #[test]
    fn tidy_moves_overlaps_up_and_drops_orphan_transitions() {
        let clip = |start, duration, transition: bool| Clip {
            name: "c".into(),
            media_id: "m".into(),
            start,
            duration,
            source_start: 0,
            speed: 1.0,
            sound: false,
            enabled: true,
            transition: transition.then(|| Transition { frames: 10, kind: "dissolve".into() }),
        };
        let mut timeline = Timeline {
            name: "t".into(),
            rate: Rate::new(30, 1),
            width: 0,
            height: 0,
            media: vec![],
            tracks: vec![Track { kind: TrackKind::Video, name: "V1".into(), clips: vec![clip(50, 50, false), clip(0, 60, true)] }],
            notes: vec![],
        };
        timeline.tidy();
        assert_eq!(timeline.tracks.len(), 2);
        assert_eq!(timeline.tracks[0].clips[0].start, 0);
        assert!(timeline.tracks[0].clips[0].transition.is_none(), "nothing follows it on its track");
        assert_eq!(timeline.tracks[1].name, "V1 (2)");
    }
}
