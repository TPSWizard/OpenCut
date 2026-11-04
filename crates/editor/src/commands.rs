//! Every edit is a [`Command`] applied with [`apply`]. Commands validate
//! their input and either change the project or leave it untouched.

use time::{FrameRate, MediaTime};

use crate::animation::{Segment, parse_color_to_linear_rgba, split_animations};
use crate::document::{
    Background, CanvasSize, Effect, Element, ElementKind, Lane, Mask, MediaItem, Project, Track,
    TrackKind,
};
use crate::error::EditorError;
use crate::params::Params;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ElementRef {
    pub track_id: String,
    pub element_id: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElementPatch {
    pub name: Option<String>,
    pub start_time: Option<MediaTime>,
    pub duration: Option<MediaTime>,
    pub trim_start: Option<MediaTime>,
    pub trim_end: Option<MediaTime>,
    pub hidden: Option<bool>,
    /// Merged into the element's parameters.
    pub params: Option<Params>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsPatch {
    pub fps: Option<FrameRate>,
    pub canvas: Option<CanvasSize>,
    pub background: Option<Background>,
}

/// Where an inserted element goes.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackPlacement {
    /// On this track, which must accept the element and be free there.
    Track(String),
    /// On the first fitting track (the main track for video), or a new one.
    Auto,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    AddMedia(MediaItem),
    RemoveMedia {
        media_id: String,
    },
    InsertElement {
        element: Element,
        placement: TrackPlacement,
    },
    MoveElement {
        element_id: String,
        track_id: Option<String>,
        start_time: MediaTime,
    },
    /// Drags one edge of an element to a new timeline time, trimming or
    /// extending it against its source.
    ResizeElement {
        element_id: String,
        edge: Edge,
        time: MediaTime,
    },
    SplitElement {
        element_id: String,
        time: MediaTime,
    },
    DeleteElements(Vec<String>),
    DuplicateElements(Vec<String>),
    UpdateElement {
        element_id: String,
        patch: ElementPatch,
    },
    AddTrack {
        kind: TrackKind,
    },
    RemoveTrack {
        track_id: String,
    },
    SetTrackMuted {
        track_id: String,
        muted: bool,
    },
    SetTrackHidden {
        track_id: String,
        hidden: bool,
    },
    RenameProject(String),
    UpdateSettings(SettingsPatch),
    /// Sets a keyframe of `path` at a timeline time (relative times are
    /// stored). A key already at that time is replaced.
    SetKeyframe {
        element_id: String,
        path: String,
        time: MediaTime,
        value: f64,
        segment: Option<Segment>,
    },
    /// Sets a colour key (`#rrggbb` or `#rrggbbaa`) of `path` at a timeline
    /// time: the web app's four linear-light component channels.
    SetColorKeyframe {
        element_id: String,
        path: String,
        time: MediaTime,
        color: String,
    },
    RemoveKeyframe {
        element_id: String,
        path: String,
        keyframe_id: String,
    },
    /// Drops every keyframe of `path`; the element's own parameter value applies again.
    ClearAnimation {
        element_id: String,
        path: String,
    },
    AddEffect {
        element_id: String,
        effect: Effect,
    },
    UpdateEffect {
        element_id: String,
        effect_id: String,
        params: Option<Params>,
        enabled: Option<bool>,
    },
    RemoveEffect {
        element_id: String,
        effect_id: String,
    },
    AddMask {
        element_id: String,
        mask: Mask,
    },
    UpdateMask {
        element_id: String,
        mask_id: String,
        params: Params,
    },
    RemoveMask {
        element_id: String,
        mask_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    Elements(Vec<ElementRef>),
    Track(String),
}

impl Outcome {
    pub fn element_refs(&self) -> &[ElementRef] {
        match self {
            Self::Elements(refs) => refs,
            _ => &[],
        }
    }
}

const MIN_ELEMENT_DURATION: MediaTime = MediaTime::from_ticks(1200); // 10 ms

pub fn apply(project: &mut Project, command: &Command) -> Result<Outcome, EditorError> {
    let outcome = match command {
        Command::AddMedia(item) => {
            if project.media_item(&item.id).is_some() {
                return Err(EditorError::InvalidParams(format!(
                    "media {} already exists",
                    item.id
                )));
            }
            project.media.push(item.clone());
            Outcome::None
        }
        Command::RemoveMedia { media_id } => {
            let index = project
                .media
                .iter()
                .position(|item| &item.id == media_id)
                .ok_or_else(|| EditorError::MediaNotFound(media_id.clone()))?;
            project.media.remove(index);
            for track in &mut project.tracks {
                track
                    .elements
                    .retain(|element| element.media_id() != Some(media_id.as_str()));
            }
            Outcome::None
        }
        Command::InsertElement { element, placement } => {
            validate_element(project, element)?;
            let element = element.clone();
            let track_index = match placement {
                TrackPlacement::Track(track_id) => {
                    let index = project
                        .track_index(track_id)
                        .ok_or_else(|| EditorError::TrackNotFound(track_id.clone()))?;
                    let track = &project.tracks[index];
                    if !track.accepts(&element.kind) {
                        return Err(EditorError::TrackKindMismatch {
                            element: element.kind.label().to_owned(),
                            track: track.kind.label().to_owned(),
                        });
                    }
                    if !track.is_free(element.start_time, element.end_time(), None) {
                        return Err(EditorError::Overlap);
                    }
                    index
                }
                TrackPlacement::Auto => {
                    fitting_track(project, &element.kind, element.start_time, element.end_time())
                }
            };
            let track = &mut project.tracks[track_index];
            let reference = ElementRef {
                track_id: track.id.clone(),
                element_id: element.id.clone(),
            };
            track.elements.push(element);
            track.sort_elements();
            Outcome::Elements(vec![reference])
        }
        Command::MoveElement {
            element_id,
            track_id,
            start_time,
        } => {
            let (from_track, element_index) = locate(project, element_id)?;
            let start_time = (*start_time).max(MediaTime::ZERO);
            let to_track = match track_id {
                Some(track_id) => project
                    .track_index(track_id)
                    .ok_or_else(|| EditorError::TrackNotFound(track_id.clone()))?,
                None => from_track,
            };
            let element = &project.tracks[from_track].elements[element_index];
            let end_time = start_time + element.duration;
            let target = &project.tracks[to_track];
            if !target.accepts(&element.kind) {
                return Err(EditorError::TrackKindMismatch {
                    element: element.kind.label().to_owned(),
                    track: target.kind.label().to_owned(),
                });
            }
            let ignore = (from_track == to_track).then_some(element_id.as_str());
            if !target.is_free(start_time, end_time, ignore) {
                return Err(EditorError::Overlap);
            }
            let mut element = project.tracks[from_track].elements.remove(element_index);
            element.start_time = start_time;
            let track = &mut project.tracks[to_track];
            let reference = ElementRef {
                track_id: track.id.clone(),
                element_id: element.id.clone(),
            };
            track.elements.push(element);
            track.sort_elements();
            Outcome::Elements(vec![reference])
        }
        Command::ResizeElement {
            element_id,
            edge,
            time,
        } => {
            let (track_index, element_index) = locate(project, element_id)?;
            let track = &project.tracks[track_index];
            let element = &track.elements[element_index];
            let (start, end) = match edge {
                Edge::Start => {
                    let earliest = element.start_time - element.trim_start;
                    let latest = element.end_time() - MIN_ELEMENT_DURATION;
                    let start = (*time).clamp(earliest.max(MediaTime::ZERO), latest);
                    (start, element.end_time())
                }
                Edge::End => {
                    let earliest = element.start_time + MIN_ELEMENT_DURATION;
                    let latest = element
                        .max_duration()
                        .map(|max| element.start_time + max)
                        .unwrap_or(MediaTime::from_ticks(i64::MAX / 4));
                    let end = (*time).clamp(earliest, latest);
                    (element.start_time, end)
                }
            };
            if !track.is_free(start, end, Some(element_id)) {
                return Err(EditorError::Overlap);
            }
            let reference = ElementRef {
                track_id: track.id.clone(),
                element_id: element_id.clone(),
            };
            let element = &mut project.tracks[track_index].elements[element_index];
            match edge {
                Edge::Start => {
                    let delta = start - element.start_time;
                    element.trim_start = element.trim_start + delta;
                    element.start_time = start;
                    element.duration = end - start;
                    // Keys stay with the same source frames.
                    for channel in element.animations.values_mut() {
                        channel.shift(-delta);
                    }
                }
                Edge::End => {
                    element.duration = end - start;
                    if let Some(source) = element.source_duration {
                        element.trim_end =
                            (source - element.trim_start - element.duration).max(MediaTime::ZERO);
                    }
                }
            }
            project.tracks[track_index].sort_elements();
            Outcome::Elements(vec![reference])
        }
        Command::SplitElement { element_id, time } => {
            let (track_index, element_index) = locate(project, element_id)?;
            let element = &project.tracks[track_index].elements[element_index];
            if *time <= element.start_time + MIN_ELEMENT_DURATION
                || *time >= element.end_time() - MIN_ELEMENT_DURATION
            {
                return Err(EditorError::InvalidParams(
                    "split time must fall inside the element".to_owned(),
                ));
            }
            let split_at = *time;
            let mut right = element.clone();
            right.id = crate::new_id();
            let left_duration = split_at - element.start_time;
            right.start_time = split_at;
            right.duration = element.duration - left_duration;
            right.trim_start = element.trim_start + left_duration;
            let (left_animations, right_animations) =
                split_animations(&element.animations, left_duration);
            right.animations = right_animations;
            let track = &mut project.tracks[track_index];
            let left = &mut track.elements[element_index];
            left.duration = left_duration;
            left.animations = left_animations;
            if let Some(source) = left.source_duration {
                left.trim_end = (source - left.trim_start - left.duration).max(MediaTime::ZERO);
            }
            let track_id = track.id.clone();
            let left_id = left.id.clone();
            let right_id = right.id.clone();
            track.elements.push(right);
            track.sort_elements();
            Outcome::Elements(vec![
                ElementRef {
                    track_id: track_id.clone(),
                    element_id: left_id,
                },
                ElementRef {
                    track_id,
                    element_id: right_id,
                },
            ])
        }
        Command::DeleteElements(ids) => {
            for id in ids {
                let (track_index, element_index) = locate(project, id)?;
                project.tracks[track_index].elements.remove(element_index);
            }
            Outcome::None
        }
        Command::DuplicateElements(ids) => {
            let mut created = Vec::new();
            for id in ids {
                let (track_index, element_index) = locate(project, id)?;
                let original = project.tracks[track_index].elements[element_index].clone();
                let mut copy = original.clone();
                copy.id = crate::new_id();
                let kind = copy.kind.clone();
                let duration = copy.duration;
                // Right after the original on the same track when there is
                // room, otherwise at the next free spot.
                let track = &project.tracks[track_index];
                let desired = original.end_time();
                let start = nearest_free_start(track, duration, desired, None)
                    .unwrap_or(track.end_time());
                copy.start_time = start;
                let target = if project.tracks[track_index].is_free(start, start + duration, None)
                {
                    track_index
                } else {
                    fitting_track(project, &kind, start, start + duration)
                };
                let track = &mut project.tracks[target];
                created.push(ElementRef {
                    track_id: track.id.clone(),
                    element_id: copy.id.clone(),
                });
                track.elements.push(copy);
                track.sort_elements();
            }
            Outcome::Elements(created)
        }
        Command::UpdateElement { element_id, patch } => {
            let (track_index, element_index) = locate(project, element_id)?;
            let track = &project.tracks[track_index];
            let current = &track.elements[element_index];
            let start_time = patch.start_time.unwrap_or(current.start_time);
            let duration = patch.duration.unwrap_or(current.duration);
            if start_time < MediaTime::ZERO || duration < MIN_ELEMENT_DURATION {
                return Err(EditorError::InvalidParams(
                    "start time must be positive and duration at least 10 ms".to_owned(),
                ));
            }
            if !track.is_free(start_time, start_time + duration, Some(element_id)) {
                return Err(EditorError::Overlap);
            }
            let reference = ElementRef {
                track_id: track.id.clone(),
                element_id: element_id.clone(),
            };
            let element = &mut project.tracks[track_index].elements[element_index];
            if let Some(name) = &patch.name {
                element.name = name.clone();
            }
            element.start_time = start_time;
            element.duration = duration;
            if let Some(trim_start) = patch.trim_start {
                element.trim_start = trim_start.max(MediaTime::ZERO);
            }
            if let Some(trim_end) = patch.trim_end {
                element.trim_end = trim_end.max(MediaTime::ZERO);
            }
            if let Some(hidden) = patch.hidden {
                element.hidden = hidden;
            }
            if let Some(params) = &patch.params {
                element.params.merge(params);
            }
            project.tracks[track_index].sort_elements();
            Outcome::Elements(vec![reference])
        }
        Command::AddTrack { kind } => {
            let track = new_track(project, *kind);
            let id = track.id.clone();
            let index = insertion_index(project, *kind);
            project.tracks.insert(index, track);
            Outcome::Track(id)
        }
        Command::RemoveTrack { track_id } => {
            let index = project
                .track_index(track_id)
                .ok_or_else(|| EditorError::TrackNotFound(track_id.clone()))?;
            if project.tracks[index].lane == Lane::Main {
                return Err(EditorError::CannotRemoveMainTrack);
            }
            project.tracks.remove(index);
            Outcome::None
        }
        Command::SetTrackMuted { track_id, muted } => {
            let track = project
                .track_mut(track_id)
                .ok_or_else(|| EditorError::TrackNotFound(track_id.clone()))?;
            track.muted = *muted;
            Outcome::None
        }
        Command::SetTrackHidden { track_id, hidden } => {
            let track = project
                .track_mut(track_id)
                .ok_or_else(|| EditorError::TrackNotFound(track_id.clone()))?;
            track.hidden = *hidden;
            Outcome::None
        }
        Command::SetKeyframe {
            element_id,
            path,
            time,
            value,
            segment,
        } => {
            if path.trim().is_empty() {
                return Err(EditorError::InvalidParams(
                    "keyframe path cannot be empty".to_owned(),
                ));
            }
            if !value.is_finite() {
                return Err(EditorError::InvalidParams(
                    "keyframe value must be a number".to_owned(),
                ));
            }
            let (reference, element) = element_mut(project, element_id)?;
            let local = *time - element.start_time;
            let channel = element.animations.entry(path.clone()).or_default();
            if channel.is_color() {
                return Err(EditorError::InvalidParams(format!(
                    "{path} is a colour channel; colour keys cannot be set here"
                )));
            }
            channel.upsert(local, *value, *segment);
            Outcome::Elements(vec![reference])
        }
        Command::SetColorKeyframe {
            element_id,
            path,
            time,
            color,
        } => {
            if path.trim().is_empty() {
                return Err(EditorError::InvalidParams(
                    "keyframe path cannot be empty".to_owned(),
                ));
            }
            let rgba = parse_color_to_linear_rgba(color).ok_or_else(|| {
                EditorError::InvalidParams(format!("{color} is not a hex colour"))
            })?;
            let (reference, element) = element_mut(project, element_id)?;
            if current_param(element, path).is_some_and(|value| value.as_number().is_some()) {
                return Err(EditorError::InvalidParams(format!(
                    "{path} is a number; a colour key cannot be set on it"
                )));
            }
            let local = *time - element.start_time;
            let channel = element.animations.entry(path.clone()).or_default();
            channel
                .upsert_color(local, rgba)
                .map_err(|reason| EditorError::InvalidParams(format!("{path}: {reason}")))?;
            Outcome::Elements(vec![reference])
        }
        Command::RemoveKeyframe {
            element_id,
            path,
            keyframe_id,
        } => {
            let (reference, element) = element_mut(project, element_id)?;
            let Some(channel) = element.animations.get_mut(path) else {
                return Err(EditorError::InvalidParams(format!(
                    "{path} is not animated"
                )));
            };
            if !channel.remove(keyframe_id) {
                return Err(EditorError::InvalidParams(format!(
                    "keyframe {keyframe_id} does not exist"
                )));
            }
            if channel.is_empty() {
                element.animations.remove(path);
            }
            Outcome::Elements(vec![reference])
        }
        Command::ClearAnimation { element_id, path } => {
            let (reference, element) = element_mut(project, element_id)?;
            element.animations.remove(path);
            Outcome::Elements(vec![reference])
        }
        Command::AddEffect { element_id, effect } => {
            let (reference, element) = element_mut(project, element_id)?;
            if !element.kind.is_visual() {
                return Err(EditorError::InvalidParams(
                    "effects apply to visual elements only".to_owned(),
                ));
            }
            if element.effect(&effect.id).is_some() {
                return Err(EditorError::InvalidParams(format!(
                    "effect {} already exists",
                    effect.id
                )));
            }
            element.effects.push(effect.clone());
            Outcome::Elements(vec![reference])
        }
        Command::UpdateEffect {
            element_id,
            effect_id,
            params,
            enabled,
        } => {
            let (reference, element) = element_mut(project, element_id)?;
            let effect = element
                .effects
                .iter_mut()
                .find(|effect| &effect.id == effect_id)
                .ok_or_else(|| {
                    EditorError::InvalidParams(format!("effect {effect_id} does not exist"))
                })?;
            if let Some(params) = params {
                effect.params.merge(params);
            }
            if let Some(enabled) = enabled {
                effect.enabled = *enabled;
            }
            Outcome::Elements(vec![reference])
        }
        Command::RemoveEffect {
            element_id,
            effect_id,
        } => {
            let (reference, element) = element_mut(project, element_id)?;
            let before = element.effects.len();
            element.effects.retain(|effect| &effect.id != effect_id);
            if element.effects.len() == before {
                return Err(EditorError::InvalidParams(format!(
                    "effect {effect_id} does not exist"
                )));
            }
            let prefix = format!("effects.{effect_id}.");
            element.animations.retain(|path, _| !path.starts_with(&prefix));
            Outcome::Elements(vec![reference])
        }
        Command::AddMask { element_id, mask } => {
            let (reference, element) = element_mut(project, element_id)?;
            if !element.can_be_masked() {
                return Err(EditorError::InvalidParams(
                    "only video and image elements can be masked".to_owned(),
                ));
            }
            if element.mask(&mask.id).is_some() {
                return Err(EditorError::InvalidParams(format!(
                    "mask {} already exists",
                    mask.id
                )));
            }
            element.masks.push(mask.clone());
            Outcome::Elements(vec![reference])
        }
        Command::UpdateMask {
            element_id,
            mask_id,
            params,
        } => {
            let (reference, element) = element_mut(project, element_id)?;
            let mask = element
                .masks
                .iter_mut()
                .find(|mask| &mask.id == mask_id)
                .ok_or_else(|| {
                    EditorError::InvalidParams(format!("mask {mask_id} does not exist"))
                })?;
            mask.params.merge(params);
            Outcome::Elements(vec![reference])
        }
        Command::RemoveMask {
            element_id,
            mask_id,
        } => {
            let (reference, element) = element_mut(project, element_id)?;
            let before = element.masks.len();
            element.masks.retain(|mask| &mask.id != mask_id);
            if element.masks.len() == before {
                return Err(EditorError::InvalidParams(format!(
                    "mask {mask_id} does not exist"
                )));
            }
            let prefix = format!("masks.{mask_id}.");
            element.animations.retain(|path, _| !path.starts_with(&prefix));
            Outcome::Elements(vec![reference])
        }
        Command::RenameProject(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(EditorError::InvalidParams(
                    "project name cannot be empty".to_owned(),
                ));
            }
            project.name = name.to_owned();
            Outcome::None
        }
        Command::UpdateSettings(patch) => {
            if let Some(fps) = patch.fps {
                if !fps.is_valid() {
                    return Err(EditorError::InvalidParams("invalid frame rate".to_owned()));
                }
                project.settings.fps = fps;
            }
            if let Some(canvas) = patch.canvas {
                if canvas.width < 16 || canvas.height < 16 || canvas.width > 8192 || canvas.height > 8192 {
                    return Err(EditorError::InvalidParams(
                        "canvas must be between 16 and 8192 pixels".to_owned(),
                    ));
                }
                project.settings.canvas = canvas;
            }
            if let Some(background) = &patch.background {
                project.settings.background = background.clone();
            }
            Outcome::None
        }
    };
    project.touch();
    Ok(outcome)
}

fn validate_element(project: &Project, element: &Element) -> Result<(), EditorError> {
    if element.start_time < MediaTime::ZERO {
        return Err(EditorError::InvalidParams(
            "start time must be positive".to_owned(),
        ));
    }
    if element.duration < MIN_ELEMENT_DURATION {
        return Err(EditorError::InvalidParams(
            "duration must be at least 10 ms".to_owned(),
        ));
    }
    if let Some(media_id) = element.media_id()
        && project.media_item(media_id).is_none()
    {
        return Err(EditorError::MediaNotFound(media_id.to_owned()));
    }
    if project.locate(&element.id).is_some() {
        return Err(EditorError::InvalidParams(format!(
            "element {} already exists",
            element.id
        )));
    }
    Ok(())
}

fn locate(project: &Project, element_id: &str) -> Result<(usize, usize), EditorError> {
    project
        .locate(element_id)
        .ok_or_else(|| EditorError::ElementNotFound(element_id.to_owned()))
}

/// An element to change in place, with its reference for the outcome.
/// The stored (unanimated) value a keyframe path refers to: an element
/// parameter, or one of an effect's or mask's (`effects.<id>.params.<key>`).
fn current_param<'a>(element: &'a Element, path: &str) -> Option<&'a crate::params::ParamValue> {
    if let Some(rest) = path.strip_prefix("effects.") {
        let (effect_id, key) = rest.split_once(".params.")?;
        return element.effect(effect_id)?.params.get(key);
    }
    if let Some(rest) = path.strip_prefix("masks.") {
        let (mask_id, key) = rest.split_once(".params.")?;
        return element.mask(mask_id)?.params.get(key);
    }
    element.params.get(path)
}

