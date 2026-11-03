use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Stdio};

use crate::error::MediaError;
use crate::ffmpeg::{Ffmpeg, last_stderr_line};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
        }
    }
}

/// Decodes a file's audio forward from a start time as interleaved 32-bit
/// float samples. One `ffmpeg` process per reader.
pub struct AudioReader {
    child: Child,
    stdout: ChildStdout,
    settings: AudioSettings,
    start_seconds: f64,
    frames_read: u64,
    finished: bool,
}

impl AudioReader {
    pub fn open(
        ffmpeg: &Ffmpeg,
        path: &Path,
        start_seconds: f64,
        settings: AudioSettings,
    ) -> Result<Self, MediaError> {
        let start_seconds = start_seconds.max(0.0);
        let mut command = ffmpeg.command();
        if start_seconds > 0.0 {
            command.arg("-ss").arg(format!("{start_seconds:.6}"));
        }
        command
            .arg("-i")
            .arg(path)
            .args(["-vn", "-sn", "-dn"])
            .arg("-ac")
            .arg(settings.channels.to_string())
            .arg("-ar")
            .arg(settings.sample_rate.to_string())
            .args(["-f", "f32le", "-"])
            .stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        Ok(Self {
            child,
            stdout,
            settings,
            start_seconds,
            frames_read: 0,
            finished: false,
        })
    }

    pub fn settings(&self) -> AudioSettings {
        self.settings
    }

    /// Source time of the next sample frame.
    pub fn next_time_seconds(&self) -> f64 {
        self.start_seconds + self.frames_read as f64 / self.settings.sample_rate as f64
    }

    /// Fills `out` (interleaved) and returns how many samples were written;
    /// fewer than requested only at the end of the stream.
    pub fn read(&mut self, out: &mut [f32]) -> Result<usize, MediaError> {
        if self.finished {
            return Ok(0);
        }
        let bytes: &mut [u8] = bytemuck_cast(out);
        let mut filled = 0;
        while filled < bytes.len() {
            match self.stdout.read(&mut bytes[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let samples = filled / 4;
        if filled < bytes.len() {
            self.finished = true;
            let status = self.child.wait()?;
            if !status.success() && self.frames_read == 0 && samples == 0 {
                let mut stderr = Vec::new();
                if let Some(mut pipe) = self.child.stderr.take() {
                    pipe.read_to_end(&mut stderr).ok();
                }
                return Err(MediaError::ProgramFailed {
                    program: "ffmpeg".to_owned(),
                    message: last_stderr_line(&stderr),
                });
            }
        }
        self.frames_read += (samples / self.settings.channels as usize) as u64;
        Ok(samples)
    }
}

impl Drop for AudioReader {
    fn drop(&mut self) {
        if !self.finished {
            self.child.kill().ok();
            self.child.wait().ok();
        }
    }
}

fn bytemuck_cast(samples: &mut [f32]) -> &mut [u8] {
    // Little-endian hosts match `f32le`; the crate does not target others.
    debug_assert!(cfg!(target_endian = "little"));
    bytemuck::cast_slice_mut(samples)
}
