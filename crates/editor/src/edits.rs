//! Edit points and ripple plans: the times a cut sits at (every element's
//! start and end), and what a ripple delete removes and moves. Pure
//! functions over a project, the same as the web app's `timeline/edits.ts`.

use time::MediaTime;

use crate::document::Project;

/// Every element start and end, plus zero, sorted without repeats.
pub fn collect_edit_points(project: &Project) -> Vec<MediaTime> {
    let mut points = vec![MediaTime::ZERO];
    for (_, element) in project.elements() {
        points.push(element.start_time);
        points.push(element.end_time());
    }
    points.sort();
    points.dedup();
    points
}

/// The first edit point after `time`.
pub fn next_edit_point(project: &Project, time: MediaTime) -> Option<MediaTime> {
    collect_edit_points(project).into_iter().find(|point| *point > time)
}

/// The last edit point before `time`.
pub fn previous_edit_point(project: &Project, time: MediaTime) -> Option<MediaTime> {
    collect_edit_points(project).into_iter().filter(|point| *point < time).last()
}

/// What a ripple delete does: the elements to remove and the new start
/// times of those that close the gaps.
#[derive(Clone, Debug, PartialEq)]
pub struct RipplePlan {
    pub remove: Vec<String>,
    pub moves: Vec<(String, MediaTime)>,
    /// How much the timeline shortens by.
    pub removed: MediaTime,
}

impl Default for RipplePlan {
    fn default() -> Self {
        Self { remove: Vec::new(), moves: Vec::new(), removed: MediaTime::ZERO }
    }
}

