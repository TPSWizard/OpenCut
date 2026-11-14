use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};

use crate::codecs::{VideoCodec, is_hardware_encoder, pick_encoder, resolve_codec};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EncodeQuality {
    Low,
    Medium,
    High,
    VeryHigh,
}

impl EncodeQuality {
    /// The constant-quality value for a software encoder.
    fn crf_for(self, encoder: &str, format: EncodeFormat) -> u32 {
        let index = match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
            Self::VeryHigh => 3,
        };
        let table: [u32; 4] = match encoder {
            "libx265" => [32, 28, 24, 20],
            "libsvtav1" => [45, 38, 32, 26],
            "libaom-av1" => [44, 36, 30, 24],
            "libvpx-vp9" => [40, 32, 26, 20],
            _ => return self.crf(format),
        };
        table[index]
    }

    /// A bitrate for encoders without a constant-quality mode that behaves
    /// the same everywhere (the hardware ones): bits per pixel per frame,
    /// less for the newer codecs.
    pub fn bitrate(self, codec: VideoCodec, width: u32, height: u32, fps: f64) -> u64 {
        let bits_per_pixel = match self {
            Self::Low => 0.05,
            Self::Medium => 0.08,
            Self::High => 0.12,
            Self::VeryHigh => 0.18,
        };
        let efficiency = match codec {
            VideoCodec::H264 => 1.0,
            VideoCodec::Hevc | VideoCodec::Vp9 => 0.65,
            VideoCodec::Av1 => 0.55,
        };
        let pixels_per_second = width as f64 * height as f64 * fps.max(1.0);
        (pixels_per_second * bits_per_pixel * efficiency).round().max(200_000.0) as u64
    }

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

/// Platform upload settings, as YouTube documents them for H.264: High
/// profile, two B-frames, CABAC, a closed GOP of a fixed length, a target
/// bitrate and stereo AAC at a given rate. The moov atom goes first in any
/// case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformEncode {
    /// Target video bitrate in bits per second.
    pub video_bitrate: u64,
    /// AAC bitrate in bits per second.
    pub audio_bitrate: u64,
    /// Frames between key frames.
    pub gop_frames: u32,
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
    /// Encode for a platform upload instead of at `quality`. MP4 only; WebM
    /// ignores it. Platform uploads are always H.264 from libx264.
    pub platform: Option<PlatformEncode>,
    /// Video codec; `None`, or one the container does not take, is the
    /// container's default (MP4: H.264, WebM: VP9).
    pub codec: Option<VideoCodec>,
    /// Target video bitrate in bits per second, instead of `quality`.
    pub video_bitrate: Option<u64>,
    /// Use a hardware encoder when one works, else software.
    pub hardware: bool,
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
        let encoder = video_encoder(ffmpeg, settings)?;
        let args = build_args_with(settings, &encoder);
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

/// The codec the settings encode with.
pub fn settings_codec(settings: &EncodeSettings) -> VideoCodec {
    if settings.platform.is_some() && settings.format == EncodeFormat::Mp4 {
        return VideoCodec::H264;
    }
    resolve_codec(settings.format, settings.codec)
}

/// The FFmpeg encoder the settings use on this build.
pub fn video_encoder(ffmpeg: &Ffmpeg, settings: &EncodeSettings) -> Result<String, MediaError> {
    let platform = settings.platform.is_some() && settings.format == EncodeFormat::Mp4;
    pick_encoder(ffmpeg, settings_codec(settings), settings.hardware && !platform)
}

/// The argument list with the default software encoder for the codec.
#[cfg(test)]
pub(crate) fn build_args(settings: &EncodeSettings) -> Vec<String> {
    let encoder = match settings_codec(settings) {
        VideoCodec::H264 => "libx264",
        VideoCodec::Hevc => "libx265",
        VideoCodec::Av1 => "libsvtav1",
        VideoCodec::Vp9 => "libvpx-vp9",
    };
    build_args_with(settings, encoder)
}

