use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};

use crate::error::MediaError;
use crate::ffmpeg::{Ffmpeg, last_stderr_line};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeFormat {
    Mp4,
    Webm,
}

impl EncodeFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }

    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("webm") => Self::Webm,
            _ => Self::Mp4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeQuality {
    Low,
    Medium,
    High,
    VeryHigh,
}

impl EncodeQuality {
    fn crf(self, format: EncodeFormat) -> u32 {
        match (format, self) {
            (EncodeFormat::Mp4, Self::Low) => 30,
            (EncodeFormat::Mp4, Self::Medium) => 23,
            (EncodeFormat::Mp4, Self::High) => 19,
            (EncodeFormat::Mp4, Self::VeryHigh) => 15,
            (EncodeFormat::Webm, Self::Low) => 40,
            (EncodeFormat::Webm, Self::Medium) => 32,
            (EncodeFormat::Webm, Self::High) => 26,
            (EncodeFormat::Webm, Self::VeryHigh) => 20,
        }
    }
}

/// How a volume key's value reaches the next key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GainEase {
    Hold,
    Linear,
    /// A symmetric ease in and out (smoothstep).
    Smooth,
}

/// One key of a keyframed volume.
#[derive(Clone, Debug, PartialEq)]
pub struct GainKey {
    /// Seconds from the start of the clip (its `source_start_seconds`).
    pub seconds: f64,
    /// Volume in decibels; 0 is unchanged.
    pub db: f64,
    pub ease: GainEase,
}

/// A piece of a source file to mix into the output's audio track.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioMixClip {
    pub path: PathBuf,
    pub source_start_seconds: f64,
    pub source_end_seconds: f64,
    pub timeline_start_seconds: f64,
    /// Linear gain; 1.0 is unchanged. Ignored when `gain_keys` is not empty.
    pub gain: f64,
    /// Keyframed volume; the value is held before the first and after the
    /// last key.
    pub gain_keys: Vec<GainKey>,
}

/// The `volume` filter for a clip: a constant, or an expression in `t`
/// (seconds from the clip's start, which `asetpts` resets) evaluated per
/// frame when the volume is keyframed.
fn volume_filter(clip: &AudioMixClip) -> String {
    if clip.gain_keys.is_empty() {
        return format!("volume={:.6}", clip.gain.max(0.0));
    }
    format!("volume='pow(10,({})/20)':eval=frame", db_expression(&clip.gain_keys))
}

