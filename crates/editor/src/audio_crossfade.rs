//! Audio crossfades on the editor's tracks, through the shared `crossfade`
//! crate: a clip's `crossfade` param (seconds) and `crossfadeCurve`
//! (`linear` or `equalPower`) join it to the next audio or video clip on
//! its track. Playback stretches both clips over the cut into their source
//! handles ([`crossfaded_elements`], used by [`crate::plan_audio`]); the
//! one-step command ([`crossfade_commands`]) trims to make the handles when
//! they are short. The web editor's `audio/crossfade.ts` is the same.

use time::MediaTime;

use crate::commands::{Command, ElementPatch};
use crate::document::{Element, ElementKind, Project, Track};
use crate::params::Params;

pub use crossfade::Curve as CrossfadeCurve;

/// The outgoing clip's crossfade length, seconds.
pub const CROSSFADE_PARAM: &str = "crossfade";
/// `linear` or `equalPower`.
pub const CROSSFADE_CURVE_PARAM: &str = "crossfadeCurve";
/// Longest crossfade, seconds.
pub const CROSSFADE_MAX_SECONDS: f64 = 10.0;

fn seconds_to_ticks(seconds: f64) -> i64 {
    if seconds.is_finite() { (seconds.clamp(0.0, CROSSFADE_MAX_SECONDS) * time::TICKS_PER_SECOND as f64).round() as i64 } else { 0 }
}

/// A clip's crossfade into the next one: its length in seconds (0 for none)
/// and curve (equal power unless `crossfadeCurve` says `linear`).
pub fn clip_crossfade(params: &Params) -> (f64, CrossfadeCurve) {
    let seconds = params.number(CROSSFADE_PARAM, 0.0);
    let seconds = if seconds.is_finite() { seconds.clamp(0.0, CROSSFADE_MAX_SECONDS) } else { 0.0 };
    let curve = params.get(CROSSFADE_CURVE_PARAM).map_or(CrossfadeCurve::EqualPower, |value| {
        if value.as_text() == "linear" { CrossfadeCurve::Linear } else { CrossfadeCurve::EqualPower }
    });
    (seconds, curve)
}

fn track_clip(element: &Element) -> crossfade::TrackClip {
    let (fade_in, fade_out) = crate::clip_fades(&element.params, element.duration.to_seconds_f64());
    let (seconds, curve) = clip_crossfade(&element.params);
    crossfade::TrackClip {
        start: element.start_time.as_ticks(),
        duration: element.duration.as_ticks(),
        trim_start: element.trim_start.as_ticks(),
        trim_end: element.trim_end.as_ticks(),
        retimed: element.retime.as_ref().is_some_and(|retime| !retime.is_identity()),
        eligible: !element.hidden && matches!(element.kind, ElementKind::Video { .. } | ElementKind::Audio { .. }),
        fade_in,
        fade_out,
        crossfade: seconds_to_ticks(seconds),
        curve,
    }
}

/// The track's clips for the shared crate, in the track's order.
pub fn track_clips(track: &Track) -> Vec<crossfade::TrackClip> {
    track.elements.iter().map(track_clip).collect()
}

/// A track's elements as they play with their crossfades: a joined clip is
/// stretched over the cut (start, length and trims), with its fades in
/// `fadeIn` / `fadeOut` and their curves in `fadeInCurve` / `fadeOutCurve`.
/// Elements no crossfade touches come back unchanged.
pub fn crossfaded_elements(track: &Track) -> Vec<Element> {
    let played = crossfade::crossfade_track(&track_clips(track));
    track
        .elements
        .iter()
        .zip(played)
        .map(|(element, played)| {
            let mut element = element.clone();
            if let Some(played) = played {
                element.start_time = MediaTime::from_ticks(played.start);
                element.duration = MediaTime::from_ticks(played.duration);
                element.trim_start = MediaTime::from_ticks(played.trim_start);
                element.trim_end = MediaTime::from_ticks(played.trim_end);
                element.params.set("fadeIn", played.fade_in);
                element.params.set("fadeOut", played.fade_out);
                element.params.set("fadeInCurve", played.fade_in_curve.as_param());
                element.params.set("fadeOutCurve", played.fade_out_curve.as_param());
            }
            element
        })
        .collect()
}

/// The fade curve a `fadeInCurve` / `fadeOutCurve` param names.
pub fn fade_curve(params: &Params, key: &str) -> CrossfadeCurve {
    params.get(key).map_or(CrossfadeCurve::Linear, |value| CrossfadeCurve::from_param(&value.as_text()))
}

/// What a crossfade made in one step did.
#[derive(Clone, Debug, PartialEq)]
pub struct CrossfadeOutcome {
    pub command: Command,
    /// The crossfade's length, seconds.
    pub seconds: f64,
    /// How much was trimmed off the two clips together to make room (the
    /// incoming clip and those after it on the track move this much left).
    pub trimmed_seconds: f64,
}

