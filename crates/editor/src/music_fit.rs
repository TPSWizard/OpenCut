//! Fit music to a length (row 101) on the editor's tracks, through the
//! shared `musicfit` crate: an audio clip becomes pieces of the same
//! source, back to back on its track, cut or looped at bar lines that
//! sound alike, each crossfading into the next (the `crossfade` param,
//! equal power). The first piece keeps the clip's id. The web editor's
//! `@/music-fit/fit` is the same.

use time::MediaTime;

use crate::audio_crossfade::{CROSSFADE_CURVE_PARAM, CROSSFADE_PARAM};
use crate::commands::{Command, ElementPatch, TrackPlacement};
use crate::document::{Element, ElementKind, Project};
use crate::params::Params;

pub use musicfit::{Fit as MusicFit, FitError as MusicFitError, PlanKind as MusicFitKind, SAMPLE_RATE as MUSIC_FIT_SAMPLE_RATE};

/// A clip music fitting works on: audio without a speed change.
pub fn can_fit_music(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Audio { .. }) && !element.retime.as_ref().is_some_and(|retime| !retime.is_identity())
}

/// The clip as the shared crate takes it.
pub fn music_fit_clip(element: &Element) -> musicfit::Clip {
    let (fade_in, fade_out) = crate::clip_fades(&element.params, element.duration.to_seconds_f64());
    musicfit::Clip {
        start: element.start_time.as_ticks(),
        duration: element.duration.as_ticks(),
        trim_start: element.trim_start.as_ticks(),
        trim_end: element.trim_end.as_ticks(),
        fade_in,
        fade_out,
    }
}

/// The length the music would need to end with everything else on the
/// timeline (the video): from its start to the latest end of any other
/// element. `None` when nothing else ends a second or more after it starts.
pub fn music_fit_default_length(project: &Project, element_id: &str) -> Option<MediaTime> {
    let (_, element) = project.element(element_id)?;
    let end = project
        .tracks
        .iter()
        .flat_map(|track| &track.elements)
        .filter(|other| other.id != element_id)
        .map(Element::end_time)
        .max()?;
    let length = end - element.start_time;
    (length.as_ticks() >= time::TICKS_PER_SECOND).then_some(length)
}

