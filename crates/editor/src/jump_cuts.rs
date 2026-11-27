//! Smooth jump cuts (row 106): the jump cuts on a project's video tracks
//! (the shared `jumpcut` crate decides what is one), the bridge planned
//! across each, and the bridge clip that shows the in-between frames.
//!
//! A bridge is a short video clip of its own (the in-between frames made
//! from the frame before the bridge and the frame after it) on the video
//! track directly above the cut, with the first clip's look at the cut:
//! its position, scale, effects and masks. The clips under it, and their
//! sound, are not changed, so deleting the bridge undoes it. See
//! [`crate::Command::InsertJumpCutBridges`]. The web editor's
//! `src/jump-cuts/` does the same.

use jumpcut::{BridgePlan, CutClip, find_jump_cuts, plan_bridge};
use time::MediaTime;

use crate::{Element, ElementKind, ParamValue, Project, TrackKind, resolve_params};

/// Set on bridge clips, so a cut is not smoothed twice.
pub const JUMP_CUT_BRIDGE_PARAM: &str = "jumpcut.bridge";
/// The name of a track made to hold bridges.
pub const JUMP_CUT_TRACK_NAME: &str = "Smooth cuts";

/// A jump cut on the project: `before_id` ends where `after_id` starts on
/// `track_id`, both showing the same file, which jumps by `source_gap`
/// seconds. `smoothed` when a bridge already covers it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectJumpCut {
    pub track_id: String,
    pub before_id: String,
    pub after_id: String,
    pub time: MediaTime,
    pub source_gap: f64,
    pub smoothed: bool,
}

fn is_bridge(element: &Element) -> bool {
    matches!(element.params.get(JUMP_CUT_BRIDGE_PARAM), Some(ParamValue::Bool(true)))
}

/// The jump cuts between visible video clips on unlocked video tracks.
pub fn project_jump_cuts(project: &Project) -> Vec<ProjectJumpCut> {
    let fps = project.settings.fps.as_f64().unwrap_or(30.0).max(1.0);
    let mut clips = Vec::new();
    for track in &project.tracks {
        if track.kind != TrackKind::Video || track.locked {
            continue;
        }
        for element in &track.elements {
            let ElementKind::Video { media_id } = &element.kind else { continue };
            if element.hidden || is_bridge(element) {
                continue;
            }
            clips.push(CutClip {
                id: element.id.clone(),
                track: track.id.clone(),
                media: media_id.clone(),
                start: element.start_time.to_seconds_f64(),
                end: element.end_time().to_seconds_f64(),
                source_start: element.source_time(element.start_time).to_seconds_f64(),
                source_end: element.source_time(element.end_time()).to_seconds_f64(),
            });
        }
    }
    find_jump_cuts(&clips, fps)
        .into_iter()
        .map(|cut| {
            let (before, after) = (&clips[cut.before], &clips[cut.after]);
            let time = project.element(&after.id).map(|(_, element)| element.start_time).unwrap_or(MediaTime::ZERO);
            let smoothed = project
                .track_index(&before.track)
                .and_then(|index| index.checked_sub(1))
                .map(|above| project.tracks[above].elements.iter().any(|element| is_bridge(element) && element.contains(time)))
                .unwrap_or(false);
            ProjectJumpCut {
                track_id: before.track.clone(),
                before_id: before.id.clone(),
                after_id: after.id.clone(),
                time,
                source_gap: cut.source_gap,
                smoothed,
            }
        })
        .collect()
}

/// What making one bridge needs: the plan, the file both clips show, and
/// the source times of the frame before the bridge and the frame after it.
#[derive(Clone, Debug, PartialEq)]
pub struct JumpCutBridgeJob {
    pub cut: ProjectJumpCut,
    pub plan: BridgePlan,
    pub media_id: String,
    pub from_source: f64,
    pub to_source: f64,
}

/// Plans a bridge of `frames` frames at the project's frame rate across
/// `cut` (fewer when the clips are short; `None` when not even two fit).
pub fn plan_jump_cut_bridge(project: &Project, cut: &ProjectJumpCut, frames: usize) -> Option<JumpCutBridgeJob> {
    let fps = project.settings.fps.as_f64().unwrap_or(30.0).max(1.0);
    let (_, before) = project.element(&cut.before_id)?;
    let (_, after) = project.element(&cut.after_id)?;
    let ElementKind::Video { media_id } = &before.kind else { return None };
    let time = cut.time.to_seconds_f64();
    let plan = plan_bridge(time, before.start_time.to_seconds_f64(), after.end_time().to_seconds_f64(), fps, frames)?;
    let seconds = |value: f64| MediaTime::from_seconds_f64(value).unwrap_or(MediaTime::ZERO);
    Some(JumpCutBridgeJob {
        cut: cut.clone(),
        media_id: media_id.clone(),
        from_source: before.source_time(seconds(plan.from_time)).to_seconds_f64(),
        to_source: after.source_time(seconds(plan.to_time)).to_seconds_f64(),
        plan,
    })
}