/// The one-step crossfade of `seconds` from `element_id` into the next
/// clip on its track, as one undo step: trims that make handles where the
/// clips have too little spare audio, the clips after moved left to close
/// the gap, and the crossfade stored on the outgoing clip.
pub fn crossfade_commands(project: &Project, element_id: &str, seconds: f64, curve: CrossfadeCurve) -> Result<CrossfadeOutcome, String> {
    let (track_index, element_index) = project.locate(element_id).ok_or("The clip is no longer on the timeline")?;
    let track = &project.tracks[track_index];
    if track.locked {
        return Err("The track is locked".to_owned());
    }
    let clips = track_clips(track);
    let plan = crossfade::plan(&clips, element_index, seconds_to_ticks(seconds)).map_err(|error| error.message().to_owned())?;
    let mut params = Params::new();
    params.set(CROSSFADE_PARAM, plan.length as f64 / time::TICKS_PER_SECOND as f64);
    params.set(CROSSFADE_CURVE_PARAM, curve.as_param());
    let edit = |id: &str, edit: &crossfade::ClipEdit, params: Option<Params>| Command::UpdateElement {
        element_id: id.to_owned(),
        patch: ElementPatch {
            start_time: Some(MediaTime::from_ticks(edit.start)),
            duration: Some(MediaTime::from_ticks(edit.duration)),
            trim_start: Some(MediaTime::from_ticks(edit.trim_start)),
            trim_end: Some(MediaTime::from_ticks(edit.trim_end)),
            params,
            ..Default::default()
        },
    };
    let mut commands = vec![
        edit(element_id, &plan.from_edit, Some(params)),
        edit(&track.elements[plan.to].id, &plan.to_edit, None),
    ];
    for index in &plan.moved {
        let element = &track.elements[*index];
        commands.push(Command::UpdateElement {
            element_id: element.id.clone(),
            patch: ElementPatch { start_time: Some(element.start_time - MediaTime::from_ticks(plan.shift)), ..Default::default() },
        });
    }
    Ok(CrossfadeOutcome {
        command: Command::Batch(commands),
        seconds: plan.length as f64 / time::TICKS_PER_SECOND as f64,
        trimmed_seconds: plan.shift as f64 / time::TICKS_PER_SECOND as f64,
    })
}

/// Removes a clip's crossfade (its trims stay).
pub fn remove_crossfade_command(element_id: &str) -> Command {
    let mut params = Params::new();
    params.set(CROSSFADE_PARAM, 0.0);
    Command::UpdateElement { element_id: element_id.to_owned(), patch: ElementPatch { params: Some(params), ..Default::default() } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{TrackPlacement, apply};
    use crate::document::{MediaItem, MediaKind};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    /// Two whole 4 s audio files butted together, and a 2 s one after.
    fn project() -> (Project, Vec<String>) {
        let mut project = Project::new("Crossfade");
        let mut ids = Vec::new();
        for (index, start, length) in [(0, 0.0, 4.0), (1, 4.0, 4.0), (2, 8.0, 2.0)] {
            let media = MediaItem {
                id: format!("m{index}"),
                name: format!("m{index}"),
                kind: MediaKind::Audio,
                path: format!("tone{index}.wav").into(),
                duration: Some(seconds(length)),
                width: None,
                height: None,
                fps: None,
                has_audio: true,
            };
            apply(&mut project, &Command::AddMedia(media.clone())).unwrap();
            let mut element = Element::from_media(&media, seconds(start));
            element.id = format!("a{index}");
            ids.push(element.id.clone());
            let placement = if index == 0 { TrackPlacement::Auto } else { TrackPlacement::Track(project.element("a0").unwrap().0.id.clone()) };
            apply(&mut project, &Command::InsertElement { element, placement }).unwrap();
        }
        (project, ids)
    }

    #[test]
    fn one_step_trims_ripples_and_plays_both_through_the_join() {
        let (mut project, ids) = project();
        let outcome = crossfade_commands(&project, &ids[0], 1.0, CrossfadeCurve::EqualPower).unwrap();
        assert_eq!((outcome.seconds, outcome.trimmed_seconds), (1.0, 1.0));
        apply(&mut project, &outcome.command).unwrap();
        let track = project.element(&ids[0]).unwrap().0;
        let times: Vec<(f64, f64)> =
            track.elements.iter().map(|element| (element.start_time.to_seconds_f64(), element.duration.to_seconds_f64())).collect();
        assert_eq!(times, vec![(0.0, 3.5), (3.5, 3.5), (7.0, 2.0)]);
        assert_eq!(clip_crossfade(&track.elements[0].params), (1.0, CrossfadeCurve::EqualPower));

        // Playback: the first runs to 4 s (its whole file), the second starts at 3 s (its whole file).
        let plans = crate::plan_audio(&project);
        let first = plans.iter().find(|plan| plan.element_id == ids[0]).unwrap();
        let second = plans.iter().find(|plan| plan.element_id == ids[1]).unwrap();
        assert_eq!((first.timeline_start.to_seconds_f64(), first.timeline_end().to_seconds_f64()), (0.0, 4.0));
        assert_eq!((first.source_start.to_seconds_f64(), first.source_end.to_seconds_f64()), (0.0, 4.0));
        assert_eq!((second.timeline_start.to_seconds_f64(), second.source_start.to_seconds_f64()), (3.0, 0.0));
        assert_eq!((first.fade_out_seconds, second.fade_in_seconds), (1.0, 1.0));
        // Equal power through the join: the powers add up to 1.
        for step in 0..=10 {
            let time = 3.0 + step as f64 / 10.0;
            let (out, into) = (first.gain_at_seconds(time), second.gain_at_seconds(time));
            assert!((out * out + into * into - 1.0).abs() < 1e-6, "{time}: {out} {into}");
        }
        assert!((first.gain_at_seconds(3.5) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6);

        // Removing it leaves a plain cut.
        apply(&mut project, &remove_crossfade_command(&ids[0])).unwrap();
        let plans = crate::plan_audio(&project);
        let first = plans.iter().find(|plan| plan.element_id == ids[0]).unwrap();
        assert_eq!((first.timeline_end().to_seconds_f64(), first.fade_out_seconds), (3.5, 0.0));
    }

    #[test]
    fn refuses_without_a_neighbour() {
        let (project, ids) = project();
        let error = crossfade_commands(&project, &ids[2], 1.0, CrossfadeCurve::Linear).unwrap_err();
        assert!(error.contains("No audio or video clip"), "{error}");
    }
}
