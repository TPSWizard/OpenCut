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

/// Segments of a path: between consecutive points, plus the closing one.
pub fn freeform_segment_count(points: &[FreeformPoint], closed: bool) -> usize {
    match points.len() {
        0 | 1 => 0,
        count if closed => count,
        count => count - 1,
    }
}

fn lerp((ax, ay): (f64, f64), (bx, by): (f64, f64), t: f64) -> (f64, f64) {
    (ax + (bx - ax) * t, ay + (by - ay) * t)
}

/// A cubic Bézier's point at `t`.
pub fn cubic_point(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0,
        u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1,
    )
}

/// The four control points of a segment, in path units.
fn segment_controls(points: &[FreeformPoint], segment: usize) -> [(f64, f64); 4] {
    let start = &points[segment];
    let end = &points[(segment + 1) % points.len()];
    [
        (start.x, start.y),
        (start.x + start.out_x, start.y + start.out_y),
        (end.x + end.in_x, end.y + end.in_y),
        (end.x, end.y),
    ]
}

/// The spot on a segment nearest to a canvas pixel, as the segment's `t`
/// (kept just inside the ends) and its distance in canvas pixels: the web
/// app's sampled search (24 samples, then 8 halvings).
pub fn closest_on_freeform_segment(
    quad: &ElementQuad,
    mask: &Params,
    points: &[FreeformPoint],
    closed: bool,
    segment: usize,
    (x, y): (f64, f64),
) -> Option<(f64, f64)> {
    if segment >= freeform_segment_count(points, closed) {
        return None;
    }
    let controls = segment_controls(points, segment).map(|(px, py)| freeform_local_to_canvas(quad, mask, px, py));
    let at = |t: f64| cubic_point(controls[0], controls[1], controls[2], controls[3], t);
    let distance = |(px, py): (f64, f64)| ((px - x).powi(2) + (py - y).powi(2)).sqrt();
    let samples = 24;
    let mut best_t = 0.0;
    let mut best = distance(controls[0]);
    for step in 0..=samples {
        let t = step as f64 / samples as f64;
        let d = distance(at(t));
        if d < best {
            best = d;
            best_t = t;
        }
    }
    let mut span = 1.0 / samples as f64;
    for _ in 0..8 {
        for t in [best_t - span, best_t, best_t + span] {
            let t = t.clamp(0.0, 1.0);
            let d = distance(at(t));
            if d < best {
                best = d;
                best_t = t;
            }
        }
        span /= 2.0;
    }
    let t = best_t.clamp(0.001, 0.999);
    Some((t, distance(at(t))))
}

/// Splits a segment at `t`, inserting a point there: a corner on a straight
/// segment, otherwise a smooth point with the split curve's handles (so the
/// path's shape does not change).
pub fn insert_freeform_point(points: &[FreeformPoint], closed: bool, segment: usize, t: f64) -> Vec<FreeformPoint> {
    if segment >= freeform_segment_count(points, closed) {
        return points.to_vec();
    }
    let t = t.clamp(0.001, 0.999);
    let end_index = (segment + 1) % points.len();
    let mut next = points.to_vec();
    let start = points[segment].clone();
    let end = points[end_index].clone();
    let straight = start.out_x == 0.0 && start.out_y == 0.0 && end.in_x == 0.0 && end.in_y == 0.0;
    if straight {
        next.insert(end_index, FreeformPoint::corner(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t));
        return next;
    }
    let [p0, p1, p2, p3] = segment_controls(points, segment);
    let p01 = lerp(p0, p1, t);
    let p12 = lerp(p1, p2, t);
    let p23 = lerp(p2, p3, t);
    let p012 = lerp(p01, p12, t);
    let p123 = lerp(p12, p23, t);
    let split = lerp(p012, p123, t);
    next[segment].out_x = p01.0 - start.x;
    next[segment].out_y = p01.1 - start.y;
    next[end_index].in_x = p23.0 - end.x;
    next[end_index].in_y = p23.1 - end.y;
    next.insert(
        end_index,
        FreeformPoint {
            id: crate::new_id(),
            x: split.0,
            y: split.1,
            in_x: p012.0 - split.0,
            in_y: p012.1 - split.1,
            out_x: p123.0 - split.0,
            out_y: p123.1 - split.1,
        },
    );
    next
}

