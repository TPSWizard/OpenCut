//! Freeform (pen) masks. A path's points live in units of the element's
//! size, relative to the mask's centre and rotation, exactly as the web
//! app stores them; this maps them to and from canvas pixels so a pen tool
//! can draw them on the preview.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::MediaTime;

use crate::document::{Element, ElementKind, Project};
use crate::frame::{LayerTransform, fit_contain};
use crate::params::Params;

/// One anchor of a freeform path with its two Bézier handles (relative to
/// the anchor), all in units of the element's size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeformPoint {
    pub id: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub in_x: f64,
    #[serde(default)]
    pub in_y: f64,
    #[serde(default)]
    pub out_x: f64,
    #[serde(default)]
    pub out_y: f64,
}

impl FreeformPoint {
    /// A corner point: no handles.
    pub fn corner(x: f64, y: f64) -> Self {
        Self {
            id: crate::new_id(),
            x,
            y,
            in_x: 0.0,
            in_y: 0.0,
            out_x: 0.0,
            out_y: 0.0,
        }
    }
}

/// The mask's `path` parameter as points; entries that are not points are
/// dropped, as the web app does.
pub fn read_freeform_path(params: &Params) -> Vec<FreeformPoint> {
    params
        .get("path")
        .map(|value| value.as_json())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|point| serde_json::from_value(point).ok())
        .collect()
}

pub fn write_freeform_path(points: &[FreeformPoint]) -> Value {
    serde_json::to_value(points).unwrap_or_else(|_| Value::Array(Vec::new()))
}

/// Where a video or image element sits on the canvas at a time, in canvas
/// pixels: the same placement the renderer draws it at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementQuad {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
}

pub fn element_quad(project: &Project, element: &Element, time: MediaTime) -> Option<ElementQuad> {
    let (natural_width, natural_height) = match &element.kind {
        ElementKind::Video { .. } | ElementKind::Image { .. } => {
            let media = project.media_for(element)?;
            (
                media.width.unwrap_or(1920).max(1) as f32,
                media.height.unwrap_or(1080).max(1) as f32,
            )
        }
        _ => return None,
    };
    let canvas = project.settings.canvas;
    let transform = LayerTransform::from_params(&element.params_at(time));
    let (width, height) = fit_contain(natural_width, natural_height, canvas.width as f32, canvas.height as f32);
    Some(ElementQuad {
        center_x: canvas.width as f64 / 2.0 + transform.position_x as f64,
        center_y: canvas.height as f64 / 2.0 + transform.position_y as f64,
        width: (width * transform.scale_x.abs()) as f64,
        height: (height * transform.scale_y.abs()) as f64,
        rotation_degrees: transform.rotate_degrees as f64,
    })
}

fn rotate(x: f64, y: f64, degrees: f64) -> (f64, f64) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// A canvas pixel as a point of the mask's path (the web app's
/// `freeformCanvasPointToLocal`, after undoing the element's own rotation).
pub fn canvas_to_freeform_local(quad: &ElementQuad, mask: &Params, x: f64, y: f64) -> (f64, f64) {
    let (ex, ey) = rotate(x - quad.center_x, y - quad.center_y, -quad.rotation_degrees);
    let center_x = mask.number("centerX", 0.0) * quad.width;
    let center_y = mask.number("centerY", 0.0) * quad.height;
    let (rx, ry) = rotate(ex - center_x, ey - center_y, -mask.number("rotation", 0.0));
    let scale = mask.number("scale", 1.0).max(0.01);
    (
        if quad.width > 0.0 { rx / (quad.width * scale) } else { 0.0 },
        if quad.height > 0.0 { ry / (quad.height * scale) } else { 0.0 },
    )
}

