//! Project interchange (row 111): the project's timeline as the shared
//! `interchange` crate's neutral [`Timeline`] for export to OpenTimelineIO,
//! FCPXML or a CMX 3600 EDL, and an imported timeline back as commands (one
//! undo step). The web editor maps its project the same way
//! (`apps/web/src/interchange`).
//!
//! Export takes video, picture and audio clips with their place, trims,
//! constant speed, hidden state, whether a video clip's sound plays, and
//! transitions (wipes as wipes, every other kind as a dissolve). Everything
//! else is listed in the timeline's notes.
//!
//! Import puts each imported track on a new track (video tracks above the
//! existing ones, bottom first, so the stacking is kept; audio tracks at
//! the bottom), or onto the main track when the project's timeline is
//! empty. A clip whose file the caller cannot find is skipped and its file
//! named in `missing`.

use interchange::{Clip, Rate, Timeline, Track as ITrack, TrackKind as ITrackKind, Transition as ITransition};

use crate::{Command, Element, ElementKind, Lane, MediaItem, MediaKind, Project, Retime, TrackKind, TrackPlacement, Transition};
use time::MediaTime;

pub const NOTE_TEXT: &str = "Text, captions, stickers and graphics were left out.";
pub const NOTE_COMPOUND: &str = "Compound clips were left out.";
pub const NOTE_LOOK: &str = "Effects, masks, keyframes and changes of position, size and opacity were left out.";
pub const NOTE_SPEED: &str = "Speed ramps and reversed clips went out as a constant forward speed.";
pub const NOTE_TRANSITION: &str = "Transitions other than dissolves and wipes went out as dissolves.";
pub const NOTE_FADE_IN: &str = "Transitions in from nothing were left out.";

fn frames(rate: Rate, time: MediaTime) -> i64 {
    rate.frames(time.to_seconds_f64())
}

fn changes_look(element: &Element) -> bool {
    let p = &element.params;
    !element.effects.is_empty()
        || !element.masks.is_empty()
        || !element.animations.is_empty()
        || element.cutout.is_some()
        || p.number("transform.positionX", 0.0).abs() > 1e-6
        || p.number("transform.positionY", 0.0).abs() > 1e-6
        || (p.number("transform.scaleX", 1.0) - 1.0).abs() > 1e-6
        || (p.number("transform.scaleY", 1.0) - 1.0).abs() > 1e-6
        || p.number("transform.rotate", 0.0).abs() > 1e-6
        || (p.number("opacity", 1.0) - 1.0).abs() > 1e-6
}

