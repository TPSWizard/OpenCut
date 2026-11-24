//! Finding faces on the desktop, for face-only retouch: the clip's played
//! part (or its picture) goes through `scripts/find_faces.py` in the Python
//! of the user's FaceFusion install, with FaceFusion's YOLO face model
//! (`yoloface_8n.onnx`, GPL-3.0, downloaded by FaceFusion and never shipped
//! with this editor). The result becomes the clip's face track
//! (`facetrack`). The web editor finds faces with MediaPipe's BlazeFace.

use std::path::PathBuf;
use std::process::Command;

use facetrack::{FACE_SAMPLE_RATE, FaceBox, FaceTrack};
use media::FaceFusion;

const SCRIPT: &str = include_str!("../scripts/find_faces.py");

pub(crate) struct FaceJob {
    pub facefusion: FaceFusion,
    pub media_id: String,
    pub path: PathBuf,
    /// A picture: one sample. A video: source seconds from `from` for `length`.
    pub still: bool,
    pub from: f64,
    pub length: f64,
}

/// FaceFusion's YOLO face model, once FaceFusion has downloaded it.
pub(crate) fn face_model(facefusion: &FaceFusion) -> Option<PathBuf> {
    Some(facefusion.dir.join(".assets").join("models").join("yoloface_8n.onnx")).filter(|path| path.is_file())
}

pub(crate) fn find_faces(job: &FaceJob) -> Result<FaceTrack, String> {
    let model = face_model(&job.facefusion)
        .ok_or("FaceFusion's face model (yoloface_8n) is not there yet; run a face swap once so FaceFusion downloads it.")?;
    let script = std::env::temp_dir().join("opencut-find-faces.py");
    std::fs::write(&script, SCRIPT).map_err(|error| error.to_string())?;
    let mut command = Command::new(&job.facefusion.python);
    command.arg(&script).arg("--model").arg(&model).arg("--media").arg(&job.path);
    if job.still {
        command.arg("--still");
    } else {
        command
            .arg("--start")
            .arg(format!("{:.3}", job.from))
            .arg("--length")
            .arg(format!("{:.3}", job.length))
            .arg("--rate")
            .arg(FACE_SAMPLE_RATE.to_string());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().map_err(|error| format!("FaceFusion's Python could not run: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let found: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| format!("Finding faces failed: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or("")))?;
    if let Some(error) = found["error"].as_str() {
        return Err(error.to_owned());
    }
    let frames: Vec<Vec<FaceBox>> = found["frames"]
        .as_array()
        .ok_or("Finding faces returned nothing")?
        .iter()
        .map(|faces| {
            faces
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|face| {
                    let v: Vec<f64> = face.as_array()?.iter().filter_map(|n| n.as_f64()).collect();
                    (v.len() == 4).then(|| FaceBox { cx: v[0], cy: v[1], w: v[2], h: v[3] })
                })
                .collect()
        })
        .collect();
    let start = found["start"].as_f64().unwrap_or(job.from);
    let rate = found["rate"].as_f64().unwrap_or(FACE_SAMPLE_RATE);
    Ok(FaceTrack::new(&job.media_id, start, rate, &frames))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The effects library's sample portrait (the project's own artwork):
    /// one face, upper middle. Needs a FaceFusion install with its model.
    #[test]
    fn finds_the_face_in_the_sample_portrait() {
        let Some(facefusion) = media::locate_facefusion() else { return };
        if face_model(&facefusion).is_none() {
            return;
        }
        let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/public/effects/preview.jpg"));
        let track =
            find_faces(&FaceJob { facefusion, media_id: "portrait".into(), path, still: true, from: 0.0, length: 0.0 }).unwrap();
        let faces = track.faces_at(0.0);
        assert_eq!(faces.len(), 1, "{track:?}");
        let face = faces[0];
        assert!((face.cx - 0.49).abs() < 0.05 && (face.cy - 0.33).abs() < 0.06, "{face:?}");
        assert!(face.w > 0.15 && face.w < 0.4, "{face:?}");
    }
}
