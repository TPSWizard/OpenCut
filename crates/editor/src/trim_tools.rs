//! Slip, slide and roll (row 93) on a project: the shared `trimedit`
//! crate works out the edit, this turns it into one undo step.

use time::MediaTime;

use crate::{Command, Element, ElementKind, ElementPatch, Project};

pub use trimedit::Tool as TrimTool;

/// The shortest a clip may become: one frame at the project's rate (a
/// thirtieth of a second when the rate is unknown).
fn min_duration(project: &Project) -> i64 {
    project.settings.fps.ticks_per_frame().unwrap_or(4000).max(1)
}

fn clip(element: &Element) -> trimedit::Clip {
    trimedit::Clip {
        id: element.id.clone(),
        start: element.start_time.as_ticks(),
        duration: element.duration.as_ticks(),
        trim_start: element.trim_start.as_ticks(),
        trim_end: element.trim_end.as_ticks(),
        limited: matches!(element.kind, ElementKind::Video { .. } | ElementKind::Audio { .. }),
        retimed: element.retime.as_ref().is_some_and(|retime| !retime.is_identity()),
    }
}

/// The command for a trim tool on `element_id` moved by `delta` (slip:
/// later media; slide: later on the timeline; roll: the cut at its end
/// later), clamped to what fits, and the move that fitted. Errors say why
/// the edit cannot be made.
pub fn trim_tool_command(project: &Project, tool: TrimTool, element_id: &str, delta: MediaTime) -> Result<(Command, MediaTime), String> {
    let (track, element) = project.element(element_id).ok_or("The clip is not on the timeline")?;
    if track.locked {
        return Err("The clip's track is locked".to_owned());
    }
    let clips: Vec<trimedit::Clip> = track.elements.iter().map(clip).collect();
    let plan = trimedit::plan(tool, &clip(element), &clips, delta.as_ticks(), min_duration(project))?;
    // The clip that gives way goes first, so no two clips overlap on the
    // way: moving later, the later clips change before the earlier ones.
    let mut changes = plan.changes.clone();
    if plan.delta > 0 {
        changes.reverse();
    }
    let commands: Vec<Command> = changes
        .iter()
        .map(|change| Command::UpdateElement {
            element_id: change.id.clone(),
            patch: ElementPatch {
                start_time: Some(MediaTime::from_ticks(change.start)),
                duration: Some(MediaTime::from_ticks(change.duration)),
                trim_start: Some(MediaTime::from_ticks(change.trim_start)),
                trim_end: Some(MediaTime::from_ticks(change.trim_end)),
                ..Default::default()
            },
        })
        .collect();
    Ok((Command::Batch(commands), MediaTime::from_ticks(plan.delta)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaItem, MediaKind, TrackPlacement, apply};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn slip_slide_and_roll_change_the_project_in_one_step() {
        let mut project = Project::new("Trim tools");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "v".to_owned(),
                name: "v.mp4".to_owned(),
                kind: MediaKind::Video,
                path: "v.mp4".into(),
                duration: Some(seconds(10.0)),
                width: Some(640),
                height: Some(360),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let mut ids = Vec::new();
        for (start, trim) in [(0.0, 1.0), (3.0, 2.0), (6.0, 3.0)] {
            let mut element = Element::from_media(project.media_item("v").unwrap(), seconds(start));
            element.trim_start = seconds(trim);
            element.duration = seconds(3.0);
            element.trim_end = seconds(10.0 - trim - 3.0);
            ids.push(element.id.clone());
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        let get = |project: &Project, id: &str| project.element(id).unwrap().1.clone();
        // Slip the middle clip half a second later: same place, later media.
        let (command, moved) = trim_tool_command(&project, TrimTool::Slip, &ids[1], seconds(0.5)).unwrap();
        assert_eq!(moved, seconds(0.5));
        apply(&mut project, &command).unwrap();
        let middle = get(&project, &ids[1]);
        assert_eq!((middle.start_time, middle.trim_start), (seconds(3.0), seconds(2.5)));
        // Roll the first cut a second later: the first clip ends later, the second starts later.
        let (command, _) = trim_tool_command(&project, TrimTool::Roll, &ids[0], seconds(1.0)).unwrap();
        apply(&mut project, &command).unwrap();
        let (first, middle) = (get(&project, &ids[0]), get(&project, &ids[1]));
        assert_eq!(first.duration, seconds(4.0));
        assert_eq!((middle.start_time, middle.duration, middle.trim_start), (seconds(4.0), seconds(2.0), seconds(3.5)));
        // Slide the middle clip back half a second: its neighbours give and take.
        let (command, _) = trim_tool_command(&project, TrimTool::Slide, &ids[1], seconds(-0.5)).unwrap();
        apply(&mut project, &command).unwrap();
        let (first, middle, last) = (get(&project, &ids[0]), get(&project, &ids[1]), get(&project, &ids[2]));
        assert_eq!(first.duration, seconds(3.5));
        assert_eq!((middle.start_time, middle.trim_start), (seconds(3.5), seconds(3.5)));
        assert_eq!((last.start_time, last.duration, last.trim_start), (seconds(5.5), seconds(3.5), seconds(2.5)));
    }
}
