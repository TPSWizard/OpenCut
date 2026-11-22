//! Split-screen layouts: 2, 3 or 4 clips side by side, stacked or in a
//! grid, in one step (the web app's `src/split-screen/layout.ts`, checked
//! against the same `fixtures/split-screen/cases.json`). Each clip is
//! scaled to cover its cell, moved to the cell's centre and given a
//! rectangle mask cropped to the cell.

use crate::{Command, ElementPatch, Mask, Params, Project};

/// `(id, label, cells)`; cells are `[x, y, width, height]` shares of the canvas.
pub const SPLIT_LAYOUTS: &[(&str, &str, &[[f64; 4]])] = &[
    ("side-by-side", "Side by side", &[[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 1.0]]),
    ("top-bottom", "Top and bottom", &[[0.0, 0.0, 1.0, 0.5], [0.0, 0.5, 1.0, 0.5]]),
    (
        "three-columns",
        "Three columns",
        &[[0.0, 0.0, 1.0 / 3.0, 1.0], [1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0], [2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0]],
    ),
    (
        "three-rows",
        "Three rows",
        &[[0.0, 0.0, 1.0, 1.0 / 3.0], [0.0, 1.0 / 3.0, 1.0, 1.0 / 3.0], [0.0, 2.0 / 3.0, 1.0, 1.0 / 3.0]],
    ),
    ("one-and-two", "One big, two small", &[[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]]),
    (
        "grid",
        "Grid of four",
        &[[0.0, 0.0, 0.5, 0.5], [0.5, 0.0, 0.5, 0.5], [0.0, 0.5, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]],
    ),
];

/// The cells of a layout by id.
pub fn split_layout_cells(id: &str) -> Option<&'static [[f64; 4]]> {
    SPLIT_LAYOUTS.iter().find(|(layout, _, _)| *layout == id).map(|(_, _, cells)| *cells)
}

/// Where one clip goes: its transform and the rectangle mask (shares of its own frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitPlacement {
    pub position_x: f64,
    pub position_y: f64,
    pub scale: f64,
    pub mask_width: f64,
    pub mask_height: f64,
}

/// The placement of a clip of `source` size in `cell` of a `canvas`, with
/// `gap` canvas pixels between cells (none at the canvas edge).
pub fn place_in_cell(cell: [f64; 4], canvas: (f64, f64), source: (f64, f64), gap: f64) -> SplitPlacement {
    let [x, y, w, h] = cell;
    let (canvas_width, canvas_height) = canvas;
    let half = gap.max(0.0) / 2.0;
    let left = x * canvas_width + if x > 1e-9 { half } else { 0.0 };
    let right = (x + w) * canvas_width - if x + w < 1.0 - 1e-9 { half } else { 0.0 };
    let top = y * canvas_height + if y > 1e-9 { half } else { 0.0 };
    let bottom = (y + h) * canvas_height - if y + h < 1.0 - 1e-9 { half } else { 0.0 };
    let cell_width = (right - left).max(1.0);
    let cell_height = (bottom - top).max(1.0);
    let (source_width, source_height) = (source.0.max(1.0), source.1.max(1.0));
    let fit = (canvas_width / source_width).min(canvas_height / source_height);
    let (fit_width, fit_height) = (source_width * fit, source_height * fit);
    let scale = (cell_width / fit_width).max(cell_height / fit_height);
    SplitPlacement {
        position_x: (left + right) / 2.0 - canvas_width / 2.0,
        position_y: (top + bottom) / 2.0 - canvas_height / 2.0,
        scale,
        mask_width: (cell_width / (fit_width * scale)).min(1.0),
        mask_height: (cell_height / (fit_height * scale)).min(1.0),
    }
}

/// The placements of clips of `sources` sizes, in order, for a layout;
/// `None` for an unknown layout or a count that does not match its cells.
pub fn plan_split_screen(layout: &str, canvas: (f64, f64), sources: &[(f64, f64)], gap: f64) -> Option<Vec<SplitPlacement>> {
    let cells = split_layout_cells(layout)?;
    (cells.len() == sources.len())
        .then(|| cells.iter().zip(sources).map(|(cell, source)| place_in_cell(*cell, canvas, *source, gap)).collect())
}