/// The project's timeline for export (`notes` say what was left out).
pub fn project_timeline(project: &Project) -> Timeline {
    let rate = Rate::new(project.settings.fps.numerator, project.settings.fps.denominator);
    let mut timeline = Timeline {
        name: project.name.clone(),
        rate,
        width: project.settings.canvas.width,
        height: project.settings.canvas.height,
        media: Vec::new(),
        tracks: Vec::new(),
        notes: Vec::new(),
    };
    let mut used: Vec<&str> = Vec::new();
    // Video tracks from the bottom: the main track, then the overlays upwards.
    let main = project.main_track_index();
    let mut video: Vec<usize> = vec![main];
    video.extend((0..project.tracks.len()).rev().filter(|&i| i != main && project.tracks[i].lane == Lane::Overlay));
    let audio: Vec<usize> = (0..project.tracks.len()).filter(|&i| project.tracks[i].lane == Lane::Audio).collect();
    for (kind, indices) in [(ITrackKind::Video, video), (ITrackKind::Audio, audio)] {
        for index in indices {
            let track = &project.tracks[index];
            if track.kind == TrackKind::Text {
                if !track.elements.is_empty() {
                    timeline.note(NOTE_TEXT);
                }
                continue;
            }
            let mut out = ITrack { kind, name: track.name.clone(), clips: Vec::new() };
            let mut elements: Vec<&Element> = track.elements.iter().collect();
            elements.sort_by_key(|element| element.start_time);
            for (i, element) in elements.iter().enumerate() {
                let media_id = match &element.kind {
                    ElementKind::Video { media_id } | ElementKind::Image { media_id } | ElementKind::Audio { media_id } => media_id,
                    ElementKind::Compound { .. } => {
                        timeline.note(NOTE_COMPOUND);
                        continue;
                    }
                    _ => {
                        timeline.note(NOTE_TEXT);
                        continue;
                    }
                };
                let Some(media) = project.media_item(media_id) else { continue };
                if !used.contains(&media_id.as_str()) {
                    used.push(media_id);
                }
                if changes_look(element) {
                    timeline.note(NOTE_LOOK);
                }
                let duration = frames(rate, element.duration);
                let speed = match &element.retime {
                    Some(retime) if !retime.is_identity() => {
                        if retime.reverse || !retime.ramp.is_empty() {
                            timeline.note(NOTE_SPEED);
                        }
                        let span = element.source_span_seconds();
                        let length = element.duration.to_seconds_f64();
                        if length > 0.0 { (span / length * 1e6).round() / 1e6 } else { 1.0 }
                    }
                    _ => 1.0,
                };
                if element.transition_in.is_some() {
                    timeline.note(NOTE_FADE_IN);
                }
                let next_starts_here = elements.get(i + 1).is_some_and(|next| next.start_time == element.end_time());
                let transition = element.transition.as_ref().filter(|_| next_starts_here).map(|t| {
                    let wipe = t.kind.contains("Wipe");
                    if !wipe && t.kind != "Fade" {
                        timeline.note(NOTE_TRANSITION);
                    }
                    ITransition { frames: frames(rate, t.duration).max(1), kind: if wipe { "wipe" } else { "dissolve" }.to_owned() }
                });
                let sound = match &element.kind {
                    ElementKind::Audio { .. } => true,
                    ElementKind::Video { .. } => media.has_audio && !element.params.bool("muted", false),
                    _ => false,
                };
                out.clips.push(Clip {
                    name: element.name.clone(),
                    media_id: media.id.clone(),
                    start: frames(rate, element.start_time),
                    duration,
                    source_start: frames(rate, element.trim_start),
                    speed,
                    sound,
                    enabled: !element.hidden,
                    transition,
                });
            }
            if !out.clips.is_empty() || kind == ITrackKind::Video && timeline.tracks.is_empty() {
                timeline.tracks.push(out);
            }
        }
    }
    for media in project.media.iter().filter(|m| used.contains(&m.id.as_str())) {
        timeline.media.push(interchange::Media {
            id: media.id.clone(),
            name: media.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| media.name.clone()),
            url: media.path.to_string_lossy().replace('\\', "/"),
            duration: media.duration.map(|d| frames(rate, d)),
            has_video: matches!(media.kind, MediaKind::Video | MediaKind::Image),
            has_audio: media.has_audio || media.kind == MediaKind::Audio,
            still: media.kind == MediaKind::Image,
            width: media.width,
            height: media.height,
        });
    }
    timeline
}

/// An imported timeline as commands.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineImport {
    /// One [`Command::Batch`], or `None` when nothing could be placed.
    pub command: Option<Command>,
    pub clips: usize,
    pub tracks: usize,
    /// Files no media in the project matched, by name.
    pub missing: Vec<String>,
}

/// The media item among `items` that a timeline's file is: the same path,
/// else the same file name (letter case ignored).
pub fn find_media<'a>(items: &'a [MediaItem], media: &interchange::Media) -> Option<&'a MediaItem> {
    let normal = |text: &str| text.replace('\\', "/").to_lowercase();
    let wanted = normal(&media.url);
    items
        .iter()
        .find(|item| normal(&item.path.to_string_lossy()) == wanted)
        .or_else(|| {
            let name = media.name.to_lowercase();
            items.iter().find(|item| {
                item.path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == name) || item.name.to_lowercase() == name
            })
        })
}

fn seconds(rate: Rate, frames: i64) -> MediaTime {
    MediaTime::from_seconds_f64(rate.seconds(frames).max(0.0)).unwrap_or(MediaTime::ZERO)
}

fn element_for(clip: &Clip, item: &MediaItem, rate: Rate, audio: bool) -> Option<Element> {
    let start = seconds(rate, clip.start);
    // A video file's sound alone, on an audio track.
    let mut element = if audio && item.kind != MediaKind::Audio {
        let mut as_audio = item.clone();
        as_audio.kind = MediaKind::Audio;
        let mut element = Element::from_media(&as_audio, start);
        element.kind = ElementKind::Audio { media_id: item.id.clone() };
        element
    } else {
        Element::from_media(item, start)
    };
    if !clip.name.trim().is_empty() {
        element.name = clip.name.clone();
    }
    element.duration = seconds(rate, clip.duration);
    if element.duration < MediaTime::from_ticks(1200) {
        return None;
    }
    let speed = clip.speed.abs();
    if !matches!(element.kind, ElementKind::Image { .. }) {
        element.trim_start = seconds(rate, clip.source_start.max(0));
        let span = MediaTime::from_seconds_f64(element.duration.to_seconds_f64() * speed).unwrap_or(element.duration);
        if let Some(source) = element.source_duration {
            element.trim_end = (source - element.trim_start - span).max(MediaTime::ZERO);
        }
        if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
            let mut retime = Retime::constant(speed);
            retime.reverse = clip.speed < 0.0;
            element.retime = Some(retime);
        }
    }
    element.hidden = !clip.enabled;
    if matches!(element.kind, ElementKind::Video { .. }) && item.has_audio && !clip.sound {
        element.params.set("muted", true);
    }
    element.transition = clip.transition.as_ref().map(|t| Transition {
        kind: if t.kind == "wipe" { "DirectionalWipe" } else { "Fade" }.to_owned(),
        duration: seconds(rate, t.frames),
        params: crate::Params::new(),
    });
    Some(element)
}

