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

/// A tracked mask's stored motion (row 92): `times` (clip seconds) and the
/// tracked centre `xs`, `ys` (shares of the frame) as the mask's `track`
/// param, offsets from the first sample at `rate` a second.
#[wasm_bindgen(js_name = maskTrackEncode)]
pub fn mask_track_encode(times: &[f64], xs: &[f64], ys: &[f64], rate: f64) -> Result<String, JsError> {
    let samples: Vec<::tracking::TrackSample> =
        times.iter().zip(xs).zip(ys).map(|((time, x), y)| ::tracking::TrackSample { time: *time, x: *x, y: *y }).collect();
    ::tracking::MaskTrack::from_samples(&samples, rate).map(|track| track.encode()).ok_or_else(|| JsError::new("Not enough of the video was tracked"))
}

/// The offset `[dx, dy]` (shares of the clip's box) a mask's stored track
/// gives `seconds` into its clip.
#[wasm_bindgen(js_name = maskTrackOffset)]
pub fn mask_track_offset(track: &str, seconds: f64) -> Vec<f64> {
    let (dx, dy) = ::tracking::mask_offset(track, seconds);
    vec![dx, dy]
}

/// Corner pin keys from four tracked corners: `times` (seconds) shared by
/// all, `xs` and `ys` the four corners' tracks one after another (shares
/// of the frame), the clip's centre and drawn size, `starts` the pin's
/// corners on the canvas (pixels from its centre, x and y for each).
/// Returns `[time, x0, y0, x1, y1, x2, y2, x3, y3, …]`.
#[wasm_bindgen(js_name = cornerKeys)]
#[allow(clippy::too_many_arguments)]
pub fn corner_keys(
    times: &[f64],
    xs: &[f64],
    ys: &[f64],
    position_x: f64,
    position_y: f64,
    drawn_width: f64,
    drawn_height: f64,
    starts: &[f64],
    tolerance: f64,
) -> Result<Vec<f64>, JsError> {
    let count = times.len();
    if count == 0 || xs.len() != count * 4 || ys.len() != count * 4 || starts.len() != 8 {
        return Err(JsError::new("four tracks of equal length and four start corners are needed"));
    }
    let tracks: [Vec<::tracking::TrackSample>; 4] = std::array::from_fn(|corner| {
        (0..count).map(|index| ::tracking::TrackSample { time: times[index], x: xs[corner * count + index], y: ys[corner * count + index] }).collect()
    });
    let clip = ::tracking::ClipPlacement { position_x, position_y, drawn_width, drawn_height };
    let start: [(f64, f64); 4] = std::array::from_fn(|corner| (starts[corner * 2], starts[corner * 2 + 1]));
    let mut out = Vec::new();
    for (time, corners) in ::tracking::corner_keys(&tracks, clip, start, tolerance) {
        out.push(time);
        for (x, y) in corners {
            out.push(x);
            out.push(y);
        }
    }
    Ok(out)
}