/// The video encoder's arguments: the rate (platform settings, a target
/// bitrate, or constant quality) and the pixel format.
fn video_args(settings: &EncodeSettings, encoder: &str) -> Vec<String> {
    let codec = settings_codec(settings);
    let mut args: Vec<String> = vec!["-c:v".into(), encoder.into()];
    let platform = settings.platform.filter(|_| settings.format == EncodeFormat::Mp4 && encoder == "libx264");
    let hardware = is_hardware_encoder(encoder);
    let push = |args: &mut Vec<String>, items: &[&str]| args.extend(items.iter().map(|item| (*item).to_owned()));

    // Speed settings that do not change the rate.
    match encoder {
        "libx264" | "libx265" => push(&mut args, &["-preset", "medium"]),
        "libsvtav1" => push(&mut args, &["-preset", "8"]),
        "libaom-av1" => push(&mut args, &["-cpu-used", "6", "-row-mt", "1"]),
        "libvpx-vp9" => push(&mut args, &["-row-mt", "1", "-deadline", "good", "-cpu-used", "2"]),
        _ => {}
    }
    if encoder == "libx265" {
        push(&mut args, &["-x265-params", "log-level=error"]);
    }

    if let Some(platform) = platform {
        let gop = platform.gop_frames.max(1).to_string();
        let rate = platform.video_bitrate.to_string();
        let max_rate = (platform.video_bitrate * 3 / 2).to_string();
        let buffer = (platform.video_bitrate * 2).to_string();
        push(
            &mut args,
            &[
                "-profile:v", "high", "-bf", "2", "-coder", "1", "-b:v", &rate, "-maxrate", &max_rate, "-bufsize",
                &buffer, "-g", &gop, "-keyint_min", &gop, "-sc_threshold", "0", "-flags", "+cgop",
            ],
        );
    } else if let Some(rate) = settings
        .video_bitrate
        .or_else(|| hardware.then(|| settings.quality.bitrate(codec, settings.width, settings.height, settings.fps)))
    {
        let target = rate.to_string();
        push(&mut args, &["-b:v", &target]);
        // SVT-AV1 and libaom take a plain average; the rest also get a
        // ceiling so a busy scene cannot spike far past it.
        if !matches!(encoder, "libsvtav1" | "libaom-av1" | "libvpx-vp9") {
            let max_rate = (rate * 3 / 2).to_string();
            let buffer = (rate * 2).to_string();
            push(&mut args, &["-maxrate", &max_rate, "-bufsize", &buffer]);
        }
    } else {
        let crf = settings.quality.crf_for(encoder, settings.format).to_string();
        if matches!(encoder, "libvpx-vp9" | "libaom-av1") {
            push(&mut args, &["-b:v", "0"]);
        }
        push(&mut args, &["-crf", &crf]);
    }

    push(&mut args, &["-pix_fmt", if hardware { "nv12" } else { "yuv420p" }]);
    if codec == VideoCodec::Hevc && settings.format == EncodeFormat::Mp4 {
        // `hvc1` is the tag Apple's players need to play HEVC in MP4.
        push(&mut args, &["-tag:v", "hvc1"]);
    }
    args
}

