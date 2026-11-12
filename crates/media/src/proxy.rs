//! Proxy media: a small H.264 copy of a video used for preview playback
//! and scrubbing, while export keeps reading the original. Proxies live in
//! a cache folder under a name derived from the source's path, size and
//! modified time, so a changed source gets a new proxy and nothing has to
//! be written into the project.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// The proxy's longer side, in pixels.
pub const PROXY_LONG_EDGE: u32 = 960;

/// Sources larger than this many pixels get a proxy on import.
pub const AUTO_PROXY_PIXELS: u64 = 1920 * 1080;

/// Whether a video of this size gets a proxy without being asked.
pub fn wants_proxy(width: u32, height: u32) -> bool {
    (width as u64) * (height as u64) > AUTO_PROXY_PIXELS
}

/// The proxy's frame size for a source of `source_size`: the longer side
/// is at most [`PROXY_LONG_EDGE`], the aspect ratio is kept, both sides are
/// even, and a source that is already small keeps its size.
pub fn proxy_size((width, height): (u32, u32)) -> (u32, u32) {
    let width = width.max(2);
    let height = height.max(2);
    let long = width.max(height);
    if long <= PROXY_LONG_EDGE {
        return (width & !1, height & !1);
    }
    let scale = PROXY_LONG_EDGE as f64 / long as f64;
    let scaled = |side: u32| ((side as f64 * scale).round() as u32).max(2) & !1;
    (scaled(width), scaled(height))
}

/// Where the proxy of `source` is kept in `cache_dir`, or `None` when the
/// source cannot be read. The name changes when the source's length or
/// modified time does.
pub fn proxy_path(cache_dir: &Path, source: &Path) -> Option<PathBuf> {
    let metadata = std::fs::metadata(source).ok()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|age| age.as_secs())
        .unwrap_or(0);
    let text = format!("{}|{}|{}|{}", source.display(), metadata.len(), modified, PROXY_LONG_EDGE);
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(cache_dir.join(format!("proxy-{hash:016x}.mp4")))
}

/// The proxy of `source` when one has been made.
pub fn existing_proxy(cache_dir: &Path, source: &Path) -> Option<PathBuf> {
    proxy_path(cache_dir, source).filter(|path| path.is_file())
}

/// Makes the proxy of `source` (a video of `source_size`) at `output`: H.264
/// at [`proxy_size`], a keyframe every half second for quick seeks, no audio.
/// Writes to a temporary file first so a half-made proxy is never used.
pub fn make_proxy(ffmpeg: &Ffmpeg, source: &Path, output: &Path, source_size: (u32, u32)) -> Result<(u32, u32), MediaError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (width, height) = proxy_size(source_size);
    let partial = output.with_extension("partial.mp4");
    let mut command = ffmpeg.command();
    command
        .arg("-y")
        .arg("-i")
        .arg(source)
        .args(["-map", "0:v:0", "-an", "-sn", "-dn"])
        .arg("-vf")
        .arg(format!("scale={width}:{height}:flags=bicubic"))
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p",
            "-force_key_frames", "expr:gte(t,n_forced*0.5)", "-movflags", "+faststart",
        ])
        .arg(&partial);
    let result = command.output()?;
    if !result.status.success() {
        std::fs::remove_file(&partial).ok();
        return Err(MediaError::ProgramFailed {
            program: "ffmpeg".to_owned(),
            message: crate::ffmpeg::last_stderr_line(&result.stderr),
        });
    }
    std::fs::rename(&partial, output)?;
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_fit_the_long_edge_and_stay_even() {
        assert_eq!(proxy_size((3840, 2160)), (960, 540));
        assert_eq!(proxy_size((2160, 3840)), (540, 960));
        assert_eq!(proxy_size((1921, 1081)), (960, 540));
        assert_eq!(proxy_size((640, 361)), (640, 360));
        assert!(wants_proxy(3840, 2160));
        assert!(!wants_proxy(1920, 1080));
    }

    #[test]
    fn names_follow_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.mp4");
        std::fs::write(&source, b"one").unwrap();
        let first = proxy_path(dir.path(), &source).unwrap();
        assert!(first.file_name().unwrap().to_string_lossy().starts_with("proxy-"));
        assert_eq!(proxy_path(dir.path(), &source), Some(first.clone()));
        std::fs::write(&source, b"longer").unwrap();
        assert_ne!(proxy_path(dir.path(), &source), Some(first));
        assert!(existing_proxy(dir.path(), &source).is_none());
        assert!(proxy_path(dir.path(), &dir.path().join("missing.mp4")).is_none());
    }
}