/// The fitted pieces as one undo step: the clip becomes the first piece
/// and the rest are inserted after it on its track. Fails when the track
/// is locked or another clip on it is in the way.
pub fn music_fit_command(project: &Project, element_id: &str, fit: &MusicFit) -> Result<Command, String> {
    let (track, element) = project.element(element_id).ok_or("The clip is no longer on the timeline")?;
    if track.locked {
        return Err("The track is locked".to_owned());
    }
    if !can_fit_music(element) {
        return Err("Fit to length works on audio clips without a speed change".to_owned());
    }
    let Some(last) = fit.pieces.last() else { return Err("Nothing to fit".to_owned()) };
    let end = last.start + last.duration;
    let blocked = track.elements.iter().any(|other| {
        other.id != element_id && other.start_time.as_ticks() < end && other.end_time().as_ticks() > element.start_time.as_ticks()
    });
    if blocked {
        return Err("Another clip on this track is in the way; move it or put the music on its own track".to_owned());
    }
    let piece_params = |piece: &musicfit::Piece| {
        let mut params = Params::new();
        params.set("fadeIn", piece.fade_in);
        params.set("fadeOut", piece.fade_out);
        if piece.crossfade > 0 {
            params.set(CROSSFADE_PARAM, piece.crossfade as f64 / time::TICKS_PER_SECOND as f64);
            params.set(CROSSFADE_CURVE_PARAM, "equalPower");
        }
        params
    };
    let mut commands = Vec::with_capacity(fit.pieces.len());
    for (index, piece) in fit.pieces.iter().enumerate() {
        // The last piece keeps any crossfade the clip had into what follows.
        if index == 0 {
            let params = piece_params(piece);
            commands.push(Command::UpdateElement {
                element_id: element_id.to_owned(),
                patch: ElementPatch {
                    start_time: Some(MediaTime::from_ticks(piece.start)),
                    duration: Some(MediaTime::from_ticks(piece.duration)),
                    trim_start: Some(MediaTime::from_ticks(piece.trim_start)),
                    trim_end: Some(MediaTime::from_ticks(piece.trim_end)),
                    params: Some(params),
                    ..Default::default()
                },
            });
            continue;
        }
        let mut copy = element.clone();
        copy.id = crate::new_id();
        copy.start_time = MediaTime::from_ticks(piece.start);
        copy.duration = MediaTime::from_ticks(piece.duration);
        copy.trim_start = MediaTime::from_ticks(piece.trim_start);
        copy.trim_end = MediaTime::from_ticks(piece.trim_end);
        copy.params.merge(&piece_params(piece));
        commands.push(Command::InsertElement { element: copy, placement: TrackPlacement::Track(track.id.clone()) });
    }
    Ok(Command::Batch(commands))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::apply;
    use crate::document::{Lane, MediaItem, MediaKind, Track, TrackKind};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn replaces_the_clip_with_its_pieces_in_one_step() {
        let mut project = Project::new("Fit");
        let item = MediaItem {
            id: "m-song".to_owned(),
            name: "song".to_owned(),
            kind: MediaKind::Audio,
            path: "song.wav".into(),
            duration: Some(seconds(30.0)),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        let mut element = Element::from_media(&item, seconds(1.0));
        element.id = "music".to_owned();
        element.duration = seconds(20.0);
        element.trim_end = seconds(10.0);
        element.params.set("volume", 0.5);
        project.media.push(item);
        let mut track = Track::new(TrackKind::Audio, Lane::Audio, "Music");
        track.elements.push(element);
        project.tracks.push(track);
        let mut other = Track::new(TrackKind::Text, Lane::Overlay, "Titles");
        let mut title = Element::sticker("", "Title", seconds(0.0));
        title.id = "title".to_owned();
        title.duration = seconds(25.0);
        other.elements.push(title);
        project.tracks.push(other);

        assert_eq!(music_fit_default_length(&project, "music"), Some(seconds(24.0)));
        let element = &project.element("music").unwrap().1.clone();
        assert!(can_fit_music(element));
        let clip = music_fit_clip(element);
        let plan = musicfit::Plan {
            kind: musicfit::PlanKind::Loop,
            jumps: Vec::new(),
            segments: vec![(0.0, 12.0), (8.0, 20.0)],
            trimmed: 0.0,
        };
        let pieces = musicfit::layout(&plan, &clip, seconds(24.0).as_ticks());
        let fit = MusicFit { kind: plan.kind, pieces, jumps: Vec::new(), bpm: Some(120.0), trimmed: 0.0 };
        let command = music_fit_command(&project, "music", &fit).unwrap();
        apply(&mut project, &command).unwrap();
        let track = project.element("music").unwrap().0;
        assert_eq!(track.elements.len(), 2);
        let first = track.elements.iter().find(|e| e.id == "music").unwrap();
        let second = track.elements.iter().find(|e| e.id != "music").unwrap();
        assert_eq!((first.start_time, first.duration, first.trim_start), (seconds(1.0), seconds(12.0), seconds(0.0)));
        assert_eq!((second.start_time, second.duration, second.trim_start, second.trim_end), (seconds(13.0), seconds(12.0), seconds(8.0), seconds(10.0)));
        assert_eq!(first.params.number(CROSSFADE_PARAM, 0.0), 0.1);
        assert_eq!(second.params.number("volume", 0.0), 0.5);
        assert_eq!(second.params.number(CROSSFADE_PARAM, 0.0), 0.0);

        // Something in the way on the track.
        let mut blocked = project.clone();
        let (track_index, _) = blocked.locate("music").unwrap();
        let mut extra = blocked.tracks[track_index].elements[0].clone();
        extra.id = "after".to_owned();
        extra.start_time = seconds(40.0);
        blocked.tracks[track_index].elements.push(extra);
        let element = blocked.element("music").unwrap().1.clone();
        let pieces = musicfit::layout(&plan, &music_fit_clip(&element), seconds(45.0).as_ticks());
        let fit = MusicFit { kind: plan.kind, pieces, jumps: Vec::new(), bpm: None, trimmed: 0.0 };
        assert!(music_fit_command(&blocked, "music", &fit).is_err());
    }
}
