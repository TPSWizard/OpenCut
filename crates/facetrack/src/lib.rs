//! Face tracks: where the faces are in a clip over time, found once and
//! kept on the clip (the `faces.track` param), so effects can work on
//! faces only. Each editor finds the faces its own way (the web with
//! MediaPipe's BlazeFace, the desktop with FaceFusion's YOLO face model
//! when it is installed); this crate is what they share: the stored form,
//! the faces at a source time ([`FaceTrack::faces_at`]), the soft oval a
//! face covers in the frame ([`face_oval`]) and the retouch shader's face
//! uniforms ([`retouch_face_uniforms`]). Checked against
//! `fixtures/face-track/cases.json`.

use serde::{Deserialize, Serialize};

/// The param a clip keeps its face track in.
pub const FACE_TRACK_PARAM: &str = "faces.track";
/// Faces looked for each second of a video.
pub const FACE_SAMPLE_RATE: f64 = 5.0;
/// At most this many faces are retouched at once.
pub const MAX_FACES: usize = 3;

/// A face's box as shares of the source frame: centre and size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceBox {
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
}

/// A clip's faces, sampled `rate` times a second from source second
/// `start` (one frame for a picture), measured from media `media`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceTrack {
    pub media: String,
    pub start: f64,
    pub rate: f64,
    pub frames: Vec<Vec<[f64; 4]>>,
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

impl FaceTrack {
    /// A track from boxes per sample, each face rounded to 1/1000 and the
    /// faces of a sample ordered left to right.
    pub fn new(media: &str, start: f64, rate: f64, frames: &[Vec<FaceBox>]) -> Self {
        let frames = frames
            .iter()
            .map(|faces| {
                let mut faces: Vec<[f64; 4]> = faces.iter().map(|f| [round3(f.cx), round3(f.cy), round3(f.w), round3(f.h)]).collect();
                faces.sort_by(|a, b| a[0].total_cmp(&b[0]));
                faces
            })
            .collect();
        Self { media: media.to_owned(), start: round3(start), rate, frames }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("face track serialises")
    }

    /// The stored track, or `None` when it is not valid.
    pub fn parse(text: &str) -> Option<Self> {
        let track: Self = serde_json::from_str(text).ok()?;
        (!track.frames.is_empty() && track.rate.is_finite() && track.rate > 0.0 && track.start.is_finite()).then_some(track)
    }

    /// The faces at a source time: between two samples with as many faces,
    /// each face moves in a straight line from one to the next (paired left
    /// to right); otherwise the nearer sample's faces. Ends hold.
    pub fn faces_at(&self, source_seconds: f64) -> Vec<FaceBox> {
        let last = self.frames.len() - 1;
        let position = ((source_seconds - self.start) * self.rate).clamp(0.0, last as f64);
        let (low, high) = (position.floor() as usize, (position.ceil() as usize).min(last));
        let t = position - low as f64;
        let to_box = |f: &[f64; 4]| FaceBox { cx: f[0], cy: f[1], w: f[2], h: f[3] };
        if low != high && self.frames[low].len() == self.frames[high].len() {
            self.frames[low]
                .iter()
                .zip(&self.frames[high])
                .map(|(a, b)| {
                    let lerp = |i: usize| a[i] + (b[i] - a[i]) * t;
                    FaceBox { cx: lerp(0), cy: lerp(1), w: lerp(2), h: lerp(3) }
                })
                .collect()
        } else {
            let nearest = if t < 0.5 { low } else { high };
            self.frames[nearest].iter().map(to_box).collect()
        }
    }
}

/// Where a clip's picture sits in the frame, in frame pixels (the
/// compositor's quad): centre, size, rotation and flips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// The soft oval a face covers in the frame: `[cx, cy, rx, ry]` in frame
/// pixels. The oval reaches a little above the box (the forehead) and is
/// a little larger than it (fading over its outer quarter), so cheeks, brow
/// and chin are in but a hand under the chin mostly is not.
pub fn face_oval(face: &FaceBox, placement: &Placement) -> [f64; 4] {
    let sx = if placement.flip_x { -1.0 } else { 1.0 };
    let sy = if placement.flip_y { -1.0 } else { 1.0 };
    let local_x = (face.cx - 0.5) * sx * placement.width;
    let local_y = (face.cy - 0.08 * face.h - 0.5) * sy * placement.height;
    let (sin, cos) = placement.rotation_degrees.to_radians().sin_cos();
    [
        placement.center_x + local_x * cos - local_y * sin,
        placement.center_y + local_x * sin + local_y * cos,
        0.6 * face.w * placement.width,
        0.62 * face.h * placement.height,
    ]
}

