//! Freehand drawing on the video (row 90): adding a stroke to a drawing
//! element. The strokes themselves are the shared `strokes` crate.

use time::MediaTime;

use crate::{Command, Element, ElementKind, ElementPatch, Params, Project, TrackPlacement};

/// Whether an element is a freehand drawing.
pub fn is_drawing(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Graphic { definition_id } if definition_id == crate::DRAWING_DEFINITION)
}

/// The command that adds `stroke`: onto `selected` when that is a drawing
/// showing at `playhead`, else a new drawing at the playhead (one undo step
/// either way). `None` for a stroke without points.
pub fn add_stroke_command(project: &Project, selected: Option<&str>, playhead: MediaTime, stroke: strokes::Stroke) -> Option<Command> {
    if stroke.points.is_empty() {
        return None;
    }
    let target = selected
        .and_then(|id| project.element(id))
        .map(|(_, element)| element)
        .filter(|element| is_drawing(element) && element.start_time <= playhead && playhead < element.end_time());
    match target {
        Some(element) => {
            let mut drawing = strokes::Drawing::parse(&element.params.text("strokes", "")).unwrap_or_default();
            drawing.push_stroke(stroke);
            let mut params = Params::new();
            params.set("strokes", drawing.encode());
            Some(Command::UpdateElement {
                element_id: element.id.clone(),
                patch: ElementPatch { params: Some(params), ..Default::default() },
            })
        }
        None => {
            let mut drawing = strokes::Drawing::default();
            drawing.push_stroke(stroke);
            Some(Command::InsertElement { element: Element::drawing(drawing.encode(), playhead), placement: TrackPlacement::Auto })
        }
    }
}

/// The command that takes a drawing's last stroke away (`None` when it
/// has none).
pub fn remove_last_stroke_command(element: &Element) -> Option<Command> {
    let mut drawing = strokes::Drawing::parse(&element.params.text("strokes", ""))?;
    drawing.strokes.pop()?;
    let mut params = Params::new();
    params.set("strokes", drawing.encode());
    Some(Command::UpdateElement { element_id: element.id.clone(), patch: ElementPatch { params: Some(params), ..Default::default() } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply;

    fn stroke(x: f64) -> strokes::Stroke {
        strokes::Stroke {
            tool: strokes::Tool::Marker,
            color: "#ff0000".to_owned(),
            width: 0.01,
            points: vec![
                strokes::Point { x, y: 0.5, pressure: 0.5, time: 3.0 },
                strokes::Point { x: x + 0.1, y: 0.5, pressure: 0.5, time: 3.5 },
            ],
        }
    }

    #[test]
    fn strokes_start_a_drawing_then_join_it() {
        let mut project = Project::new("Draw");
        let at = MediaTime::from_seconds_f64(1.0).unwrap();
        let command = add_stroke_command(&project, None, at, stroke(0.1)).unwrap();
        apply(&mut project, &command).unwrap();
        let drawing = project.tracks.iter().flat_map(|track| &track.elements).find(|element| is_drawing(element)).unwrap().clone();
        assert_eq!(drawing.start_time, at);
        // Selected and under the playhead: the next stroke joins it.
        let command = add_stroke_command(&project, Some(&drawing.id), at, stroke(0.5)).unwrap();
        assert!(matches!(command, Command::UpdateElement { .. }));
        apply(&mut project, &command).unwrap();
        let element = project.element(&drawing.id).unwrap().1;
        let parsed = strokes::Drawing::parse(&element.params.text("strokes", "")).unwrap();
        assert_eq!(parsed.strokes.len(), 2);
        assert_eq!(parsed.strokes[1].points[0].time, 0.65, "timed after the first stroke");
        // Undo the last stroke.
        let undo = remove_last_stroke_command(element).unwrap();
        apply(&mut project, &undo).unwrap();
        let element = project.element(&drawing.id).unwrap().1;
        assert_eq!(strokes::Drawing::parse(&element.params.text("strokes", "")).unwrap().strokes.len(), 1);
        // Away from it, a new drawing starts.
        let later = MediaTime::from_seconds_f64(30.0).unwrap();
        assert!(matches!(add_stroke_command(&project, Some(&drawing.id), later, stroke(0.2)), Some(Command::InsertElement { .. })));
        // The layer plan draws it as a drawing, drawn on with drawOn.
        let plan = crate::plan_frame(&project, MediaTime::from_seconds_f64(1.5).unwrap());
        assert!(plan.layers().iter().any(|layer| matches!(&layer.source, crate::LayerSource::Drawing { progress, .. } if *progress == 1.0)));
    }

    #[test]
    fn a_drawing_round_trips_as_the_web_graphic_element() {
        let element = Element::drawing("{\"strokes\":[]}", MediaTime::ZERO);
        let json = serde_json::to_value(&element).unwrap();
        assert_eq!(json["type"], "graphic");
        assert_eq!(json["definitionId"], "drawing");
        let back: Element = serde_json::from_value(json).unwrap();
        assert!(is_drawing(&back));
    }
}