/// A point of the mask's path as a canvas pixel.
pub fn freeform_local_to_canvas(quad: &ElementQuad, mask: &Params, local_x: f64, local_y: f64) -> (f64, f64) {
    let scale = mask.number("scale", 1.0).max(0.01);
    let (rx, ry) = rotate(
        local_x * quad.width * scale,
        local_y * quad.height * scale,
        mask.number("rotation", 0.0),
    );
    let ex = rx + mask.number("centerX", 0.0) * quad.width;
    let ey = ry + mask.number("centerY", 0.0) * quad.height;
    let (x, y) = rotate(ex, ey, quad.rotation_degrees);
    (quad.center_x + x, quad.center_y + y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, TrackPlacement, apply};
    use crate::document::{MediaItem, MediaKind};

    fn quad() -> ElementQuad {
        ElementQuad {
            center_x: 200.0,
            center_y: 100.0,
            width: 100.0,
            height: 50.0,
            rotation_degrees: 0.0,
        }
    }

    #[test]
    fn maps_canvas_pixels_to_path_units_and_back() {
        let mask = Params::new();
        assert_eq!(canvas_to_freeform_local(&quad(), &mask, 250.0, 100.0), (0.5, 0.0));
        assert_eq!(canvas_to_freeform_local(&quad(), &mask, 200.0, 125.0), (0.0, 0.5));
        assert_eq!(freeform_local_to_canvas(&quad(), &mask, 0.5, 0.0), (250.0, 100.0));

        // A mask offset by a quarter and doubled: the same pixel is nearer its centre.
        let mut mask = Params::new();
        mask.set("centerX", 0.25).set("scale", 2.0);
        let (x, y) = canvas_to_freeform_local(&quad(), &mask, 250.0, 100.0);
        assert!((x - 0.125).abs() < 1e-9 && y.abs() < 1e-9, "{x} {y}");

        // Element rotated 90°: its local x axis points down the screen.
        let mut turned = quad();
        turned.rotation_degrees = 90.0;
        let (x, y) = canvas_to_freeform_local(&turned, &Params::new(), 200.0, 150.0);
        assert!((x - 0.5).abs() < 1e-9 && y.abs() < 1e-9, "{x} {y}");

        let mut mask = Params::new();
        mask.set("centerX", -0.1).set("centerY", 0.2).set("rotation", 33.0).set("scale", 1.5);
        for (px, py) in [(210.0, 90.0), (0.0, 0.0), (333.3, 77.7)] {
            let (lx, ly) = canvas_to_freeform_local(&turned, &mask, px, py);
            let (bx, by) = freeform_local_to_canvas(&turned, &mask, lx, ly);
            assert!((bx - px).abs() < 1e-6 && (by - py).abs() < 1e-6, "{px},{py} -> {bx},{by}");
        }
    }

    #[test]
    fn paths_round_trip_through_params_and_elements_are_placed_like_the_renderer() {
        let points = vec![FreeformPoint::corner(-0.3, -0.3), FreeformPoint::corner(0.3, 0.3)];
        let mut params = Params::new();
        params.set("path", write_freeform_path(&points));
        assert_eq!(read_freeform_path(&params), points);
        params.set("path", "[{\"id\":\"a\",\"x\":0.1,\"y\":0.2},{\"nope\":1}]");
        let read = read_freeform_path(&params);
        assert_eq!(read.len(), 1, "JSON text is read too and non-points are dropped");
        assert_eq!((read[0].x, read[0].out_x), (0.1, 0.0));
        assert!(read_freeform_path(&Params::new()).is_empty());

        let mut project = Project::new("Quad");
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "img".to_owned(),
                name: "img.png".to_owned(),
                kind: MediaKind::Image,
                path: "img.png".into(),
                duration: None,
                width: Some(400),
                height: Some(300),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let mut image = Element::from_media(project.media_item("img").unwrap(), MediaTime::ZERO);
        image.params.set("transform.scaleX", 0.5).set("transform.positionX", 100.0).set("transform.rotate", 10.0);
        let id = image.id.clone();
        apply(&mut project, &Command::InsertElement { element: image, placement: TrackPlacement::Auto }).unwrap();
        let (_, element) = project.element(&id).unwrap();
        // 400x300 fits a 1920x1080 canvas as 1440x1080; half as wide.
        let quad = element_quad(&project, element, MediaTime::ZERO).unwrap();
        assert_eq!(quad.width, 720.0);
        assert_eq!(quad.height, 1080.0);
        assert_eq!((quad.center_x, quad.center_y), (1060.0, 540.0));
        assert_eq!(quad.rotation_degrees, 10.0);
        let text = Element::text("x", MediaTime::ZERO);
        assert!(element_quad(&project, &text, MediaTime::ZERO).is_none());
    }
}