/// The complete argument list after `ffmpeg -hide_banner -nostdin -loglevel error`.
pub(crate) fn build_args_with(settings: &EncodeSettings, encoder: &str) -> Vec<String> {
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

    args.extend(video_args(settings, encoder));
    match settings.format {
        EncodeFormat::Mp4 => {
            args.extend(["-movflags", "+faststart"].into_iter().map(String::from));
            if has_audio {
                let audio_rate = settings
                    .platform
                    .map(|platform| platform.audio_bitrate.to_string())
                    .unwrap_or_else(|| "192k".to_owned());
                args.extend(["-c:a".to_owned(), "aac".to_owned(), "-b:a".to_owned(), audio_rate]);
            }
        }
        EncodeFormat::Webm => {
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
            platform: None,
            codec: None,
            video_bitrate: None,
            hardware: false,
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
            platform: None,
            codec: None,
            video_bitrate: None,
            hardware: false,
            duration_seconds: 4.0,
            audio: vec![clip],
        };
        let args = build_args(&settings);
        let graph = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert!(graph.contains(",asetpts=PTS-STARTPTS,volume='pow(10,(if(lt(t,0.500000)"));
        assert!(graph.contains(":eval=frame,aresample=48000,"));
    }

    #[test]
    fn platform_settings_use_high_profile_and_a_closed_gop() {
        let settings = EncodeSettings {
            output: PathBuf::from("out.mp4"),
            width: 1080,
            height: 1920,
            fps: 30.0,
            format: EncodeFormat::Mp4,
            quality: EncodeQuality::Low,
            platform: Some(PlatformEncode { video_bitrate: 8_000_000, audio_bitrate: 384_000, gop_frames: 15 }),
            codec: None,
            video_bitrate: None,
            hardware: false,
            duration_seconds: 2.0,
            audio: vec![AudioMixClip {
                path: PathBuf::from("a.wav"),
                source_start_seconds: 0.0,
                source_end_seconds: 2.0,
                timeline_start_seconds: 0.0,
                gain: 1.0,
                gain_keys: Vec::new(),
            }],
        };
        let joined = build_args(&settings).join(" ");
        assert!(joined.contains("-profile:v high -bf 2 -coder 1 -b:v 8000000 -maxrate 12000000 -bufsize 16000000 -g 15 -keyint_min 15 -sc_threshold 0 -flags +cgop"), "{joined}");
        assert!(joined.contains("-movflags +faststart"), "{joined}");
        assert!(joined.contains("-c:a aac -b:a 384000"), "{joined}");
        assert!(!joined.contains("-crf"), "{joined}");
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
            platform: None,
            codec: None,
            video_bitrate: None,
            hardware: false,
            duration_seconds: 2.0,
            audio: vec![],
        };
        let joined = build_args(&settings).join(" ");
        assert!(joined.contains("-map 0:v -an -c:v libvpx-vp9"));
        assert!(joined.contains("-crf 32"));
        assert!(!joined.contains("filter_complex"));
    }

    fn silent(format: EncodeFormat, codec: Option<VideoCodec>) -> EncodeSettings {
        EncodeSettings {
            output: PathBuf::from(format!("out.{}", format.extension())),
            width: 1920,
            height: 1080,
            fps: 30.0,
            format,
            quality: EncodeQuality::High,
            platform: None,
            codec,
            video_bitrate: None,
            hardware: false,
            duration_seconds: 2.0,
            audio: vec![],
        }
    }

    #[test]
    fn each_codec_gets_its_encoder_and_quality_scale() {
        let hevc = build_args(&silent(EncodeFormat::Mp4, Some(VideoCodec::Hevc))).join(" ");
        assert!(hevc.contains("-c:v libx265 -preset medium -x265-params log-level=error -crf 24 -pix_fmt yuv420p -tag:v hvc1"), "{hevc}");
        assert!(hevc.contains("-movflags +faststart"), "{hevc}");

        let av1 = build_args(&silent(EncodeFormat::Mp4, Some(VideoCodec::Av1))).join(" ");
        assert!(av1.contains("-c:v libsvtav1 -preset 8 -crf 32 -pix_fmt yuv420p"), "{av1}");
        let aom = build_args_with(&silent(EncodeFormat::Webm, Some(VideoCodec::Av1)), "libaom-av1").join(" ");
        assert!(aom.contains("-c:v libaom-av1 -cpu-used 6 -row-mt 1 -b:v 0 -crf 30"), "{aom}");
        assert!(!aom.contains("movflags"), "{aom}");

        // A codec the container does not take falls back to its default.
        let webm = build_args(&silent(EncodeFormat::Webm, Some(VideoCodec::H264))).join(" ");
        assert!(webm.contains("-c:v libvpx-vp9"), "{webm}");
    }

    #[test]
    fn a_target_bitrate_replaces_constant_quality() {
        let mut settings = silent(EncodeFormat::Mp4, None);
        settings.video_bitrate = Some(6_000_000);
        let x264 = build_args(&settings).join(" ");
        assert!(x264.contains("-b:v 6000000 -maxrate 9000000 -bufsize 12000000"), "{x264}");
        assert!(!x264.contains("-crf"), "{x264}");

        settings.codec = Some(VideoCodec::Av1);
        let av1 = build_args(&settings).join(" ");
        assert!(av1.contains("-b:v 6000000"), "{av1}");
        assert!(!av1.contains("-maxrate") && !av1.contains("-crf"), "{av1}");
    }

    #[test]
    fn hardware_encoders_get_a_bitrate_and_nv12() {
        let settings = silent(EncodeFormat::Mp4, Some(VideoCodec::Hevc));
        let qsv = build_args_with(&settings, "hevc_qsv").join(" ");
        // 1080p30 at High: 62.2 Mpx/s x 0.12 bits x 0.65 for HEVC.
        let rate = EncodeQuality::High.bitrate(VideoCodec::Hevc, 1920, 1080, 30.0);
        assert_eq!(rate, 4_852_224);
        assert!(qsv.contains(&format!("-c:v hevc_qsv -b:v {rate} -maxrate")), "{qsv}");
        assert!(qsv.contains("-pix_fmt nv12 -tag:v hvc1"), "{qsv}");
        assert!(!qsv.contains("-crf") && !qsv.contains("-preset"), "{qsv}");
    }

    #[test]
    fn platform_uploads_stay_h264() {
        let mut settings = silent(EncodeFormat::Mp4, Some(VideoCodec::Av1));
        settings.platform = Some(PlatformEncode { video_bitrate: 8_000_000, audio_bitrate: 384_000, gop_frames: 15 });
        settings.hardware = true;
        assert_eq!(settings_codec(&settings), VideoCodec::H264);
        let joined = build_args(&settings).join(" ");
        assert!(joined.contains("-c:v libx264 -preset medium -profile:v high"), "{joined}");
    }
}
