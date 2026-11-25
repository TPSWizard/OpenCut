//! Track mute, solo, volume and pan (row 83) in the audio plan, the
//! command and web bundles.

use editor::{Command, Element, MediaItem, MediaKind, Project, TrackKind, TrackPlacement, apply, plan_audio};
use time::MediaTime;

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap()
}

/// Two audio tracks with one 4 s clip each.
fn project_in(dir: &std::path::Path) -> (Project, Vec<String>) {
    let mut project = Project::new("Mix");
    let mut tracks = Vec::new();
    for id in ["a", "b"] {
        let media = MediaItem {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: MediaKind::Audio,
            path: {
                let path = dir.join(format!("{id}.wav"));
                std::fs::write(&path, b"RIFF").unwrap();
                path
            },
            duration: Some(seconds(4.0)),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        apply(&mut project, &Command::AddMedia(media.clone())).unwrap();
        apply(&mut project, &Command::AddTrack { kind: TrackKind::Audio }).unwrap();
        let track = project.tracks.iter().rev().find(|track| track.kind == TrackKind::Audio && track.elements.is_empty()).unwrap().id.clone();
        apply(&mut project, &Command::InsertElement { element: Element::from_media(&media, MediaTime::ZERO), placement: TrackPlacement::Track(track.clone()) })
            .unwrap();
        tracks.push(track);
    }
    (project, tracks)
}

#[test]
fn solo_volume_and_pan_reach_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let (mut project, tracks) = project_in(dir.path());
    assert_eq!(plan_audio(&project).len(), 2);
    apply(&mut project, &Command::SetTrackMix { track_id: tracks[1].clone(), solo: Some(true), volume: Some(-6.0), pan: Some(-0.5) }).unwrap();
    let plans = plan_audio(&project);
    assert_eq!(plans.len(), 1, "only the soloed track plays");
    assert_eq!(plans[0].media_id, "b");
    assert!((plans[0].track_gain - 0.501187).abs() < 1e-6);
    assert!((plans[0].gain() - 0.501187).abs() < 1e-6, "the track volume is in the clip's gain");
    assert_eq!(plans[0].track_pan, -0.5);
    // Muting the soloed track brings the other back.
    apply(&mut project, &Command::SetTrackMuted { track_id: tracks[1].clone(), muted: true }).unwrap();
    let plans = plan_audio(&project);
    assert_eq!(plans.iter().map(|plan| plan.media_id.as_str()).collect::<Vec<_>>(), vec!["a"]);
    // Values are kept within the fader's range; the bottom is silent.
    apply(&mut project, &Command::SetTrackMix { track_id: tracks[0].clone(), solo: None, volume: Some(-99.0), pan: Some(4.0) }).unwrap();
    let track = project.tracks.iter().find(|track| track.id == tracks[0]).unwrap();
    assert_eq!((track.volume, track.pan), (-60.0, 1.0));
    assert!(plan_audio(&project).is_empty(), "faded to the bottom and the other muted");
}

#[test]
fn web_bundles_carry_the_mixer() {
    let dir = tempfile::tempdir().unwrap();
    let (mut project, tracks) = project_in(dir.path());
    apply(&mut project, &Command::SetTrackMix { track_id: tracks[0].clone(), solo: Some(true), volume: Some(3.5), pan: Some(0.25) }).unwrap();
    let path = dir.path().join("mix.opencut.zip");
    editor::export_bundle(&project, &path).unwrap();
    let back = editor::import_bundle(&path, &editor::ProjectStore::new(dir.path().join("projects"))).unwrap().project;
    let track = back.tracks.iter().find(|track| track.id == tracks[0]).unwrap();
    assert_eq!((track.solo, track.volume, track.pan), (true, 3.5, 0.25));
    let other = back.tracks.iter().find(|track| track.id == tracks[1]).unwrap();
    assert_eq!((other.solo, other.volume, other.pan), (false, 0.0, 0.0));
}
