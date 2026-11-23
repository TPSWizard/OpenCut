//! A cover picture embedded in an exported MP4 (row 70): the file is
//! remuxed with the picture as an attached-picture stream, which MP4 stores
//! as the `covr` cover art players and file browsers show. Streams are
//! copied, so this takes a moment whatever the video's length.

use std::path::Path;

use crate::{Ffmpeg, MediaError};

/// Embeds the PNG or JPEG `cover` into the MP4 `video` in place.
pub fn attach_cover(ffmpeg: &Ffmpeg, video: &Path, cover: &Path) -> Result<(), MediaError> {
    let temp = video.with_extension("cover.mp4");
    std::fs::remove_file(&temp).ok();
    let codec = match cover.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("jpg" | "jpeg") => "mjpeg",
        _ => "png",
    };
    let mut command = ffmpeg.command();
    command
        .arg("-y")
        .arg("-i")
        .arg(video)
        .arg("-i")
        .arg(cover)
        .args(["-map", "0", "-map", "1:v:0", "-c", "copy", "-c:v:1", codec, "-disposition:v:1", "attached_pic"])
        .args(["-movflags", "+faststart"])
        .arg(&temp);
    let result = command.output()?;
    if !result.status.success() {
        std::fs::remove_file(&temp).ok();
        return Err(MediaError::ProgramFailed { program: "ffmpeg".to_owned(), message: crate::ffmpeg::last_stderr_line(&result.stderr) });
    }
    std::fs::rename(&temp, video)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_cover_as_an_attached_picture() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("clip.mp4");
        let made = ffmpeg
            .command()
            .args(["-f", "lavfi", "-i", "testsrc=size=320x180:rate=30:duration=1", "-f", "lavfi", "-i", "sine=duration=1"])
            .args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", "-y"])
            .arg(&video)
            .output()
            .unwrap();
        assert!(made.status.success());
        let cover = dir.path().join("cover.png");
        image::RgbaImage::from_pixel(64, 36, image::Rgba([255, 0, 0, 255])).save(&cover).unwrap();
        attach_cover(&ffmpeg, &video, &cover).unwrap();
        assert!(!video.with_extension("cover.mp4").exists());
        // The cover comes back out as the second video stream.
        let out = dir.path().join("back.png");
        let pulled = ffmpeg.command().arg("-i").arg(&video).args(["-map", "0:v:1", "-frames:v", "1", "-y"]).arg(&out).output().unwrap();
        assert!(pulled.status.success(), "{}", String::from_utf8_lossy(&pulled.stderr));
        let picture = image::open(&out).unwrap().to_rgba8();
        assert_eq!((picture.width(), picture.height()), (64, 36));
        assert_eq!(picture.get_pixel(10, 10).0, [255, 0, 0, 255]);
        // The video and audio are still there.
        let probe = ffmpeg.command().args(["-loglevel", "info", "-i"]).arg(&video).output().unwrap();
        let text = String::from_utf8_lossy(&probe.stderr);
        assert!(text.contains("Audio:") && text.contains("attached pic"), "{text}");
    }
}
