//! Tracked masks and corner pin (row 92): what motion tracking writes.
//! The tracking itself is the shared `tracking` crate.

use time::MediaTime;

use crate::{Command, Element, ElementPatch, Params, Project};

/// The commands that key a clip's corner pin to tracked corners: `keys` as
/// `tracking::corner_keys` gives them (timeline seconds; top left, top
/// right, bottom right, bottom left in canvas pixels from the canvas
/// centre), each turned into the clip's box units at its time. Switches the
/// pin on. One undo step; `None` when the clip has no box.
pub fn tracked_corner_pin_command(project: &Project, pinned: &Element, keys: &[(f64, [(f64, f64); 4])]) -> Option<Command> {
    let canvas = project.settings.canvas;
    let (half_width, half_height) = (canvas.width as f64 / 2.0, canvas.height as f64 / 2.0);
    let paths = crate::corner_pin_keys();
    let mut channels: Vec<Vec<(MediaTime, f64)>> = vec![Vec::new(); paths.len()];
    for (seconds, corners) in keys {
        let Some(time) = MediaTime::from_seconds_f64(*seconds) else { continue };
        let quad = crate::element_quad(project, pinned, time.clamp(pinned.start_time, pinned.end_time() - MediaTime::ONE_TICK))?;
        let pin_box = crate::PinBox { center: [quad.center_x, quad.center_y], size: [quad.width, quad.height], rotation_degrees: quad.rotation_degrees };
        let in_box: [[f64; 2]; 4] = corners.map(|(x, y)| crate::canvas_to_box(&pin_box, [x + half_width, y + half_height]));
        let local = (time - pinned.start_time).clamp(MediaTime::ZERO, pinned.duration);
        for (channel, (_, value)) in channels.iter_mut().zip(crate::corner_pin_params(&in_box)) {
            if channel.last().is_some_and(|(last, _)| *last == local) {
                continue;
            }
            channel.push((local, value));
        }
    }
    if channels[0].is_empty() {
        return None;
    }
    let mut params = Params::new();
    params.set(crate::CORNER_PIN_ENABLED, true);
    let mut commands = vec![Command::UpdateElement { element_id: pinned.id.clone(), patch: ElementPatch { params: Some(params), ..Default::default() } }];
    for (path, keys) in paths.into_iter().zip(channels) {
        commands.push(Command::SetKeyframes { element_id: pinned.id.clone(), path, keys });
    }
    Some(Command::Batch(commands))
}

/// The command that stores a mask's tracked motion (`tracking::MaskTrack`
/// text) on it, or clears it with an empty text.
pub fn tracked_mask_command(element: &Element, mask_id: &str, track: &str) -> Command {
    let mut params = Params::new();
    params.set(tracking::MASK_TRACK_PARAM, track.to_owned());
    Command::UpdateMask { element_id: element.id.clone(), mask_id: mask_id.to_owned(), params }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaItem, MediaKind, TrackPlacement, apply};

    #[test]
    fn tracked_corners_become_corner_pin_keys_and_masks_move_with_their_track() {
        let mut project = Project::new("Tracked");
        project.settings.canvas = crate::CanvasSize { width: 1000, height: 500 };
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "pic".to_owned(),
                name: "pic.png".to_owned(),
                kind: MediaKind::Image,
                path: "pic.png".into(),
                duration: None,
                width: Some(1000),
                height: Some(500),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let element = Element::from_media(project.media_item("pic").unwrap(), MediaTime::ZERO);
        let id = element.id.clone();
        apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        let pinned = project.element(&id).unwrap().1.clone();
        // The box fills the canvas: its corners from the centre.
        let corners = [(-500.0, -250.0), (500.0, -250.0), (500.0, 250.0), (-500.0, 250.0)];
        let moved = corners.map(|(x, y)| (x + 100.0, y));
        let command = tracked_corner_pin_command(&project, &pinned, &[(0.0, corners), (1.0, moved)]).unwrap();
        apply(&mut project, &command).unwrap();
        let element = project.element(&id).unwrap().1;
        let at = |seconds: f64| crate::corner_pin_corners(&element.params_at(MediaTime::from_seconds_f64(seconds).unwrap()));
        assert_eq!(at(0.0)[0], [0.0, 0.0]);
        assert!((at(1.0)[0][0] - 0.1).abs() < 1e-9, "a tenth of the box to the right: {:?}", at(1.0));
        assert!((at(0.5)[2][0] - 1.05).abs() < 1e-9, "halfway: {:?}", at(0.5));
        assert!(element.params.bool(crate::CORNER_PIN_ENABLED, false));

        // A mask with a track moves by it.
        let mut element = element.clone();
        let mut mask = crate::Mask::ellipse();
        mask.params.set("centerX", 0.1);
        let samples: Vec<tracking::TrackSample> = (0..=10).map(|i| tracking::TrackSample { time: i as f64 * 0.1, x: 0.5 + i as f64 * 0.02, y: 0.5 }).collect();
        mask.params.set(tracking::MASK_TRACK_PARAM, tracking::MaskTrack::from_samples(&samples, 30.0).unwrap().encode());
        element.masks = vec![mask];
        let layer = crate::frame::layer_plan_for_test(&project, &element, MediaTime::from_seconds_f64(0.5).unwrap()).unwrap();
        let mask = layer.mask.unwrap();
        assert!((mask.params.number("centerX", 0.0) - 0.2).abs() < 1e-3, "{}", mask.params.number("centerX", 0.0));
    }
}