/// The retouch shader's twelve face numbers: the largest [`MAX_FACES`]
/// ovals, biggest first, then zeros.
pub fn retouch_face_uniforms(ovals: &[[f64; 4]]) -> [f64; 12] {
    let mut sorted: Vec<[f64; 4]> = ovals.iter().copied().filter(|oval| oval[2] > 0.0 && oval[3] > 0.0).collect();
    sorted.sort_by(|a, b| (b[2] * b[3]).total_cmp(&(a[2] * a[3])));
    let mut out = [0.0; 12];
    for (index, oval) in sorted.iter().take(MAX_FACES).enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(oval);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/face-track/cases.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn boxes(value: &Value) -> Vec<[f64; 4]> {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn faces_at_times_match_the_reference() {
        let cases = cases();
        let track = FaceTrack::parse(cases["track"].as_str().unwrap()).expect("valid");
        for probe in cases["lookups"].as_array().unwrap() {
            let got: Vec<[f64; 4]> = track.faces_at(probe["time"].as_f64().unwrap()).iter().map(|f| [f.cx, f.cy, f.w, f.h]).collect();
            let want = boxes(&probe["faces"]);
            assert_eq!(got.len(), want.len(), "{probe}");
            for (g, w) in got.iter().zip(&want) {
                for i in 0..4 {
                    assert!((g[i] - w[i]).abs() < 1e-9, "{probe}: {got:?}");
                }
            }
        }
        for bad in cases["invalid"].as_array().unwrap() {
            assert_eq!(FaceTrack::parse(bad.as_str().unwrap()), None, "{bad}");
        }
    }

    #[test]
    fn ovals_and_uniforms_match_the_reference() {
        for case in cases()["ovals"].as_array().unwrap() {
            let p = &case["placement"];
            let placement = Placement {
                center_x: p["centerX"].as_f64().unwrap(),
                center_y: p["centerY"].as_f64().unwrap(),
                width: p["width"].as_f64().unwrap(),
                height: p["height"].as_f64().unwrap(),
                rotation_degrees: p["rotation"].as_f64().unwrap(),
                flip_x: p["flipX"].as_bool().unwrap(),
                flip_y: p["flipY"].as_bool().unwrap(),
            };
            let faces: Vec<FaceBox> = boxes(&case["faces"]).iter().map(|f| FaceBox { cx: f[0], cy: f[1], w: f[2], h: f[3] }).collect();
            let ovals: Vec<[f64; 4]> = faces.iter().map(|face| face_oval(face, &placement)).collect();
            for (got, want) in ovals.iter().zip(boxes(&case["ovals"])) {
                for i in 0..4 {
                    assert!((got[i] - want[i]).abs() < 1e-9, "{}: {ovals:?}", case["name"]);
                }
            }
            let uniforms = retouch_face_uniforms(&ovals);
            let want: Vec<f64> = serde_json::from_value(case["uniforms"].clone()).unwrap();
            for (g, w) in uniforms.iter().zip(&want) {
                assert!((g - w).abs() < 1e-9, "{}: {uniforms:?}", case["name"]);
            }
        }
    }

    #[test]
    fn new_tracks_round_and_order_faces() {
        let track = FaceTrack::new(
            "m",
            1.23456,
            5.0,
            &[vec![FaceBox { cx: 0.7, cy: 0.5, w: 0.1, h: 0.2 }, FaceBox { cx: 0.21234, cy: 0.4, w: 0.1, h: 0.1 }]],
        );
        assert_eq!(track.start, 1.235);
        assert_eq!(track.frames[0][0], [0.212, 0.4, 0.1, 0.1]);
        assert_eq!(FaceTrack::parse(&track.encode()), Some(track));
    }
}