/// A piecewise expression giving the volume in decibels at `t`. Values are
/// parenthesised because ffmpeg's expression parser does not take `--`.
fn db_expression(keys: &[GainKey]) -> String {
    let mut keys: Vec<&GainKey> = keys.iter().collect();
    keys.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let first = keys[0];
    let last = keys[keys.len() - 1];
    let mut expression = format!("({:.6})", last.db);
    for pair in keys.windows(2).rev() {
        let (a, b) = (pair[0], pair[1]);
        let span = b.seconds - a.seconds;
        let segment = if span <= 0.0 {
            format!("({:.6})", a.db)
        } else {
            let progress = format!("((t-{:.6})/{:.6})", a.seconds, span);
            match a.ease {
                GainEase::Hold => format!("({:.6})", a.db),
                GainEase::Linear => format!(
                    "(({a:.6})+(({b:.6})-({a:.6}))*{progress})",
                    a = a.db,
                    b = b.db,
                ),
                GainEase::Smooth => format!(
                    "(({a:.6})+(({b:.6})-({a:.6}))*(3*{p}*{p}-2*{p}*{p}*{p}))",
                    a = a.db,
                    b = b.db,
                    p = progress,
                ),
            }
        };
        expression = format!("if(lt(t,{:.6}),{segment},{expression})", b.seconds);
    }
    format!("if(lt(t,{:.6}),({:.6}),{expression})", first.seconds, first.db)
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncodeSettings {
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub format: EncodeFormat,
    pub quality: EncodeQuality,
    /// Total length, so audio can be padded or cut to the picture.
    pub duration_seconds: f64,
    pub audio: Vec<AudioMixClip>,
}

/// Encodes BGRA frames pushed through [`Encoder::write_frame`] into a video
/// file, mixing the audio clips with an FFmpeg filter graph.
pub struct Encoder {
    child: Child,
    stdin: Option<ChildStdin>,
    frame_len: usize,
    frames_written: u64,
    output: PathBuf,
}

impl Encoder {
    pub fn start(ffmpeg: &Ffmpeg, settings: &EncodeSettings) -> Result<Self, MediaError> {
        if settings.width < 2 || settings.height < 2 || settings.fps <= 0.0 {
            return Err(MediaError::Invalid(
                "encode size and frame rate must be positive".to_owned(),
            ));
        }
        let args = build_args(settings);
        if std::env::var_os("OPENCUT_FFMPEG_TRACE").is_some() {
            eprintln!("ffmpeg {}", args.join(" "));
        }
        let mut command = ffmpeg.command();
        command.args(args).stdin(Stdio::piped()).stdout(Stdio::null());
        let mut child = command.spawn()?;
        let stdin = child.stdin.take();
        Ok(Self {
            child,
            stdin,
            frame_len: settings.width as usize * settings.height as usize * 4,
            frames_written: 0,
            output: settings.output.clone(),
        })
    }

    pub fn frames_written(&self) -> u64 {
        self.frames_written
    }

    pub fn write_frame(&mut self, bgra: &[u8]) -> Result<(), MediaError> {
        if bgra.len() != self.frame_len {
            return Err(MediaError::Invalid(format!(
                "frame has {} bytes, expected {}",
                bgra.len(),
                self.frame_len
            )));
        }
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(MediaError::Invalid("encoder already finished".to_owned()));
        };
        if let Err(error) = stdin.write_all(bgra) {
            // ffmpeg died: report its message rather than a broken pipe.
            let message = self.failure_message();
            return Err(MediaError::ProgramFailed {
                program: "ffmpeg".to_owned(),
                message: message.unwrap_or_else(|| error.to_string()),
            });
        }
        self.frames_written += 1;
        Ok(())
    }

    /// Closes the input and waits for the file to be written.
    pub fn finish(mut self) -> Result<PathBuf, MediaError> {
        drop(self.stdin.take());
        let status = self.child.wait()?;
        if !status.success() {
            let mut stderr = Vec::new();
            if let Some(mut pipe) = self.child.stderr.take() {
                pipe.read_to_end(&mut stderr).ok();
            }
            std::fs::remove_file(&self.output).ok();
            return Err(MediaError::ProgramFailed {
                program: "ffmpeg".to_owned(),
                message: last_stderr_line(&stderr),
            });
        }
        Ok(self.output.clone())
    }

    fn failure_message(&mut self) -> Option<String> {
        let status = self.child.try_wait().ok()??;
        if status.success() {
            return None;
        }
        let mut stderr = Vec::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            pipe.read_to_end(&mut stderr).ok();
        }
        Some(last_stderr_line(&stderr))
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        if self.stdin.is_some() {
            // Abandoned mid-way (an error or a cancel): do not leave a
            // half-written file or a process behind.
            drop(self.stdin.take());
            self.child.kill().ok();
            self.child.wait().ok();
            std::fs::remove_file(&self.output).ok();
        }
    }
}