/// `[start, end)` spans merged where they touch or overlap.
pub fn merge_spans(mut spans: Vec<(MediaTime, MediaTime)>) -> Vec<(MediaTime, MediaTime)> {
    spans.retain(|(start, end)| end > start);
    spans.sort_by_key(|(start, _)| *start);
    let mut merged: Vec<(MediaTime, MediaTime)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// The ripple delete of `element_ids`: they go, and every other element
/// that starts at or after a removed span's end moves left by the length
/// of the removed spans before it (spans merged across tracks, so
/// overlapping deletions count once), on every track.
pub fn ripple_delete_plan(project: &Project, element_ids: &[String]) -> RipplePlan {
    let spans: Vec<(MediaTime, MediaTime)> = project
        .elements()
        .filter(|(_, element)| element_ids.contains(&element.id))
        .map(|(_, element)| (element.start_time, element.end_time()))
        .collect();
    let merged = merge_spans(spans);
    let mut moves = Vec::new();
    for (track, element) in project.elements() {
        // Locked tracks stay where they are.
        if element_ids.contains(&element.id) || track.locked {
            continue;
        }
        let shift = merged
            .iter()
            .filter(|(_, end)| *end <= element.start_time)
            .fold(MediaTime::ZERO, |sum, (start, end)| sum + (*end - *start));
        if shift > MediaTime::ZERO {
            moves.push((element.id.clone(), element.start_time - shift));
        }
    }
    RipplePlan {
        remove: element_ids.to_vec(),
        moves,
        removed: merged.iter().fold(MediaTime::ZERO, |sum, (start, end)| sum + (*end - *start)),
    }
}

/// What removing the stretch `[from, to)` takes once every element crossing
/// `from` or `to` has been split there: the elements inside go, and
/// everything from `to` on moves left by the stretch's length.
pub fn delete_range_plan(project: &Project, from: MediaTime, to: MediaTime) -> RipplePlan {
    let length = (to - from).max(MediaTime::ZERO);
    let mut plan = RipplePlan { removed: length, ..Default::default() };
    for (_, element) in project.elements().filter(|(track, _)| !track.locked) {
        if element.start_time >= from && element.end_time() <= to {
            plan.remove.push(element.id.clone());
        } else if element.start_time >= to && length > MediaTime::ZERO {
            plan.moves.push((element.id.clone(), element.start_time - length));
        }
    }
    plan
}

/// The elements whose span crosses `time` with at least `margin` on both
/// sides (a split there makes two pieces of a usable length).
pub fn elements_crossing(project: &Project, time: MediaTime, margin: MediaTime) -> Vec<String> {
    project
        .elements()
        .filter(|(track, _)| !track.locked)
        .filter(|(_, element)| element.start_time + margin < time && element.end_time() - margin > time)
        .map(|(_, element)| element.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Element, Lane, Track, TrackKind};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    fn project() -> (Project, Vec<String>) {
        let mut project = Project::new("Edits");
        let mut video = Track::new(TrackKind::Text, Lane::Overlay, "v");
        let mut ids = Vec::new();
        for (start, duration) in [(0.0, 10.0), (10.0, 5.0), (20.0, 5.0)] {
            let mut element = Element::text("x", seconds(start));
            element.duration = seconds(duration);
            ids.push(element.id.clone());
            video.elements.push(element);
        }
        let mut titles = Track::new(TrackKind::Text, Lane::Overlay, "t");
        let mut title = Element::text("title", seconds(12.0));
        title.duration = seconds(3.0);
        ids.push(title.id.clone());
        titles.elements.push(title);
        project.tracks.push(video);
        project.tracks.push(titles);
        (project, ids)
    }

    #[test]
    fn edit_points_step_through_starts_and_ends() {
        let (project, _) = project();
        let points: Vec<f64> = collect_edit_points(&project).into_iter().map(|p| p.to_seconds_f64()).collect();
        assert_eq!(points, vec![0.0, 10.0, 12.0, 15.0, 20.0, 25.0]);
        assert_eq!(next_edit_point(&project, seconds(10.0)), Some(seconds(12.0)));
        assert_eq!(next_edit_point(&project, seconds(25.0)), None);
        assert_eq!(previous_edit_point(&project, seconds(12.0)), Some(seconds(10.0)));
        assert_eq!(previous_edit_point(&project, MediaTime::ZERO), None);
    }

    #[test]
    fn ripple_delete_closes_gaps_on_every_track() {
        let (project, ids) = project();
        let plan = ripple_delete_plan(&project, &[ids[1].clone()]);
        assert_eq!(plan.remove, vec![ids[1].clone()]);
        assert_eq!(plan.removed, seconds(5.0));
        assert_eq!(plan.moves, vec![(ids[2].clone(), seconds(15.0))], "the title inside the span stays");
        assert_eq!(merge_spans(vec![(seconds(0.0), seconds(10.0)), (seconds(5.0), seconds(15.0))]), vec![(seconds(0.0), seconds(15.0))]);
    }

    #[test]
    fn delete_range_drops_the_inside_and_pulls_the_rest_left() {
        let (project, ids) = project();
        // Nothing crosses 10 exactly; the second clip crosses 12 by 2 s each side.
        assert!(elements_crossing(&project, seconds(10.0), seconds(0.1)).is_empty());
        assert_eq!(elements_crossing(&project, seconds(12.0), seconds(0.1)), vec![ids[1].clone()]);
        let plan = delete_range_plan(&project, seconds(10.0), seconds(15.0));
        assert_eq!(plan.remove, vec![ids[1].clone(), ids[3].clone()]);
        assert_eq!(plan.moves, vec![(ids[2].clone(), seconds(15.0))]);
    }

    #[test]
    fn locked_tracks_refuse_edits_and_stay_put_in_ripples() {
        use crate::{Command, EditorError, Element, TrackPlacement, apply};
        let seconds = |value: f64| MediaTime::from_seconds_f64(value).unwrap();
        let mut project = Project::new("Lock");
        let insert = |project: &mut Project, start: f64| {
            let mut element = Element::text("t", seconds(start));
            element.duration = seconds(2.0);
            apply(project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap().element_refs()[0].clone()
        };
        let first = insert(&mut project, 0.0);
        let later = insert(&mut project, 4.0);
        // A second text track (overlapping) holds the clip that will be locked.
        let locked_clip = insert(&mut project, 1.0);
        assert_ne!(locked_clip.track_id, first.track_id);
        apply(&mut project, &Command::SetTrackLocked { track_id: locked_clip.track_id.clone(), locked: true }).unwrap();

        let moved = apply(&mut project, &Command::MoveElement { element_id: locked_clip.element_id.clone(), track_id: None, start_time: seconds(9.0) });
        assert!(matches!(moved, Err(EditorError::TrackLocked(_))));
        assert!(apply(&mut project, &Command::DeleteElements(vec![locked_clip.element_id.clone()])).is_err());
        let onto = apply(
            &mut project,
            &Command::MoveElement { element_id: later.element_id.clone(), track_id: Some(locked_clip.track_id.clone()), start_time: seconds(5.0) },
        );
        assert!(matches!(onto, Err(EditorError::TrackLocked(_))), "nothing moves onto a locked track");

        // Ripple-deleting the first clip shifts the unlocked track only.
        apply(&mut project, &Command::RippleDelete { element_ids: vec![first.element_id.clone()] }).unwrap();
        assert_eq!(project.element(&later.element_id).unwrap().1.start_time, seconds(2.0));
        assert_eq!(project.element(&locked_clip.element_id).unwrap().1.start_time, seconds(1.0));

        // A range delete skips the locked clip it crosses.
        apply(&mut project, &Command::DeleteRange { from: seconds(1.5), to: seconds(2.5) }).unwrap();
        let kept = project.element(&locked_clip.element_id).unwrap().1;
        assert_eq!((kept.start_time, kept.duration), (seconds(1.0), seconds(2.0)));

        // New clips never land on the locked track.
        let placed = insert(&mut project, 20.0);
        assert_ne!(placed.track_id, locked_clip.track_id);
        assert!(apply(&mut project, &Command::RemoveTrack { track_id: locked_clip.track_id.clone() }).is_err());

        apply(&mut project, &Command::SetTrackLocked { track_id: locked_clip.track_id.clone(), locked: false }).unwrap();
        apply(&mut project, &Command::DeleteElements(vec![locked_clip.element_id])).unwrap();
    }
}