/// Drops points by id; a closed path stays closed only with three or more
/// points left, as in the web app.
pub fn remove_freeform_points(points: &[FreeformPoint], ids: &[String], closed: bool) -> (Vec<FreeformPoint>, bool) {
    let kept: Vec<FreeformPoint> = points.iter().filter(|point| !ids.contains(&point.id)).cloned().collect();
    let still_closed = closed && kept.len() >= 3;
    (kept, still_closed)
}

/// Moves the mask's centre to the middle of the path's bounding box
/// (anchors and handles, in the element's own pixels) and re-expresses
/// the points from there, as the web app does after every change to a
/// path, so the mask's position handles sit on the shape. Returns the new
/// `centerX`/`centerY` (in element units) and points.
pub fn recenter_freeform_path(quad: &ElementQuad, mask: &Params, points: &[FreeformPoint]) -> (f64, f64, Vec<FreeformPoint>) {
    let old_center = (mask.number("centerX", 0.0), mask.number("centerY", 0.0));
    if points.is_empty() || quad.width <= 0.0 || quad.height <= 0.0 {
        return (old_center.0, old_center.1, points.to_vec());
    }
    let rotation = mask.number("rotation", 0.0);
    let scale = mask.number("scale", 1.0).max(0.01);
    // Path units → the element's own pixels (unrotated element frame).
    let to_element = |center: (f64, f64), x: f64, y: f64| {
        let (rx, ry) = rotate(x * quad.width * scale, y * quad.height * scale, rotation);
        (rx + center.0 * quad.width, ry + center.1 * quad.height)
    };
    let to_local = |center: (f64, f64), ex: f64, ey: f64| {
        let (rx, ry) = rotate(ex - center.0 * quad.width, ey - center.1 * quad.height, -rotation);
        (rx / (quad.width * scale), ry / (quad.height * scale))
    };
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in points {
        for (x, y) in [
            (point.x, point.y),
            (point.x + point.in_x, point.y + point.in_y),
            (point.x + point.out_x, point.y + point.out_y),
        ] {
            let (ex, ey) = to_element(old_center, x, y);
            min = (min.0.min(ex), min.1.min(ey));
            max = (max.0.max(ex), max.1.max(ey));
        }
    }
    let new_center = ((min.0 + max.0) / 2.0 / quad.width, (min.1 + max.1) / 2.0 / quad.height);
    let moved = points
        .iter()
        .map(|point| {
            let (ax, ay) = to_element(old_center, point.x, point.y);
            let (ix, iy) = to_element(old_center, point.x + point.in_x, point.y + point.in_y);
            let (ox, oy) = to_element(old_center, point.x + point.out_x, point.y + point.out_y);
            let anchor = to_local(new_center, ax, ay);
            let in_handle = to_local(new_center, ix, iy);
            let out_handle = to_local(new_center, ox, oy);
            FreeformPoint {
                id: point.id.clone(),
                x: anchor.0,
                y: anchor.1,
                in_x: in_handle.0 - anchor.0,
                in_y: in_handle.1 - anchor.1,
                out_x: out_handle.0 - anchor.0,
                out_y: out_handle.1 - anchor.1,
            }
        })
        .collect();
    (new_center.0, new_center.1, moved)
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

    fn square() -> Vec<FreeformPoint> {
        vec![
            FreeformPoint::corner(-0.5, -0.5),
            FreeformPoint::corner(0.5, -0.5),
            FreeformPoint::corner(0.5, 0.5),
            FreeformPoint::corner(-0.5, 0.5),
        ]
    }

    #[test]
    fn points_are_inserted_on_segments_and_removed() {
        let points = square();
        assert_eq!(freeform_segment_count(&points, true), 4);
        assert_eq!(freeform_segment_count(&points, false), 3);
        assert_eq!(freeform_segment_count(&points[..1], true), 0);

        // The closing segment of the square runs along its left edge; the
        // nearest spot to a pixel left of the middle is halfway down it.
        let mask = Params::new();
        let (t, distance) = closest_on_freeform_segment(&quad(), &mask, &points, true, 3, (140.0, 100.0)).unwrap();
        assert!((t - 0.5).abs() < 1e-3, "{t}");
        assert!((distance - 10.0).abs() < 1e-6, "{distance}");
        assert!(closest_on_freeform_segment(&quad(), &mask, &points, false, 3, (140.0, 100.0)).is_none());

        // Inserting on a straight segment adds a corner and leaves the others alone.
        let inserted = insert_freeform_point(&points, true, 3, t);
        assert_eq!(inserted.len(), 5);
        assert_eq!(inserted[4].id, points[3].id, "the new point goes between the segment's ends");
        assert!((inserted[0].x + 0.5).abs() < 1e-9 && inserted[0].y.abs() < 1e-3, "{:?}", inserted[0]);
        assert_eq!((inserted[0].in_x, inserted[0].out_x), (0.0, 0.0));
        assert_eq!(&inserted[1..], &points[..], "the other points are unchanged");
    }

    #[test]
    fn splitting_a_curved_segment_keeps_its_shape() {
        let mut curve = square();
        curve[0].out_x = 0.3;
        curve[0].out_y = -0.4;
        curve[1].in_x = -0.3;
        curve[1].in_y = -0.4;
        let [p0, p1, p2, p3] = segment_controls(&curve, 0);
        let before: Vec<(f64, f64)> = (0..=10).map(|i| cubic_point(p0, p1, p2, p3, i as f64 / 10.0)).collect();
        let split = insert_freeform_point(&curve, true, 0, 0.3);
        assert_eq!(split.len(), 5);
        assert_ne!(split[1].in_x, 0.0, "a smooth point with handles");
        let [a0, a1, a2, a3] = segment_controls(&split, 0);
        let [b0, b1, b2, b3] = segment_controls(&split, 1);
        for (i, expected) in before.iter().enumerate() {
            let t = i as f64 / 10.0;
            let actual = if t <= 0.3 {
                cubic_point(a0, a1, a2, a3, t / 0.3)
            } else {
                cubic_point(b0, b1, b2, b3, (t - 0.3) / 0.7)
            };
            assert!((actual.0 - expected.0).abs() < 1e-9 && (actual.1 - expected.1).abs() < 1e-9, "t={t}: {actual:?} vs {expected:?}");
        }
        let (kept, closed) = remove_freeform_points(&split, &[split[1].id.clone(), split[2].id.clone()], true);
        assert_eq!(kept.len(), 3);
        assert!(closed);
        let (kept, closed) = remove_freeform_points(&kept, &[kept[0].id.clone()], true);
        assert_eq!(kept.len(), 2);
        assert!(!closed, "a closed path needs three points");
    }

    #[test]
    fn recentering_moves_the_centre_without_moving_the_shape() {
        let quad = quad();
        let mut mask = Params::new();
        mask.set("rotation", 30.0).set("scale", 1.5);
        let mut points = square();
        points[2].x = 1.5; // lopsided: the bbox centre is off the origin
        points[2].out_x = 0.5;
        let canvas_before: Vec<(f64, f64)> =
            points.iter().map(|p| freeform_local_to_canvas(&quad, &mask, p.x, p.y)).collect();
        let (cx, cy, moved) = recenter_freeform_path(&quad, &mask, &points);
        assert!(cx != 0.0, "centre moved");
        let mut recentered = mask.clone();
        recentered.set("centerX", cx).set("centerY", cy);
        for (point, before) in moved.iter().zip(&canvas_before) {
            let after = freeform_local_to_canvas(&quad, &recentered, point.x, point.y);
            assert!((after.0 - before.0).abs() < 1e-9 && (after.1 - before.1).abs() < 1e-9, "{after:?} vs {before:?}");
        }
        // Handles keep their canvas positions too.
        let out_before = freeform_local_to_canvas(&quad, &mask, points[2].x + points[2].out_x, points[2].y + points[2].out_y);
        let out_after = freeform_local_to_canvas(&quad, &recentered, moved[2].x + moved[2].out_x, moved[2].y + moved[2].out_y);
        assert!((out_before.0 - out_after.0).abs() < 1e-9 && (out_before.1 - out_after.1).abs() < 1e-9);
        // The bbox is centred on the new centre, in the element's frame.
        let (ex, ey) = {
            let mut min = (f64::INFINITY, f64::INFINITY);
            let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in &moved {
                for (x, y) in [(p.x, p.y), (p.x + p.in_x, p.y + p.in_y), (p.x + p.out_x, p.y + p.out_y)] {
                    let (rx, ry) = rotate(x * quad.width * 1.5, y * quad.height * 1.5, 30.0);
                    min = (min.0.min(rx), min.1.min(ry));
                    max = (max.0.max(rx), max.1.max(ry));
                }
            }
            ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0)
        };
        assert!(ex.abs() < 1e-9 && ey.abs() < 1e-9, "{ex} {ey}");
        assert_eq!(recenter_freeform_path(&quad, &mask, &[]).2.len(), 0);
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