/// The bridge clip for `job`, showing `media_id` (the in-between frames as
/// a video `plan.frames` frames long): the first clip's params with
/// animated values fixed at the cut (no sound), its effects and masks.
pub fn jump_cut_bridge_element(project: &Project, job: &JumpCutBridgeJob, media_id: &str) -> Option<Element> {
    let (_, before) = project.element(&job.cut.before_id)?;
    let local = (job.cut.time - before.start_time).max(MediaTime::ZERO).min(before.duration);
    let mut params = resolve_params(&before.params, &before.animations, local);
    for key in ["volume", "muted"] {
        params.remove(key);
    }
    params.set(JUMP_CUT_BRIDGE_PARAM, ParamValue::Bool(true));
    let duration = MediaTime::from_seconds_f64(job.plan.duration)?;
    Some(Element {
        id: crate::new_id(),
        name: format!("{} (smooth cut)", before.name),
        kind: ElementKind::Video { media_id: media_id.to_owned() },
        start_time: MediaTime::from_seconds_f64(job.plan.start)?,
        duration,
        trim_start: MediaTime::ZERO,
        trim_end: MediaTime::ZERO,
        source_duration: Some(duration),
        hidden: false,
        params,
        animations: Default::default(),
        effects: before.effects.clone(),
        masks: before.masks.clone(),
        transition: None,
        transition_in: None,
        cutout: None,
        words: Default::default(),
        retime: None,
        group_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Editor, MediaItem, MediaKind, Project, TrackPlacement};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn media(id: &str, kind: MediaKind, duration: f64) -> MediaItem {
        MediaItem {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
            path: format!("{id}.mp4").into(),
            width: Some(1920),
            height: Some(1080),
            duration: Some(seconds(duration)),
            fps: Some(30.0),
            has_audio: kind != MediaKind::Image,
        }
    }

    fn piece(editor: &Editor, media_id: &str, start: f64, trim: f64, length: f64) -> Element {
        let mut element = Element::from_media(editor.project().media_item(media_id).unwrap(), seconds(start));
        element.trim_start = seconds(trim);
        element.duration = seconds(length);
        element.trim_end = seconds(20.0 - trim - length);
        element
    }

    #[test]
    fn bridges_go_above_each_jump_cut_without_touching_the_clips() {
        let mut editor = Editor::new(Project::new("Jump cuts"));
        editor.execute(&Command::AddMedia(media("take", MediaKind::Video, 20.0))).unwrap();
        editor.execute(&Command::AddMedia(media("bridge-a", MediaKind::Video, 0.2))).unwrap();
        editor.execute(&Command::AddMedia(media("bridge-b", MediaKind::Video, 0.2))).unwrap();
        let main = editor.project().tracks[editor.project().main_track_index()].id.clone();
        // Words cut out at 2 s (the file jumps 0.8 s) and at 5 s; a plain split at 3.5 s.
        let mut first = piece(&editor, "take", 0.0, 0.0, 2.0);
        first.params.set("transform.scaleX", 1.5);
        for element in [first, piece(&editor, "take", 2.0, 2.8, 1.5), piece(&editor, "take", 3.5, 4.3, 1.5), piece(&editor, "take", 5.0, 7.0, 3.0)] {
            editor.execute(&Command::InsertElement { element, placement: TrackPlacement::Track(main.clone()) }).unwrap();
        }
        let cuts = project_jump_cuts(editor.project());
        assert_eq!(cuts.iter().map(|cut| cut.time.to_seconds_f64()).collect::<Vec<_>>(), vec![2.0, 5.0]);
        assert!((cuts[0].source_gap - 0.8).abs() < 1e-9);
        assert!(cuts.iter().all(|cut| !cut.smoothed && cut.track_id == main));

        let job = plan_jump_cut_bridge(editor.project(), &cuts[0], 6).unwrap();
        assert_eq!(job.plan.frames, 6);
        assert!((job.plan.start - 1.9).abs() < 1e-9);
        assert!((job.from_source - (2.0 - 4.0 / 30.0)).abs() < 1e-6, "{}", job.from_source);
        assert!((job.to_source - (2.8 + 0.1)).abs() < 1e-6, "{}", job.to_source);
        let second = plan_jump_cut_bridge(editor.project(), &cuts[1], 6).unwrap();
        let bridges = vec![
            (main.clone(), jump_cut_bridge_element(editor.project(), &job, "bridge-a").unwrap()),
            (main.clone(), jump_cut_bridge_element(editor.project(), &second, "bridge-b").unwrap()),
        ];
        assert_eq!(bridges[0].1.params.get("transform.scaleX"), Some(&ParamValue::Number(1.5)), "the first clip's look");
        let tracks_before = editor.project().tracks.len();
        let clips_before = editor.project().tracks[editor.project().main_track_index()].elements.clone();
        editor.execute(&Command::InsertJumpCutBridges { bridges }).unwrap();

        let project = editor.project();
        assert_eq!(project.tracks.len(), tracks_before + 1, "one new track holds both bridges");
        let main_index = project.main_track_index();
        assert_eq!(project.tracks[main_index].elements, clips_before, "the cut clips are unchanged");
        let above = &project.tracks[main_index - 1];
        assert_eq!(above.name, JUMP_CUT_TRACK_NAME);
        assert_eq!(above.elements.len(), 2);
        assert!(project_jump_cuts(project).iter().all(|cut| cut.smoothed));

        editor.undo().unwrap();
        assert_eq!(editor.project().tracks.len(), tracks_before);
    }

    #[test]
    fn short_clips_get_a_shorter_bridge_or_none() {
        let mut editor = Editor::new(Project::new("Short"));
        editor.execute(&Command::AddMedia(media("take", MediaKind::Video, 20.0))).unwrap();
        let main = editor.project().tracks[editor.project().main_track_index()].id.clone();
        for element in [piece(&editor, "take", 0.0, 0.0, 0.1), piece(&editor, "take", 0.1, 1.0, 1.0), piece(&editor, "take", 1.1, 3.0, 0.05)] {
            editor.execute(&Command::InsertElement { element, placement: TrackPlacement::Track(main.clone()) }).unwrap();
        }
        let cuts = project_jump_cuts(editor.project());
        assert_eq!(cuts.len(), 2);
        assert_eq!(plan_jump_cut_bridge(editor.project(), &cuts[0], 6).unwrap().plan.frames, 5, "three frames before the cut");
        assert!(plan_jump_cut_bridge(editor.project(), &cuts[1], 6).is_none(), "one frame after the cut");
    }
}
