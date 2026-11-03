use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::Arc;

use crate::error::MediaError;
use crate::ffmpeg::{Ffmpeg, last_stderr_line};

/// One decoded picture, BGRA, tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Presentation time within the source, in seconds.
    pub time_seconds: f64,
    pub bgra: Vec<u8>,
}

impl Frame {
    pub fn byte_len(width: u32, height: u32) -> usize {
        width as usize * height as usize * 4
    }
}

/// Decodes a video file forward from a start time at a fixed frame rate and
/// output size. Each [`VideoReader`] is one `ffmpeg` process; dropping it
/// kills the process.
pub struct VideoReader {
    child: Child,
    stdout: ChildStdout,
    width: u32,
    height: u32,
    fps: f64,
    start_seconds: f64,
    frames_read: u64,
    finished: bool,
}

impl VideoReader {
    /// `width` is the output width; the height follows the source's aspect
    /// ratio (both are rounded to even numbers, which encoders need too).
    pub fn open(
        ffmpeg: &Ffmpeg,
        path: &Path,
        start_seconds: f64,
        output_width: u32,
        source_size: (u32, u32),
        fps: f64,
    ) -> Result<Self, MediaError> {
        let (width, height) = output_size(source_size, output_width);
        let start_seconds = start_seconds.max(0.0);
        let mut command = ffmpeg.command();
        if start_seconds > 0.0 {
            // Seeking before the input is fast (keyframe), ffmpeg then decodes
            // forward to the exact time because `-accurate_seek` is the default.
            command.arg("-ss").arg(format!("{start_seconds:.6}"));
        }
        command
            .arg("-i")
            .arg(path)
            .args(["-an", "-sn", "-dn"])
            .arg("-vf")
            .arg(format!("scale={width}:{height}:flags=bilinear,fps={fps:.6}"))
            .args(["-f", "rawvideo", "-pix_fmt", "bgra", "-"])
            .stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        Ok(Self {
            child,
            stdout,
            width,
            height,
            fps,
            start_seconds,
            frames_read: 0,
            finished: false,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The source time of the next frame that [`Self::next_frame`] returns.
    pub fn next_time_seconds(&self) -> f64 {
        self.start_seconds + self.frames_read as f64 / self.fps
    }

    /// Reads the next frame, or `None` at the end of the file.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, MediaError> {
        if self.finished {
            return Ok(None);
        }
        let mut bgra = vec![0u8; Frame::byte_len(self.width, self.height)];
        let mut filled = 0;
        while filled < bgra.len() {
            match self.stdout.read(&mut bgra[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if filled < bgra.len() {
            self.finished = true;
            let status = self.child.wait()?;
            if !status.success() && self.frames_read == 0 {
                let mut stderr = Vec::new();
                if let Some(mut pipe) = self.child.stderr.take() {
                    pipe.read_to_end(&mut stderr).ok();
                }
                return Err(MediaError::ProgramFailed {
                    program: "ffmpeg".to_owned(),
                    message: last_stderr_line(&stderr),
                });
            }
            return Ok(None);
        }
        let time_seconds = self.next_time_seconds();
        self.frames_read += 1;
        Ok(Some(Frame {
            width: self.width,
            height: self.height,
            time_seconds,
            bgra,
        }))
    }

    /// Skips `count` frames without keeping them.
    pub fn skip(&mut self, count: u64) -> Result<(), MediaError> {
        for _ in 0..count {
            if self.next_frame()?.is_none() {
                break;
            }
        }
        Ok(())
    }
}

impl Drop for VideoReader {
    fn drop(&mut self) {
        if !self.finished {
            self.child.kill().ok();
            self.child.wait().ok();
        }
    }
}

/// Random access to a video file on top of a forward decoder: nearby
/// requests read ahead, far ones reopen the decoder at the new time.
pub struct VideoSource {
    ffmpeg: Arc<Ffmpeg>,
    path: PathBuf,
    source_size: (u32, u32),
    output_width: u32,
    fps: f64,
    reader: Option<VideoReader>,
    current: Option<Frame>,
}

/// How far ahead a request may be before the decoder is reopened instead of
/// decoding through the gap.
const READ_AHEAD_SECONDS: f64 = 1.5;

impl VideoSource {
    pub fn new(
        ffmpeg: Arc<Ffmpeg>,
        path: impl Into<PathBuf>,
        source_size: (u32, u32),
        output_width: u32,
        fps: f64,
    ) -> Self {
        Self {
            ffmpeg,
            path: path.into(),
            source_size,
            output_width,
            fps: if fps > 0.0 { fps } else { 30.0 },
            reader: None,
            current: None,
        }
    }

    pub fn output_width(&self) -> u32 {
        self.output_width
    }

    /// The frame shown at `time_seconds` into the source. Returns the last
    /// frame past the end of the file.
    pub fn frame_at(&mut self, time_seconds: f64) -> Result<Option<&Frame>, MediaError> {
        let time_seconds = time_seconds.max(0.0);
        let frame_duration = 1.0 / self.fps;
        // Already showing this frame?
        if let Some(current) = &self.current
            && time_seconds >= current.time_seconds - frame_duration * 0.5
            && time_seconds < current.time_seconds + frame_duration * 0.5
        {
            return Ok(self.current.as_ref());
        }
        let needs_reopen = match &self.reader {
            Some(reader) => {
                let next = reader.next_time_seconds();
                time_seconds < next - frame_duration * 0.5
                    || time_seconds > next + READ_AHEAD_SECONDS
            }
            None => true,
        };
        if needs_reopen {
            // Open slightly early so the wanted frame is the first or second out.
            let start = (time_seconds - frame_duration * 0.5).max(0.0);
            let start = (start * self.fps).floor() / self.fps;
            self.reader = Some(VideoReader::open(
                &self.ffmpeg,
                &self.path,
                start,
                self.output_width,
                self.source_size,
                self.fps,
            )?);
        }
        let reader = self.reader.as_mut().expect("reader was just opened");
        loop {
            let next = reader.next_time_seconds();
            if next + frame_duration * 0.5 > time_seconds {
                // The next frame covers the wanted time.
                match reader.next_frame()? {
                    Some(frame) => {
                        self.current = Some(frame);
                    }
                    None => {
                        // Past the end: keep whatever we last decoded.
                        self.reader = None;
                    }
                }
                return Ok(self.current.as_ref());
            }
            match reader.next_frame()? {
                Some(frame) => self.current = Some(frame),
                None => {
                    self.reader = None;
                    return Ok(self.current.as_ref());
                }
            }
        }
    }

    pub fn current(&self) -> Option<&Frame> {
        self.current.as_ref()
    }

    /// Drops the decoder process; the next request reopens it.
    pub fn release(&mut self) {
        self.reader = None;
    }
}

/// A single frame at `time_seconds`, scaled to `width`, for thumbnails.
pub fn thumbnail(
    ffmpeg: &Ffmpeg,
    path: &Path,
    time_seconds: f64,
    width: u32,
    source_size: (u32, u32),
) -> Result<Frame, MediaError> {
    let mut reader = VideoReader::open(ffmpeg, path, time_seconds, width, source_size, 30.0)?;
    match reader.next_frame()? {
        Some(frame) => Ok(frame),
        None if time_seconds > 0.0 => thumbnail(ffmpeg, path, 0.0, width, source_size),
        None => Err(MediaError::NoStream {
            path: path.to_path_buf(),
            stream: "video",
        }),
    }
}

fn output_size((source_width, source_height): (u32, u32), output_width: u32) -> (u32, u32) {
    let source_width = source_width.max(1);
    let source_height = source_height.max(1);
    let width = output_width.max(2);
    let height =
        ((width as f64 * source_height as f64 / source_width as f64).round() as u32).max(2);
    (width & !1, height & !1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_sizes_keep_aspect_and_stay_even() {
        assert_eq!(output_size((1920, 1080), 640), (640, 360));
        assert_eq!(output_size((1080, 1920), 320), (320, 568));
        assert_eq!(output_size((100, 75), 333), (332, 250));
    }
}