/// The commands that put `element_ids` (video or picture clips, in cell
/// order) into a layout: transform keyframes cleared, the transform set,
/// earlier masks replaced by the cell's rectangle. `None` when a clip is
/// missing, has no known size, or the count does not fit the layout.
pub fn split_screen_commands(project: &Project, layout: &str, element_ids: &[String], gap: f64) -> Option<Vec<Command>> {
    let mut sources = Vec::new();
    for id in element_ids {
        let element = project.tracks.iter().flat_map(|track| &track.elements).find(|element| &element.id == id)?;
        let media = match &element.kind {
            crate::ElementKind::Video { media_id } | crate::ElementKind::Image { media_id } => project.media_item(media_id)?,
            _ => return None,
        };
        sources.push((media.width? as f64, media.height? as f64));
    }
    let canvas = (project.settings.canvas.width as f64, project.settings.canvas.height as f64);
    let placements = plan_split_screen(layout, canvas, &sources, gap)?;
    let mut commands = Vec::new();
    for (id, placement) in element_ids.iter().zip(placements) {
        let element = project.tracks.iter().flat_map(|track| &track.elements).find(|element| &element.id == id)?;
        for path in ["transform.positionX", "transform.positionY", "transform.scaleX", "transform.scaleY", "transform.rotate"] {
            commands.push(Command::ClearAnimation { element_id: id.clone(), path: path.to_owned() });
        }
        let mut params = Params::new();
        params
            .set("transform.positionX", placement.position_x)
            .set("transform.positionY", placement.position_y)
            .set("transform.scaleX", placement.scale)
            .set("transform.scaleY", placement.scale)
            .set("transform.rotate", 0.0);
        commands.push(Command::UpdateElement { element_id: id.clone(), patch: ElementPatch { params: Some(params), ..Default::default() } });
        for mask in &element.masks {
            commands.push(Command::RemoveMask { element_id: id.clone(), mask_id: mask.id.clone() });
        }
        let mut mask = Mask::rectangle();
        mask.params.set("width", placement.mask_width).set("height", placement.mask_height);
        commands.push(Command::AddMask { element_id: id.clone(), mask });
    }
    Some(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_the_shared_cases_like_the_web() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/split-screen/cases.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() > 3);
        for case in cases {
            let canvas = (case["canvas"][0].as_f64().unwrap(), case["canvas"][1].as_f64().unwrap());
            let sources: Vec<(f64, f64)> = case["sources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|size| (size[0].as_f64().unwrap(), size[1].as_f64().unwrap()))
                .collect();
            let planned = plan_split_screen(case["layout"].as_str().unwrap(), canvas, &sources, case["gap"].as_f64().unwrap()).unwrap();
            for (placement, expected) in planned.iter().zip(case["placements"].as_array().unwrap()) {
                let close = |value: f64, key: &str| (value - expected[key].as_f64().unwrap()).abs() < 1e-9;
                assert!(
                    close(placement.position_x, "positionX")
                        && close(placement.position_y, "positionY")
                        && close(placement.scale, "scale")
                        && close(placement.mask_width, "maskWidth")
                        && close(placement.mask_height, "maskHeight"),
                    "{}: {placement:?} vs {expected}",
                    case["name"]
                );
            }
        }
        assert!(plan_split_screen("grid", (10.0, 10.0), &[(1.0, 1.0)], 0.0).is_none());
        assert!(plan_split_screen("nope", (10.0, 10.0), &[], 0.0).is_none());
    }

    #[test]
    fn commands_place_clips_and_crop_them_in_one_batch() {
        use crate::{MediaItem, MediaKind, TrackPlacement, apply};
        let mut project = Project::new("Split");
        let mut ids = Vec::new();
        for id in ["a", "b"] {
            apply(
                &mut project,
                &Command::AddMedia(MediaItem {
                    id: id.to_owned(),
                    name: format!("{id}.png"),
                    kind: MediaKind::Image,
                    path: format!("{id}.png").into(),
                    duration: None,
                    width: Some(1920),
                    height: Some(1080),
                    fps: None,
                    has_audio: false,
                }),
            )
            .unwrap();
            let mut clip = crate::Element::from_media(project.media_item(id).unwrap(), time::MediaTime::ZERO);
            clip.masks.push(Mask::ellipse());
            clip.params.set("transform.rotate", 30.0);
            ids.push(clip.id.clone());
            apply(&mut project, &Command::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();
        }
        assert!(split_screen_commands(&project, "grid", &ids, 0.0).is_none(), "two clips do not fill a grid");
        let commands = split_screen_commands(&project, "side-by-side", &ids, 0.0).unwrap();
        apply(&mut project, &Command::Batch(commands)).unwrap();
        let find = |id: &str| project.tracks.iter().flat_map(|track| &track.elements).find(|element| element.id == id).unwrap();
        let (left, right) = (find(&ids[0]), find(&ids[1]));
        assert_eq!(left.params.number("transform.positionX", 0.0), -480.0);
        assert_eq!(right.params.number("transform.positionX", 0.0), 480.0);
        assert_eq!(left.params.number("transform.rotate", 1.0), 0.0);
        assert_eq!(left.masks.len(), 1);
        assert_eq!(left.masks[0].kind, Mask::RECTANGLE);
        assert_eq!((left.masks[0].params.number("width", 0.0), left.masks[0].params.number("height", 0.0)), (0.5, 1.0));
    }
}
