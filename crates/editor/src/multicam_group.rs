//! Multicam groups: camera angles of one scene, each a clip on its own
//! track, lined up by their sound, with a cut list saying which angle shows
//! when (the shared `multicam` crate). The group lives in the clips' params:
//! `multicam.group` (the group's id), `multicam.angle` (0 for the angle the
//! others were lined up to) and `multicam.cuts` (the cut list, the same on
//! every angle). Only the angle a cut picks is opaque (stepped opacity
//! keys), and only the first angle's sound plays. The web editor builds the
//! same commands (`apps/web/src/multicam`).

use ::multicam::Cut;
use time::MediaTime;

use crate::commands::Command;
use crate::document::{ElementKind, Project};
use crate::params::Params;
use crate::ElementPatch;

pub const MULTICAM_GROUP: &str = "multicam.group";
pub const MULTICAM_ANGLE: &str = "multicam.angle";
pub const MULTICAM_CUTS: &str = "multicam.cuts";

/// One angle of a group, as the editors need it.
#[derive(Clone, Debug, PartialEq)]
pub struct MulticamAngle {
    pub element_id: String,
    pub track_id: String,
    pub name: String,
    pub angle: usize,
    pub start: f64,
    pub end: f64,
}

/// The group `element_id` belongs to, by angle; `None` when it is not in one.
pub fn multicam_group(project: &Project, element_id: &str) -> Option<(String, Vec<MulticamAngle>)> {
    let (_, element) = project.element(element_id)?;
    let group = element.params.text(MULTICAM_GROUP, "").into_owned();
    if group.is_empty() {
        return None;
    }
    let mut angles: Vec<MulticamAngle> = project
        .tracks
        .iter()
        .flat_map(|track| track.elements.iter().map(move |element| (track, element)))
        .filter(|(_, element)| element.params.text(MULTICAM_GROUP, "") == group)
        .map(|(track, element)| MulticamAngle {
            element_id: element.id.clone(),
            track_id: track.id.clone(),
            name: element.name.clone(),
            angle: element.params.number(MULTICAM_ANGLE, 0.0).max(0.0) as usize,
            start: element.start_time.to_seconds_f64(),
            end: element.end_time().to_seconds_f64(),
        })
        .collect();
    angles.sort_by_key(|angle| angle.angle);
    Some((group, angles))
}

/// The group's cut list (from any angle that has one).
pub fn multicam_cuts(project: &Project, angles: &[MulticamAngle]) -> Vec<Cut> {
    angles
        .iter()
        .filter_map(|angle| project.element(&angle.element_id))
        .map(|(_, element)| element.params.text(MULTICAM_CUTS, "").into_owned())
        .find(|text| !text.is_empty())
        .map(|text| ::multicam::parse_cuts(&text))
        .unwrap_or_default()
}

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap_or(MediaTime::ZERO)
}

/// Lines angles up: `angles` are element ids with the lag the sound sync
/// found for each (the first is the reference, lag 0). Each moves to its
/// reference start plus its lag (everything later if one would start before
/// zero), joins a new group `group_id` as angle 0, 1, 2…, and all but the
/// first are muted. Every angle must be a video clip on its own track.
pub fn multicam_sync_commands(project: &Project, angles: &[(String, f64)], group_id: &str) -> Result<Vec<Command>, String> {
    if angles.len() < 2 {
        return Err("Select two or more video clips to sync".to_owned());
    }
    let mut placed = Vec::new();
    let mut tracks = std::collections::HashSet::new();
    for (element_id, lag) in angles {
        let (track, element) = project.element(element_id).ok_or("A clip to sync is no longer on the timeline")?;
        if !matches!(element.kind, ElementKind::Video { .. }) {
            return Err("Multicam angles must be video clips".to_owned());
        }
        if !tracks.insert(track.id.clone()) {
            return Err("Put each angle on its own track".to_owned());
        }
        placed.push((element, *lag));
    }
    let reference_start = placed[0].0.start_time.to_seconds_f64();
    let lowest = placed.iter().map(|(_, lag)| reference_start + lag).fold(f64::INFINITY, f64::min);
    let shift = (-lowest).max(0.0);
    let mut commands = Vec::new();
    for (index, (element, lag)) in placed.iter().enumerate() {
        let start = seconds(reference_start + lag + shift);
        if start != element.start_time {
            commands.push(Command::MoveElement { element_id: element.id.clone(), track_id: None, start_time: start });
        }
        let mut params = Params::new();
        params
            .set(MULTICAM_GROUP, group_id)
            .set(MULTICAM_ANGLE, index as f64)
            .set(MULTICAM_CUTS, "")
            .set("muted", index > 0);
        commands.push(Command::UpdateElement {
            element_id: element.id.clone(),
            patch: ElementPatch { params: Some(params), ..Default::default() },
        });
    }
    Ok(commands)
}

/// Switches the group of `element_id` to `angle` from timeline second
/// `time` on: the cut list gains the cut and every angle's opacity keys
/// follow it.
pub fn multicam_switch_commands(project: &Project, element_id: &str, time: f64, angle: usize) -> Result<Vec<Command>, String> {
    let (_, angles) = multicam_group(project, element_id).ok_or("The clip is not in a multicam group")?;
    if !angles.iter().any(|candidate| candidate.angle == angle) {
        return Err(format!("The group has no angle {}", angle + 1));
    }
    let cuts = ::multicam::add_cut(&multicam_cuts(project, &angles), time, angle);
    Ok(apply_cuts(&angles, &cuts))
}