fn element_mut<'a>(
    project: &'a mut Project,
    element_id: &str,
) -> Result<(ElementRef, &'a mut Element), EditorError> {
    let (track_index, element_index) = locate(project, element_id)?;
    let track = &mut project.tracks[track_index];
    let reference = ElementRef {
        track_id: track.id.clone(),
        element_id: element_id.to_owned(),
    };
    Ok((reference, &mut track.elements[element_index]))
}

/// The first track that can take the element at `[start, end)`, creating one
/// when none is free. Video prefers the main track, then overlays closest to it.
fn fitting_track(project: &mut Project, kind: &ElementKind, start: MediaTime, end: MediaTime) -> usize {
    let track_kind = kind.track_kind();
    let main = project.main_track_index();
    let candidates: Vec<usize> = match track_kind {
        TrackKind::Video => std::iter::once(main)
            .chain((0..main).rev())
            .filter(|&index| project.tracks[index].kind == TrackKind::Video)
            .collect(),
        TrackKind::Text => (0..main)
            .rev()
            .filter(|&index| project.tracks[index].kind == TrackKind::Text)
            .collect(),
        TrackKind::Audio => (main + 1..project.tracks.len())
            .filter(|&index| project.tracks[index].kind == TrackKind::Audio)
            .collect(),
    };
    if let Some(index) = candidates
        .into_iter()
        .find(|&index| project.tracks[index].is_free(start, end, None))
    {
        return index;
    }
    let track = new_track(project, track_kind);
    let index = insertion_index(project, track_kind);
    project.tracks.insert(index, track);
    index
}