/// The complete argument list after `ffmpeg -hide_banner -nostdin -loglevel error`.
pub(crate) fn build_args(settings: &EncodeSettings) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "bgra".into(),
        "-s".into(),
        format!("{}x{}", settings.width, settings.height),
        "-r".into(),
        format!("{:.6}", settings.fps),
        "-i".into(),
        "-".into(),
    ];
    for clip in &settings.audio {
        args.push("-i".into());
        args.push(clip.path.to_string_lossy().into_owned());
    }

    let has_audio = !settings.audio.is_empty();
    if has_audio {
        let mut filter = String::new();
        let mut labels = Vec::new();
        for (index, clip) in settings.audio.iter().enumerate() {
            let label = format!("a{index}");
            let delay_ms = (clip.timeline_start_seconds.max(0.0) * 1000.0).round() as u64;
            filter.push_str(&format!(
                "[{input}:a]atrim=start={start:.6}:end={end:.6},asetpts=PTS-STARTPTS,{volume},aresample=48000,aformat=channel_layouts=stereo,adelay={delay}|{delay}[{label}];",
                input = index + 1,
                start = clip.source_start_seconds.max(0.0),
                end = clip.source_end_seconds.max(clip.source_start_seconds),
                volume = volume_filter(clip),
                delay = delay_ms,
            ));
            labels.push(format!("[{label}]"));
        }
        filter.push_str(&labels.concat());
        // Cut or pad the mix to exactly the video's length. `apad` gets the
        // whole duration so the audio stream ends by itself. With an
        // unbounded `apad` followed by `atrim`, ffmpeg 9 kept buffering once
        // the video arrived at render speed rather than instantly, and
        // exports longer than about ten seconds failed with ENOSPC (or grew
        // without bound when `-shortest` was absent). `asetnsamples` keeps
        // the encoder fed with even frames.
        let duration = settings.duration_seconds.max(0.0);
        filter.push_str(&format!(
            "amix=inputs={}:duration=longest:normalize=0,atrim=end={duration:.6},apad=whole_dur={duration:.6},asetnsamples=n=1024:p=0[aout]",
            settings.audio.len(),
        ));
        args.push("-filter_complex".into());
        args.push(filter);
        args.push("-map".into());
        args.push("0:v".into());
        args.push("-map".into());
        args.push("[aout]".into());
    } else {
        args.push("-map".into());
        args.push("0:v".into());
        args.push("-an".into());
    }

    let crf = settings.quality.crf(settings.format);
    match settings.format {
        EncodeFormat::Mp4 => {
            args.extend(
                [
                    "-c:v",
                    "libx264",
                    "-preset",
                    "medium",
                    "-crf",
                    &crf.to_string(),
                    "-pix_fmt",
                    "yuv420p",
                    "-movflags",
                    "+faststart",
                ]
                .into_iter()
                .map(String::from),
            );
            if has_audio {
                args.extend(["-c:a", "aac", "-b:a", "192k"].into_iter().map(String::from));
            }
        }
        EncodeFormat::Webm => {
            args.extend(
                [
                    "-c:v",
                    "libvpx-vp9",
                    "-b:v",
                    "0",
                    "-crf",
                    &crf.to_string(),
                    "-pix_fmt",
                    "yuv420p",
                    "-row-mt",
                    "1",
                    "-deadline",
                    "good",
                    "-cpu-used",
                    "2",
                ]
                .into_iter()
                .map(String::from),
            );
            if has_audio {
                args.extend(["-c:a", "libopus", "-b:a", "128k"].into_iter().map(String::from));
            }
        }
    }
    args.push("-shortest".into());
    args.push(settings.output.to_string_lossy().into_owned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_mix_graph_for_audio_clips() {
        let settings = EncodeSettings {
            output: PathBuf::from("out.mp4"),
            width: 1280,
            height: 720,
            fps: 30.0,
            format: EncodeFormat::Mp4,
            quality: EncodeQuality::High,
            duration_seconds: 10.0,
            audio: vec![
                AudioMixClip {
                    path: PathBuf::from("a.mp4"),
                    source_start_seconds: 1.0,
                    source_end_seconds: 4.0,
                    timeline_start_seconds: 0.0,
                    gain: 1.0,
                    gain_keys: Vec::new(),
                },
                AudioMixClip {
                    path: PathBuf::from("b.mp3"),
                    source_start_seconds: 0.0,
                    source_end_seconds: 2.5,
                    timeline_start_seconds: 3.25,
                    gain: 0.5,
                    gain_keys: Vec::new(),
                },
            ],
        };
        let args = build_args(&settings);
        let joined = args.join(" ");
        assert!(joined.starts_with("-y -f rawvideo -pix_fmt bgra -s 1280x720 -r 30.000000 -i - -i a.mp4 -i b.mp3 -filter_complex"));
        let filter = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert!(filter.contains("[1:a]atrim=start=1.000000:end=4.000000"));
        assert!(filter.contains("[2:a]atrim=start=0.000000:end=2.500000"));
        assert!(filter.contains("volume=0.500000"));
        assert!(filter.contains("adelay=3250|3250[a1]"));
        assert!(filter.ends_with("[a0][a1]amix=inputs=2:duration=longest:normalize=0,atrim=end=10.000000,apad=whole_dur=10.000000,asetnsamples=n=1024:p=0[aout]"));
        assert!(joined.contains("-map 0:v -map [aout] -c:v libx264"));
        assert!(joined.contains("-crf 19"));
        assert!(joined.contains("-c:a aac"));
        assert!(joined.ends_with("-shortest out.mp4"));
    }

    #[test]
    fn keyframed_volume_becomes_a_per_frame_expression() {
        let key = |seconds: f64, db: f64, ease: GainEase| GainKey { seconds, db, ease };
        let clip = AudioMixClip {
            path: PathBuf::from("a.mp3"),
            source_start_seconds: 0.0,
            source_end_seconds: 4.0,
            timeline_start_seconds: 0.0,
            gain: 1.0,
            // Out of order on purpose: keys are sorted by time.
            gain_keys: vec![
                key(2.0, 0.0, GainEase::Smooth),
                key(0.5, -20.0, GainEase::Linear),
                key(1.0, 0.0, GainEase::Hold),
                key(3.0, -6.0, GainEase::Linear),
            ],
        };
        let filter = volume_filter(&clip);
        let u = "((t-2.000000)/1.000000)";
        let expected = format!(
            "volume='pow(10,(if(lt(t,0.500000),(-20.000000),\
             if(lt(t,1.000000),((-20.000000)+((0.000000)-(-20.000000))*((t-0.500000)/0.500000)),\
             if(lt(t,2.000000),(0.000000),\
             if(lt(t,3.000000),((0.000000)+((-6.000000)-(0.000000))*(3*{u}*{u}-2*{u}*{u}*{u})),\
             (-6.000000))))))/20)':eval=frame"
        );
        assert_eq!(filter, expected);
        assert!(!filter.contains("--"));
        let settings = EncodeSettings {
            output: PathBuf::from("out.mp4"),
            width: 320,
            height: 180,
            fps: 30.0,
            format: EncodeFormat::Mp4,
            quality: EncodeQuality::Low,
            duration_seconds: 4.0,
            audio: vec![clip],
        };
        let args = build_args(&settings);
        let graph = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert!(graph.contains(",asetpts=PTS-STARTPTS,volume='pow(10,(if(lt(t,0.500000)"));
        assert!(graph.contains(":eval=frame,aresample=48000,"));
    }

    #[test]
    fn silent_webm_has_no_audio_stream() {
        let settings = EncodeSettings {
            output: PathBuf::from("out.webm"),
            width: 640,
            height: 360,
            fps: 24.0,
            format: EncodeFormat::Webm,
            quality: EncodeQuality::Medium,
            duration_seconds: 2.0,
            audio: vec![],
        };
        let joined = build_args(&settings).join(" ");
        assert!(joined.contains("-map 0:v -an -c:v libvpx-vp9"));
        assert!(joined.contains("-crf 32"));
        assert!(!joined.contains("filter_complex"));
    }
}