/// Takes every cut out of the group of `element_id`: the first angle shows
/// throughout again.
pub fn multicam_clear_commands(project: &Project, element_id: &str) -> Result<Vec<Command>, String> {
    let (_, angles) = multicam_group(project, element_id).ok_or("The clip is not in a multicam group")?;
    Ok(apply_cuts(&angles, &[]))
}

fn apply_cuts(angles: &[MulticamAngle], cuts: &[Cut]) -> Vec<Command> {
    let text = ::multicam::encode_cuts(cuts);
    let mut commands = Vec::new();
    for angle in angles {
        let keys = ::multicam::opacity_keys(cuts, angle.angle, angle.start, angle.end);
        commands.push(Command::SetKeyframes {
            element_id: angle.element_id.clone(),
            path: "opacity".to_owned(),
            keys: keys.into_iter().map(|(time, value)| (seconds(time), value)).collect(),
        });
        let mut params = Params::new();
        params.set(MULTICAM_CUTS, text.as_str());
        commands.push(Command::UpdateElement {
            element_id: angle.element_id.clone(),
            patch: ElementPatch { params: Some(params), ..Default::default() },
        });
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{TrackPlacement, apply};
    use crate::document::{Element, MediaItem, MediaKind};

    fn project_with_angles() -> (Project, Vec<String>) {
        let mut project = Project::new("Multicam");
        let mut ids = Vec::new();
        for (index, start) in [(0, 2.0), (1, 2.0), (2, 5.0)] {
            let id = format!("cam{index}");
            apply(
                &mut project,
                &Command::AddMedia(MediaItem {
                    id: id.clone(),
                    name: format!("{id}.mp4"),
                    kind: MediaKind::Video,
                    path: format!("{id}.mp4").into(),
                    duration: Some(seconds(10.0)),
                    width: Some(1920),
                    height: Some(1080),
                    fps: Some(30.0),
                    has_audio: true,
                }),
            )
            .unwrap();
            let element = Element::from_media(project.media_item(&id).unwrap(), seconds(start));
            ids.push(element.id.clone());
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        (project, ids)
    }

    fn run(project: &mut Project, commands: Vec<Command>) {
        apply(project, &Command::Batch(commands)).unwrap();
    }

    #[test]
    fn syncing_moves_tags_and_mutes_the_angles() {
        let (mut project, ids) = project_with_angles();
        // Angle 2 starts its sound 1.5 s before the reference, angle 3 0.25 s after.
        let commands =
            multicam_sync_commands(&project, &[(ids[0].clone(), 0.0), (ids[1].clone(), -1.5), (ids[2].clone(), 0.25)], "g1").unwrap();
        run(&mut project, commands);
        let (group, angles) = multicam_group(&project, &ids[1]).expect("grouped");
        assert_eq!(group, "g1");
        let starts: Vec<f64> = angles.iter().map(|angle| angle.start).collect();
        assert_eq!(starts, vec![2.0, 0.5, 2.25]);
        let muted: Vec<bool> =
            angles.iter().map(|angle| project.element(&angle.element_id).unwrap().1.params.bool("muted", false)).collect();
        assert_eq!(muted, vec![false, true, true]);

        // Two before zero: everything shifts later so nothing starts before 0.
        let (mut early, ids) = project_with_angles();
        let commands = multicam_sync_commands(&early, &[(ids[0].clone(), 0.0), (ids[1].clone(), -3.0)], "g2").unwrap();
        run(&mut early, commands);
        let (_, angles) = multicam_group(&early, &ids[0]).unwrap();
        assert_eq!(angles.iter().map(|angle| angle.start).collect::<Vec<_>>(), vec![3.0, 0.0]);
    }

    #[test]
    fn switching_steps_each_angles_opacity() {
        let (mut project, ids) = project_with_angles();
        let commands = multicam_sync_commands(&project, &[(ids[0].clone(), 0.0), (ids[1].clone(), 0.0)], "g").unwrap();
        run(&mut project, commands);
        let commands = multicam_switch_commands(&project, &ids[0], 5.0, 1).unwrap();
        run(&mut project, commands);
        let commands = multicam_switch_commands(&project, &ids[1], 8.0, 0).unwrap();
        run(&mut project, commands);
        let opacity_at = |project: &Project, id: &str, t: f64| {
            let (_, element) = project.element(id).unwrap();
            element.params_at(seconds(t)).number("opacity", 1.0)
        };
        for (t, first, second) in [(3.0, 1.0, 0.0), (6.0, 0.0, 1.0), (9.0, 1.0, 0.0)] {
            assert_eq!((opacity_at(&project, &ids[0], t), opacity_at(&project, &ids[1], t)), (first, second), "at {t}");
        }
        assert_eq!(multicam_cuts(&project, &multicam_group(&project, &ids[0]).unwrap().1).len(), 2);
        assert!(multicam_switch_commands(&project, &ids[0], 9.0, 5).is_err(), "no such angle");
        assert!(multicam_switch_commands(&project, &ids[2], 9.0, 0).is_err(), "not in a group");

        let commands = multicam_clear_commands(&project, &ids[0]).unwrap();
        run(&mut project, commands);
        assert_eq!(opacity_at(&project, &ids[0], 6.0), 1.0);
        assert_eq!(opacity_at(&project, &ids[1], 6.0), 0.0);
    }

    #[test]
    fn angles_must_be_videos_on_their_own_tracks() {
        let (project, ids) = project_with_angles();
        assert!(multicam_sync_commands(&project, &[(ids[0].clone(), 0.0)], "g").is_err());
        let same_track = multicam_sync_commands(&project, &[(ids[0].clone(), 0.0), (ids[0].clone(), 0.5)], "g");
        assert_eq!(same_track.unwrap_err(), "Put each angle on its own track");
    }
}