fn new_track(project: &Project, kind: TrackKind) -> Track {
    let count = project
        .tracks
        .iter()
        .filter(|track| track.kind == kind)
        .count();
    let (lane, label) = match kind {
        TrackKind::Video => (Lane::Overlay, "Video"),
        TrackKind::Text => (Lane::Overlay, "Text"),
        TrackKind::Audio => (Lane::Audio, "Audio"),
    };
    Track::new(kind, lane, format!("{label} {}", count + 1))
}

/// New overlay tracks go on top; new audio tracks go to the bottom.
fn insertion_index(project: &Project, kind: TrackKind) -> usize {
    match kind {
        TrackKind::Video | TrackKind::Text => 0,
        TrackKind::Audio => project.tracks.len(),
    }
}

/// The closest start time to `desired` at which an element of `duration`
/// fits on `track` (ignoring `ignore`), or `None` when it cannot fit at all
/// before the track's last gap.
pub fn nearest_free_start(
    track: &Track,
    duration: MediaTime,
    desired: MediaTime,
    ignore: Option<&str>,
) -> Option<MediaTime> {
    let desired = desired.max(MediaTime::ZERO);
    if track.is_free(desired, desired + duration, ignore) {
        return Some(desired);
    }
    let mut others: Vec<&Element> = track
        .elements
        .iter()
        .filter(|element| ignore != Some(element.id.as_str()))
        .collect();
    others.sort_by_key(|element| element.start_time);
    let mut candidates = Vec::new();
    // Gap before the first element, between elements, and after the last.
    let mut cursor = MediaTime::ZERO;
    for element in &others {
        if element.start_time - cursor >= duration {
            candidates.push((cursor, element.start_time - duration));
        }
        cursor = cursor.max(element.end_time());
    }
    candidates.push((cursor, MediaTime::from_ticks(i64::MAX / 4)));
    candidates
        .into_iter()
        .map(|(low, high)| desired.clamp(low, high))
        .min_by_key(|candidate| (*candidate - desired).as_ticks().abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::MediaKind;

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn video_media(id: &str, duration: f64) -> MediaItem {
        MediaItem {
            id: id.to_owned(),
            name: format!("{id}.mp4"),
            kind: MediaKind::Video,
            path: format!("{id}.mp4").into(),
            duration: Some(seconds(duration)),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            has_audio: true,
        }
    }

    fn project_with_clip() -> (Project, String) {
        let mut project = Project::new("Test");
        apply(&mut project, &Command::AddMedia(video_media("m1", 10.0))).unwrap();
        let element = Element::from_media(project.media_item("m1").unwrap(), seconds(0.0));
        let outcome = apply(
            &mut project,
            &Command::InsertElement {
                element,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        let id = outcome.element_refs()[0].element_id.clone();
        (project, id)
    }

    #[test]
    fn auto_placement_uses_main_track_then_creates_overlays() {
        let (mut project, _) = project_with_clip();
        assert_eq!(project.tracks.len(), 1);
        let element = Element::from_media(project.media_item("m1").unwrap(), seconds(2.0));
        let outcome = apply(
            &mut project,
            &Command::InsertElement {
                element,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        assert_eq!(project.tracks.len(), 2);
        assert_eq!(project.tracks[0].lane, Lane::Overlay);
        assert_eq!(outcome.element_refs()[0].track_id, project.tracks[0].id);
        assert_eq!(project.main_track_index(), 1);
        // Text goes on its own track above everything.
        apply(
            &mut project,
            &Command::InsertElement {
                element: Element::text("Title", seconds(0.0)),
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        assert_eq!(project.tracks[0].kind, TrackKind::Text);
        assert_eq!(project.tracks.len(), 3);
    }

    #[test]
    fn explicit_placement_rejects_overlap_and_wrong_kind() {
        let (mut project, _) = project_with_clip();
        let main = project.tracks[0].id.clone();
        let element = Element::from_media(project.media_item("m1").unwrap(), seconds(5.0));
        let error = apply(
            &mut project,
            &Command::InsertElement {
                element,
                placement: TrackPlacement::Track(main.clone()),
            },
        )
        .unwrap_err();
        assert_eq!(error, EditorError::Overlap);
        let error = apply(
            &mut project,
            &Command::InsertElement {
                element: Element::text("x", seconds(20.0)),
                placement: TrackPlacement::Track(main),
            },
        )
        .unwrap_err();
        assert!(matches!(error, EditorError::TrackKindMismatch { .. }));
    }

    #[test]
    fn split_keeps_source_continuity() {
        let (mut project, id) = project_with_clip();
        let outcome = apply(
            &mut project,
            &Command::SplitElement {
                element_id: id.clone(),
                time: seconds(4.0),
            },
        )
        .unwrap();
        let refs = outcome.element_refs();
        assert_eq!(refs.len(), 2);
        let (_, left) = project.element(&refs[0].element_id).unwrap();
        let (_, right) = project.element(&refs[1].element_id).unwrap();
        assert_eq!(left.duration, seconds(4.0));
        assert_eq!(left.trim_end, seconds(6.0));
        assert_eq!(right.start_time, seconds(4.0));
        assert_eq!(right.trim_start, seconds(4.0));
        assert_eq!(right.duration, seconds(6.0));
        assert_eq!(right.source_time(seconds(5.0)), seconds(5.0));
        assert_eq!(project.duration(), seconds(10.0));
    }

    #[test]
    fn resize_trims_against_source_bounds() {
        let (mut project, id) = project_with_clip();
        apply(
            &mut project,
            &Command::ResizeElement {
                element_id: id.clone(),
                edge: Edge::Start,
                time: seconds(3.0),
            },
        )
        .unwrap();
        let (_, element) = project.element(&id).unwrap();
        assert_eq!(element.start_time, seconds(3.0));
        assert_eq!(element.trim_start, seconds(3.0));
        assert_eq!(element.duration, seconds(7.0));
        // Cannot extend past the end of the source.
        apply(
            &mut project,
            &Command::ResizeElement {
                element_id: id.clone(),
                edge: Edge::End,
                time: seconds(30.0),
            },
        )
        .unwrap();
        let (_, element) = project.element(&id).unwrap();
        assert_eq!(element.end_time(), seconds(10.0));
        assert_eq!(element.trim_end, seconds(0.0));
        // Pulling the start back exposes trimmed material again.
        apply(
            &mut project,
            &Command::ResizeElement {
                element_id: id.clone(),
                edge: Edge::Start,
                time: seconds(-5.0),
            },
        )
        .unwrap();
        let (_, element) = project.element(&id).unwrap();
        assert_eq!(element.start_time, seconds(0.0));
        assert_eq!(element.trim_start, seconds(0.0));
    }

    #[test]
    fn move_duplicate_delete() {
        let (mut project, id) = project_with_clip();
        apply(
            &mut project,
            &Command::MoveElement {
                element_id: id.clone(),
                track_id: None,
                start_time: seconds(2.5),
            },
        )
        .unwrap();
        assert_eq!(project.element(&id).unwrap().1.start_time, seconds(2.5));
        let outcome = apply(&mut project, &Command::DuplicateElements(vec![id.clone()])).unwrap();
        let copy = outcome.element_refs()[0].element_id.clone();
        assert_eq!(project.element(&copy).unwrap().1.start_time, seconds(12.5));
        apply(&mut project, &Command::DeleteElements(vec![id.clone(), copy])).unwrap();
        assert!(project.elements().next().is_none());
        assert!(matches!(
            apply(&mut project, &Command::DeleteElements(vec![id])),
            Err(EditorError::ElementNotFound(_))
        ));
    }

    #[test]
    fn removing_media_removes_its_elements() {
        let (mut project, _) = project_with_clip();
        apply(
            &mut project,
            &Command::RemoveMedia {
                media_id: "m1".to_owned(),
            },
        )
        .unwrap();
        assert!(project.media.is_empty());
        assert!(project.elements().next().is_none());
    }

    #[test]
    fn update_element_merges_params() {
        let (mut project, id) = project_with_clip();
        let mut params = Params::new();
        params.set("opacity", 0.25).set("transform.rotate", 45.0);
        apply(
            &mut project,
            &Command::UpdateElement {
                element_id: id.clone(),
                patch: ElementPatch {
                    name: Some("Renamed".to_owned()),
                    params: Some(params),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let (_, element) = project.element(&id).unwrap();
        assert_eq!(element.name, "Renamed");
        assert_eq!(element.params.number("opacity", 1.0), 0.25);
        assert_eq!(element.params.number("transform.scaleX", 0.0), 1.0);
    }

    #[test]
    fn tracks_keep_their_order() {
        let mut project = Project::new("Tracks");
        apply(&mut project, &Command::AddTrack { kind: TrackKind::Audio }).unwrap();
        apply(&mut project, &Command::AddTrack { kind: TrackKind::Text }).unwrap();
        apply(&mut project, &Command::AddTrack { kind: TrackKind::Video }).unwrap();
        let lanes: Vec<Lane> = project.tracks.iter().map(|track| track.lane).collect();
        assert_eq!(lanes, vec![Lane::Overlay, Lane::Overlay, Lane::Main, Lane::Audio]);
        assert_eq!(project.tracks[0].kind, TrackKind::Video);
        let main = project.tracks[project.main_track_index()].id.clone();
        assert_eq!(
            apply(&mut project, &Command::RemoveTrack { track_id: main }),
            Err(EditorError::CannotRemoveMainTrack)
        );
        let audio = project.tracks[3].id.clone();
        apply(&mut project, &Command::RemoveTrack { track_id: audio }).unwrap();
        assert_eq!(project.tracks.len(), 3);
    }

    #[test]
    fn keyframes_follow_trims_and_splits() {
        let (mut project, id) = project_with_clip();
        let key = |time: f64, value: f64| Command::SetKeyframe {
            element_id: id.clone(),
            path: "opacity".to_owned(),
            time: seconds(time),
            value,
            segment: None,
        };
        apply(&mut project, &key(2.0, 0.0)).unwrap();
        apply(&mut project, &key(6.0, 1.0)).unwrap();
        apply(&mut project, &key(6.0, 0.5)).unwrap();
        let element = project.element(&id).unwrap().1.clone();
        let channel = &element.animations["opacity"];
        assert_eq!(channel.keys.len(), 2, "a key at the same time is replaced");
        assert_eq!(channel.keys[1].value, 0.5);
        assert!(element.is_animated("opacity"));
        assert_eq!(element.params_at(seconds(4.0)).number("opacity", 9.0), 0.25);

        // Trimming the start by 1 s keeps the keys on the same frames.
        apply(
            &mut project,
            &Command::ResizeElement {
                element_id: id.clone(),
                edge: Edge::Start,
                time: seconds(1.0),
            },
        )
        .unwrap();
        let element = project.element(&id).unwrap().1.clone();
        assert_eq!(element.animations["opacity"].keys[0].time, seconds(1.0));
        assert_eq!(element.params_at(seconds(4.0)).number("opacity", 9.0), 0.25);

        // Splitting keeps the motion continuous on both sides.
        let outcome = apply(
            &mut project,
            &Command::SplitElement {
                element_id: id.clone(),
                time: seconds(4.0),
            },
        )
        .unwrap();
        let right_id = outcome.element_refs()[1].element_id.clone();
        let left = project.element(&id).unwrap().1.clone();
        let right = project.element(&right_id).unwrap().1.clone();
        assert_eq!(left.params_at(seconds(3.0)).number("opacity", 9.0), 0.125);
        assert_eq!(right.params_at(seconds(4.0)).number("opacity", 9.0), 0.25);
        assert_eq!(right.params_at(seconds(5.0)).number("opacity", 9.0), 0.375);
        assert_eq!(right.params_at(seconds(9.0)).number("opacity", 9.0), 0.5);

        let keyframe_id = right.animations["opacity"].keys[0].id.clone();
        apply(
            &mut project,
            &Command::RemoveKeyframe {
                element_id: right_id.clone(),
                path: "opacity".to_owned(),
                keyframe_id,
            },
        )
        .unwrap();
        apply(
            &mut project,
            &Command::ClearAnimation {
                element_id: right_id.clone(),
                path: "opacity".to_owned(),
            },
        )
        .unwrap();
        let right = project.element(&right_id).unwrap().1.clone();
        assert!(!right.is_animated("opacity"));
        assert_eq!(right.params_at(seconds(5.0)).number("opacity", 9.0), 1.0);
        assert!(matches!(
            apply(
                &mut project,
                &Command::RemoveKeyframe {
                    element_id: right_id,
                    path: "opacity".to_owned(),
                    keyframe_id: "nope".to_owned(),
                },
            ),
            Err(EditorError::InvalidParams(_))
        ));
    }

    #[test]
    fn color_keyframes_animate_text_colour() {
        let (mut project, _) = project_with_clip();
        let text = Element::text("Hi", seconds(0.0));
        let id = text.id.clone();
        apply(&mut project, &Command::InsertElement { element: text, placement: TrackPlacement::Auto }).unwrap();
        let key = |time: f64, color: &str| Command::SetColorKeyframe {
            element_id: id.clone(),
            path: "color".to_owned(),
            time: seconds(time),
            color: color.to_owned(),
        };
        apply(&mut project, &key(0.0, "#000000")).unwrap();
        apply(&mut project, &key(2.0, "#ffffff")).unwrap();
        let element = project.element(&id).unwrap().1.clone();
        assert!(element.is_animated("color"));
        assert_eq!(element.params_at(seconds(1.0)).text("color", ""), "#bcbcbc");
        assert!(matches!(apply(&mut project, &key(1.0, "red")), Err(EditorError::InvalidParams(_))));
        // A number key cannot go on a colour, nor a colour on a number.
        assert!(matches!(
            apply(
                &mut project,
                &Command::SetKeyframe {
                    element_id: id.clone(),
                    path: "color".to_owned(),
                    time: seconds(1.0),
                    value: 1.0,
                    segment: None,
                },
            ),
            Err(EditorError::InvalidParams(_))
        ));
        assert!(matches!(
            apply(
                &mut project,
                &Command::SetColorKeyframe {
                    element_id: id.clone(),
                    path: "opacity".to_owned(),
                    time: seconds(1.0),
                    color: "#ffffff".to_owned(),
                },
            ),
            Err(EditorError::InvalidParams(_))
        ));
        assert!(!project.element(&id).unwrap().1.is_animated("opacity"));
        let keyframe_id = element.animations["color"].key_at(seconds(0.0)).unwrap().id.clone();
        apply(&mut project, &Command::RemoveKeyframe { element_id: id.clone(), path: "color".to_owned(), keyframe_id }).unwrap();
        let element = project.element(&id).unwrap().1.clone();
        assert_eq!(element.animations["color"].all_keys().count(), 4);
        apply(&mut project, &Command::ClearAnimation { element_id: id.clone(), path: "color".to_owned() }).unwrap();
        assert!(!project.element(&id).unwrap().1.is_animated("color"));
    }

    #[test]
    fn effects_and_masks_are_added_updated_and_removed() {
        let (mut project, id) = project_with_clip();
        let mut blur = Effect::blur();
        blur.id = "fx".to_owned();
        apply(
            &mut project,
            &Command::AddEffect {
                element_id: id.clone(),
                effect: blur.clone(),
            },
        )
        .unwrap();
        assert!(matches!(
            apply(
                &mut project,
                &Command::AddEffect {
                    element_id: id.clone(),
                    effect: blur,
                },
            ),
            Err(EditorError::InvalidParams(_))
        ));
        let mut params = Params::new();
        params.set("intensity", 40.0);
        apply(
            &mut project,
            &Command::UpdateEffect {
                element_id: id.clone(),
                effect_id: "fx".to_owned(),
                params: Some(params),
                enabled: Some(false),
            },
        )
        .unwrap();
        apply(
            &mut project,
            &Command::SetKeyframe {
                element_id: id.clone(),
                path: "effects.fx.params.intensity".to_owned(),
                time: seconds(0.0),
                value: 10.0,
                segment: None,
            },
        )
        .unwrap();
        let element = project.element(&id).unwrap().1.clone();
        let effect = element.effect("fx").unwrap();
        assert!(!effect.enabled);
        assert_eq!(effect.params.number("intensity", 0.0), 40.0);
        assert_eq!(
            element.effect_params_at(effect, seconds(3.0)).number("intensity", 0.0),
            10.0
        );
        apply(
            &mut project,
            &Command::RemoveEffect {
                element_id: id.clone(),
                effect_id: "fx".to_owned(),
            },
        )
        .unwrap();
        let element = project.element(&id).unwrap().1.clone();
        assert!(element.effects.is_empty());
        assert!(element.animations.is_empty(), "the effect's keyframes go with it");

        let mut mask = Mask::ellipse();
        mask.id = "m".to_owned();
        apply(
            &mut project,
            &Command::AddMask {
                element_id: id.clone(),
                mask,
            },
        )
        .unwrap();
        let mut params = Params::new();
        params.set("feather", 12.0).set("inverted", true);
        apply(
            &mut project,
            &Command::UpdateMask {
                element_id: id.clone(),
                mask_id: "m".to_owned(),
                params,
            },
        )
        .unwrap();
        let element = project.element(&id).unwrap().1.clone();
        let mask = element.mask("m").unwrap();
        assert_eq!(mask.params.number("feather", 0.0), 12.0);
        assert!(mask.params.bool("inverted", false));
        assert_eq!(mask.params.number("width", 0.0), 0.6, "other settings stay");
        apply(
            &mut project,
            &Command::RemoveMask {
                element_id: id.clone(),
                mask_id: "m".to_owned(),
            },
        )
        .unwrap();
        assert!(project.element(&id).unwrap().1.masks.is_empty());

        // Text cannot be masked; audio cannot take effects.
        let text = Element::text("Hi", seconds(20.0));
        let text_id = text.id.clone();
        apply(
            &mut project,
            &Command::InsertElement {
                element: text,
                placement: TrackPlacement::Auto,
            },
        )
        .unwrap();
        assert!(matches!(
            apply(
                &mut project,
                &Command::AddMask {
                    element_id: text_id,
                    mask: Mask::rectangle(),
                },
            ),
            Err(EditorError::InvalidParams(_))
        ));
    }

    #[test]
    fn nearest_free_start_finds_gaps() {
        let (project, _) = project_with_clip();
        let track = &project.tracks[0];
        // The clip occupies 0..10s; a 3 s element wanted at 2 s lands at 10 s.
        assert_eq!(
            nearest_free_start(track, seconds(3.0), seconds(2.0), None),
            Some(seconds(10.0))
        );
        assert_eq!(
            nearest_free_start(track, seconds(3.0), seconds(12.0), None),
            Some(seconds(12.0))
        );
    }
}
