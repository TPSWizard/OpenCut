//! One frame of a video as a full-size PNG (freeze frame).

use std::path::Path;

use crate::{Ffmpeg, MediaError};

/// Writes the frame shown at `seconds` into `source` to `output` as a PNG
/// at the video's display size (rotation applied). A time past the last
/// frame falls back to a moment earlier, so the clip's last frame still
/// freezes.
pub fn frame_to_png(ffmpeg: &Ffmpeg, source: &Path, output: &Path, seconds: f64) -> Result<(), MediaError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut at = seconds.max(0.0);
    for _ in 0..4 {
        std::fs::remove_file(output).ok();
        let mut command = ffmpeg.command();
        command
            .arg("-y")
            .args(["-ss", &format!("{at:.4}")])
            .arg("-i")
            .arg(source)
            .args(["-map", "0:v:0", "-frames:v", "1", "-update", "1", "-f", "image2", "-c:v", "png"])
            .arg(output);
        let result = command.output()?;
        if !result.status.success() {
            return Err(MediaError::ProgramFailed { program: "ffmpeg".to_owned(), message: crate::ffmpeg::last_stderr_line(&result.stderr) });
        }
        if std::fs::metadata(output).map(|meta| meta.len() > 0).unwrap_or(false) {
            return Ok(());
        }
        if at == 0.0 {
            break;
        }
        at = (at - 0.25).max(0.0);
    }
    Err(MediaError::Invalid("FFmpeg made no frame at that time".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezes_a_frame_at_full_size() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.mp4");
        let status = ffmpeg
            .command()
            .args(["-f", "lavfi", "-i", "testsrc=size=320x180:rate=30:duration=2", "-pix_fmt", "yuv420p", "-y"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(status.status.success());
        let output = dir.path().join("out").join("still.png");
        frame_to_png(&ffmpeg, &source, &output, 1.0).unwrap();
        let picture = image::open(&output).unwrap();
        assert_eq!((picture.width(), picture.height()), (320, 180));
        // testsrc's counter differs between frames: a later time gives another picture.
        let later = dir.path().join("out").join("later.png");
        frame_to_png(&ffmpeg, &source, &later, 1.5).unwrap();
        assert_ne!(picture.to_rgba8().into_raw(), image::open(&later).unwrap().to_rgba8().into_raw());
        // Past the end it still freezes the last frame.
        let past = dir.path().join("out").join("past.png");
        frame_to_png(&ffmpeg, &source, &past, 2.2).unwrap();
        assert!(std::fs::metadata(&past).unwrap().len() > 0);
    }
}
