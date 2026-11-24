//! Face-only retouch on a clip: the clip keeps its face track
//! (`facetrack::FACE_TRACK_PARAM`) and its Retouch effect is set to "faces
//! only" (added at its defaults when the clip has none), in one undo step.
//! The web editor builds the same change (`apps/web/src/retouch`).

use facetrack::{FACE_TRACK_PARAM, FaceTrack};

use crate::commands::Command;
use crate::document::{ElementKind, Project};
use crate::params::Params;
use crate::{Effect, ElementPatch};

pub fn face_retouch_commands(project: &Project, element_id: &str, track: &FaceTrack) -> Result<Vec<Command>, String> {
    let (_, element) = project.element(element_id).ok_or("The clip is no longer on the timeline")?;
    if !matches!(element.kind, ElementKind::Video { .. } | ElementKind::Image { .. }) {
        return Err("Face retouch works on video and picture clips".to_owned());
    }
    let mut params = Params::new();
    params.set(FACE_TRACK_PARAM, track.encode());
    let mut commands =
        vec![Command::UpdateElement { element_id: element_id.to_owned(), patch: ElementPatch { params: Some(params), ..Default::default() } }];
    let mut area = Params::new();
    area.set("area", "faces");
    match element.effects.iter().find(|effect| effect.kind == "retouch") {
        Some(effect) => commands.push(Command::UpdateEffect {
            element_id: element_id.to_owned(),
            effect_id: effect.id.clone(),
            params: Some(area),
            enabled: Some(true),
        }),
        None => {
            let mut effect = Effect::from_catalogue("retouch").ok_or("The Retouch effect is missing")?;
            effect.params.set("area", "faces");
            commands.push(Command::AddEffect { element_id: element_id.to_owned(), effect });
        }
    }
    Ok(commands)
}