/// The commands that put `timeline` into `project`. `media_for` finds (or
/// brings in) each file; a media item it returns that the project does not
/// have yet is added first, in the same undo step.
pub fn timeline_import(project: &Project, timeline: &Timeline, mut media_for: impl FnMut(&interchange::Media) -> Option<MediaItem>) -> TimelineImport {
    let rate = timeline.rate;
    let mut commands = Vec::new();
    let mut added: Vec<MediaItem> = Vec::new();
    let mut resolved: Vec<(String, Option<MediaItem>)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for media in &timeline.media {
        let item = media_for(media);
        match &item {
            Some(item) if project.media_item(&item.id).is_none() && !added.iter().any(|a| a.id == item.id) => {
                added.push(item.clone());
                commands.push(Command::AddMedia(item.clone()));
            }
            Some(_) => {}
            None => {}
        }
        resolved.push((media.id.clone(), item));
    }
    let empty = project.tracks.iter().all(|track| track.elements.is_empty());
    let main_id = project.tracks[project.main_track_index()].id.clone();
    let (mut clips, mut tracks) = (0, 0);
    for (index, track) in timeline.tracks.iter().enumerate() {
        let audio = track.kind == ITrackKind::Audio;
        let mut elements = Vec::new();
        for clip in &track.clips {
            let Some((_, item)) = resolved.iter().find(|(id, _)| *id == clip.media_id) else { continue };
            let Some(item) = item else {
                if let Some(media) = timeline.media(&clip.media_id)
                    && !missing.contains(&media.name)
                {
                    missing.push(media.name.clone());
                }
                continue;
            };
            if let Some(element) = element_for(clip, item, rate, audio) {
                elements.push(element);
            }
        }
        if elements.is_empty() {
            continue;
        }
        // A transition needs its next clip, which may not have come in.
        for i in 0..elements.len() {
            let next = elements.get(i + 1).map(|e| e.start_time);
            if next != Some(elements[i].end_time()) {
                elements[i].transition = None;
            }
        }
        clips += elements.len();
        tracks += 1;
        if !audio && index == 0 && empty {
            for element in elements {
                commands.push(Command::InsertElement { element, placement: TrackPlacement::Track(main_id.clone()) });
            }
        } else {
            commands.push(Command::InsertOnNewTrack {
                kind: if audio { TrackKind::Audio } else { TrackKind::Video },
                name: track.name.clone(),
                elements,
            });
        }
    }
    TimelineImport { command: (clips > 0).then(|| Command::Batch(commands)), clips, tracks, missing }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply;

    fn seconds_of(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn project() -> Project {
        let mut project = Project::new("Interchange");
        for (id, kind, audio) in [("take", MediaKind::Video, true), ("logo", MediaKind::Image, false), ("song", MediaKind::Audio, true)] {
            apply(
                &mut project,
                &Command::AddMedia(MediaItem {
                    id: id.into(),
                    name: id.into(),
                    kind,
                    path: format!("C:/Clips/{id}.{}", if kind == MediaKind::Image { "png" } else { "mp4" }).into(),
                    duration: (kind != MediaKind::Image).then(|| seconds_of(20.0)),
                    width: Some(1920),
                    height: Some(1080),
                    fps: Some(30.0),
                    has_audio: audio,
                }),
            )
            .unwrap();
        }
        let mut a = Element::from_media(project.media_item("take").unwrap(), seconds_of(0.0));
        a.trim_start = seconds_of(1.0);
        a.duration = seconds_of(3.0);
        a.transition = Some(Transition { kind: "Fade".into(), duration: seconds_of(0.5), params: crate::Params::new() });
        let mut b = Element::from_media(project.media_item("take").unwrap(), seconds_of(3.0));
        b.trim_start = seconds_of(10.0);
        b.duration = seconds_of(2.0);
        b.retime = Some(Retime::constant(2.0));
        b.params.set("muted", true);
        let mut logo = Element::from_media(project.media_item("logo").unwrap(), seconds_of(1.0));
        logo.duration = seconds_of(2.0);
        let mut song = Element::from_media(project.media_item("song").unwrap(), seconds_of(0.0));
        song.duration = seconds_of(5.0);
        for element in [a, b] {
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        apply(&mut project, &Command::InsertOnNewTrack { kind: TrackKind::Video, name: "Logo".into(), elements: vec![logo] }).unwrap();
        apply(&mut project, &Command::InsertElement { element: song, placement: TrackPlacement::Auto }).unwrap();
        apply(&mut project, &Command::InsertElement { element: Element::text("Hello", seconds_of(0.0)), placement: TrackPlacement::Auto }).unwrap();
        project
    }

    #[test]
    fn exports_clips_trims_speed_sound_and_dissolves() {
        let timeline = project_timeline(&project());
        assert_eq!(timeline.rate, Rate::new(30, 1));
        let kinds: Vec<_> = timeline.tracks.iter().map(|t| (t.kind, t.clips.len())).collect();
        assert_eq!(kinds, vec![(ITrackKind::Video, 2), (ITrackKind::Video, 1), (ITrackKind::Audio, 1)], "main, then the logo above, then audio");
        let main = &timeline.tracks[0].clips;
        assert_eq!((main[0].start, main[0].duration, main[0].source_start, main[0].sound), (0, 90, 30, true));
        assert_eq!(main[0].transition, Some(ITransition { frames: 15, kind: "dissolve".into() }));
        assert_eq!((main[1].start, main[1].source_start, main[1].speed, main[1].sound), (90, 300, 2.0, false));
        assert!(timeline.notes.contains(&NOTE_TEXT.to_owned()), "{:?}", timeline.notes);
        assert_eq!(timeline.media.len(), 3);
        assert!(timeline.media.iter().any(|m| m.still && m.name == "logo.png"));
    }

    #[test]
    fn round_trips_through_every_format_into_an_empty_project() {
        let source = project();
        let timeline = project_timeline(&source);
        for format in [interchange::Format::Otio, interchange::Format::Fcpxml, interchange::Format::Edl] {
            let text = interchange::write(&timeline, format);
            let back = interchange::read(&text, format, timeline.rate).unwrap();
            let mut target = Project::new("Imported");
            for media in &source.media {
                apply(&mut target, &Command::AddMedia(media.clone())).unwrap();
            }
            let import = timeline_import(&target, &back, |m| find_media(&target.media, m).cloned());
            assert!(import.missing.is_empty(), "{format:?}: {:?}", import.missing);
            apply(&mut target, &import.command.unwrap()).unwrap();
            let main = &target.tracks[target.main_track_index()].elements;
            assert_eq!(main.len(), 2, "{format:?}");
            assert_eq!((main[0].trim_start, main[0].duration), (seconds_of(1.0), seconds_of(3.0)), "{format:?}");
            assert_eq!(main[0].transition.as_ref().map(|t| (t.kind.as_str(), t.duration)), Some(("Fade", seconds_of(0.5))), "{format:?}");
            assert_eq!(main[1].retime.as_ref().map(|r| r.rate), Some(2.0), "{format:?}");
            assert!(main[1].params.bool("muted", false), "{format:?}: the second clip's sound stays off");
            let overlays = target.tracks.iter().filter(|t| t.lane == Lane::Overlay).count();
            let audio: usize = target.tracks.iter().filter(|t| t.kind == TrackKind::Audio).map(|t| t.elements.len()).sum();
            let (want_overlays, want_audio) = if format == interchange::Format::Edl { (0, 1) } else { (1, 1) };
            assert_eq!((overlays, audio), (want_overlays, want_audio), "{format:?}");
        }
    }

    #[test]
    fn names_missing_files_and_imports_the_rest_above() {
        let source = project();
        let timeline = project_timeline(&source);
        let mut target = source.clone();
        let import = timeline_import(&target, &timeline, |m| (m.name != "song.mp4").then(|| find_media(&target.media, m).cloned()).flatten());
        assert_eq!(import.missing, vec!["song.mp4".to_owned()]);
        // Without its next clip a transition is dropped.
        let only_first = timeline_import(&Project::new("x"), &{
            let mut t = timeline.clone();
            t.tracks[0].clips.truncate(1);
            t
        }, |m| find_media(&target.media, m).cloned());
        let Some(Command::Batch(commands)) = only_first.command else { panic!("a batch") };
        assert!(commands.iter().all(|c| !matches!(c, Command::InsertElement { element, .. } if element.transition.is_some())));
        let tracks_before = target.tracks.len();
        apply(&mut target, &import.command.unwrap()).unwrap();
        assert_eq!(target.tracks.len(), tracks_before + 2, "the main track and the logo come in on new tracks; the song is missing");
    }
}
