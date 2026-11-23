//! Motion tracking for the web editor: the shared `tracking` crate, so the
//! web and the desktop follow points identically.

use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// Follows one point through RGBA frames of the same size.
#[wasm_bindgen]
pub struct PointTracker {
    inner: ::tracking::Tracker,
    width: usize,
    height: usize,
}

#[wasm_bindgen]
impl PointTracker {
    /// Starts at (`x`, `y`) pixels of the first frame, with a box
    /// `box_width` × `box_height` pixels around it.
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: &[u8], width: usize, height: usize, x: f64, y: f64, box_width: f64, box_height: f64) -> Result<PointTracker, JsError> {
        if rgba.len() < width * height * 4 {
            return Err(JsError::new("the frame is smaller than its size"));
        }
        let frame = ::tracking::Gray::from_rgba(rgba, width, height);
        let inner = ::tracking::Tracker::new(&frame, x, y, box_width, box_height).map_err(|message| JsError::new(&message))?;
        Ok(PointTracker { inner, width, height })
    }

    /// The point in the next frame: `[x, y, score, lost (0 or 1)]` (pixels).
    pub fn step(&mut self, rgba: &[u8]) -> Result<Vec<f64>, JsError> {
        if rgba.len() < self.width * self.height * 4 {
            return Err(JsError::new("the frame is smaller than its size"));
        }
        let step = self.inner.step(&::tracking::Gray::from_rgba(rgba, self.width, self.height));
        Ok(vec![step.x, step.y, step.score, if step.lost { 1.0 } else { 0.0 }])
    }

    /// The point where tracking starts (pixels; moved off the edge when needed).
    pub fn position(&self) -> Vec<f64> {
        let (x, y) = self.inner.position();
        vec![x, y]
    }
}

/// Position keys for a follower: `times`, `xs`, `ys` are the tracked point
/// (seconds, shares of the frame); the clip's centre and drawn size place it
/// on the canvas; the follower keeps `offset_x`, `offset_y` from it. Returns
/// `[time, x, y, …]` simplified to `tolerance` canvas pixels.
#[wasm_bindgen(js_name = followKeys)]
#[allow(clippy::too_many_arguments)]
pub fn follow_keys(
    times: &[f64],
    xs: &[f64],
    ys: &[f64],
    position_x: f64,
    position_y: f64,
    drawn_width: f64,
    drawn_height: f64,
    offset_x: f64,
    offset_y: f64,
    tolerance: f64,
) -> Vec<f64> {
    let samples: Vec<::tracking::TrackSample> =
        times.iter().zip(xs).zip(ys).map(|((time, x), y)| ::tracking::TrackSample { time: *time, x: *x, y: *y }).collect();
    let clip = ::tracking::ClipPlacement { position_x, position_y, drawn_width, drawn_height };
    ::tracking::follow_keys(&samples, clip, (offset_x, offset_y), tolerance).into_iter().flat_map(|(t, x, y)| [t, x, y]).collect()
}
