//! Face tracks for the web editor: the shared `facetrack` crate, so the web
//! stores faces, finds them at a time and places their ovals for the
//! retouch shader exactly as the desktop does.

use wasm_bindgen::prelude::wasm_bindgen;

/// A stored face track from samples (JSON `[[[cx, cy, w, h], …], …]`).
#[wasm_bindgen(js_name = faceTrackEncode)]
pub fn face_track_encode(media: &str, start: f64, rate: f64, frames: &str) -> Result<String, String> {
    let frames: Vec<Vec<[f64; 4]>> = serde_json::from_str(frames).map_err(|error| error.to_string())?;
    let frames: Vec<Vec<::facetrack::FaceBox>> = frames
        .iter()
        .map(|faces| faces.iter().map(|f| ::facetrack::FaceBox { cx: f[0], cy: f[1], w: f[2], h: f[3] }).collect())
        .collect();
    Ok(::facetrack::FaceTrack::new(media, start, rate, &frames).encode())
}

/// The media a stored track was found in; empty when it is not valid.
#[wasm_bindgen(js_name = faceTrackMedia)]
pub fn face_track_media(track: &str) -> String {
    ::facetrack::FaceTrack::parse(track).map(|track| track.media).unwrap_or_default()
}

/// How many samples of a stored track have a face (0 when not valid).
#[wasm_bindgen(js_name = faceTrackFoundCount)]
pub fn face_track_found_count(track: &str) -> u32 {
    ::facetrack::FaceTrack::parse(track).map_or(0, |track| track.frames.iter().filter(|faces| !faces.is_empty()).count() as u32)
}

/// The faces at a source time, flat `[cx, cy, w, h, …]`.
#[wasm_bindgen(js_name = faceTrackFacesAt)]
pub fn face_track_faces_at(track: &str, source_seconds: f64) -> Vec<f64> {
    ::facetrack::FaceTrack::parse(track)
        .map(|track| track.faces_at(source_seconds).iter().flat_map(|f| [f.cx, f.cy, f.w, f.h]).collect())
        .unwrap_or_default()
}

/// The retouch shader's twelve face numbers for faces (flat) drawn with a
/// placement (the compositor's quad, frame pixels).
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = retouchFaceUniforms)]
pub fn retouch_face_uniforms(
    faces: &[f64],
    center_x: f64,
    center_y: f64,
    width: f64,
    height: f64,
    rotation_degrees: f64,
    flip_x: bool,
    flip_y: bool,
) -> Vec<f64> {
    let placement = ::facetrack::Placement { center_x, center_y, width, height, rotation_degrees, flip_x, flip_y };
    let ovals: Vec<[f64; 4]> = faces
        .chunks_exact(4)
        .map(|f| ::facetrack::face_oval(&::facetrack::FaceBox { cx: f[0], cy: f[1], w: f[2], h: f[3] }, &placement))
        .collect();
    ::facetrack::retouch_face_uniforms(&ovals).to_vec()
}
