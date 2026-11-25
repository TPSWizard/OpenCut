//! Rendering a whole project to a file, frame by frame through the
//! renderer and FFmpeg's encoder, with the audio clips mixed. The desktop
//! export dialog and the `opencut-render` command line both come here.

use std::path::PathBuf;
use std::sync::Arc;

use editor::{Channel, Project, Segment, plan_audio};
use media::{
    AudioMixClip, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg, GainEase, GainKey, PlatformEncode, VideoCodec,
};
use time::MediaTime;

use crate::renderer::Renderer;

/// What to write and how.
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub output: PathBuf,
    pub format: EncodeFormat,
    pub quality: EncodeQuality,
    /// Platform upload settings; replace `quality` for MP4.
    pub platform: Option<PlatformEncode>,
    /// Video codec; `None` is the container's default. Ignored with `platform`.
    pub codec: Option<VideoCodec>,
    /// Target video bitrate in bits per second, instead of `quality`.
    pub video_bitrate: Option<u64>,
    /// Use a hardware encoder when one works.
    pub hardware: bool,
    pub include_audio: bool,
    /// Where retimed audio is rendered to (`default_cache_dir()` when none).
    pub cache_dir: Option<PathBuf>,
    /// Bring the whole mix to this loudness with true peaks under its
    /// ceiling (see [`MixLoudness`]); `None` leaves the mix as it is.
    pub loudness: Option<MixLoudness>,
}

/// A whole-mix loudness target for export: the mix is rendered, measured
/// and mastered (`editor::master_mix`: one gain and a true-peak limiter)
/// before it is encoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixLoudness {
    pub target_lufs: f64,
    pub ceiling_dbtp: f64,
}

impl MixLoudness {
    /// The delivery target with this id (`streaming` −14, `podcast` −16,
    /// `broadcast` −23, `loud` −11; all −1 dBTP), if there is one.
    pub fn target(id: &str) -> Option<Self> {
        editor::LOUDNESS_TARGETS
            .iter()
            .find(|target| target.id == id)
            .map(|target| Self { target_lufs: target.lufs, ceiling_dbtp: target.true_peak_dbtp })
    }
}

/// Renders the mix, masters it to `loudness` and returns one clip that
/// plays the mastered mix (a float WAV in `dir`), with what was done.
fn mastered_mix(
    ffmpeg: &Arc<Ffmpeg>,
    audio: &[AudioMixClip],
    duration_seconds: f64,
    loudness: MixLoudness,
    dir: &std::path::Path,
) -> Result<(AudioMixClip, editor::MasterReport), String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let raw = dir.join("mix.wav");
    media::render_audio_mix(ffmpeg, audio, duration_seconds, &raw).map_err(|error| error.to_string())?;
    let mut planes = media::read_audio_planes(ffmpeg, &raw, 2).map_err(|error| error.to_string())?;
    let report = editor::master_mix(&mut planes, media::VOICE_SAMPLE_RATE, loudness.target_lufs, loudness.ceiling_dbtp);
    let mastered = dir.join("mastered.wav");
    media::write_wav_float(&mastered, &planes, media::VOICE_SAMPLE_RATE).map_err(|error| error.to_string())?;
    let length = planes.first().map_or(0.0, |plane| plane.len() as f64 / media::VOICE_SAMPLE_RATE as f64);
    let clip = AudioMixClip {
        path: mastered,
        source_start_seconds: 0.0,
        source_end_seconds: length.max(duration_seconds),
        timeline_start_seconds: 0.0,
        gain: 1.0,
        gain_keys: Vec::new(),
        fade_in_seconds: 0.0,
        fade_out_seconds: 0.0,
        fade_in_equal_power: false,
        fade_out_equal_power: false,
        pan: 0.0,
    };
    Ok((clip, report))
}

/// Renders every frame of the project at its canvas size and frame rate
/// and encodes them, mixing the audio clips. `cancelled` is asked before
/// each frame; `progress` hears (frames done, frames total) now and then.
/// Returns the written path.
pub fn export_project(
    renderer: &mut Renderer,
    ffmpeg: &Arc<Ffmpeg>,
    project: &Project,
    job: &ExportJob,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    export_project_with_report(renderer, ffmpeg, project, job, cancelled, progress).map(|(path, _)| path)
}

/// [`export_project`], also returning what whole-mix loudness did (when the
/// job asks for it and there is audio).
pub fn export_project_with_report(
    renderer: &mut Renderer,
    ffmpeg: &Arc<Ffmpeg>,
    project: &Project,
    job: &ExportJob,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(PathBuf, Option<editor::MasterReport>), String> {
    let fps_rate = project.settings.fps;
    let fps = fps_rate.as_f64().unwrap_or(30.0);
    let duration = project.duration();
    if duration <= MediaTime::ZERO {
        return Err("The timeline is empty".to_owned());
    }
    let frames_total = (duration.to_seconds_f64() * fps).ceil().max(1.0) as u64;
    let canvas = project.settings.canvas;
    let width = canvas.width & !1;
    let height = canvas.height & !1;

    let audio = if job.include_audio {
        let cache_dir = job.cache_dir.clone().unwrap_or_else(crate::retime_audio::default_cache_dir);
        plan_audio(project)
            .into_iter()
            .map(|clip| {
                if crate::retime_audio::needs_render(&clip) {
                    crate::retime_audio::rendered_clip(ffmpeg, &clip, &cache_dir).unwrap_or_else(|error| {
                        eprintln!("retimed audio for {}: {error}", clip.element_id);
                        clip
                    })
                } else {
                    clip
                }
            })
            .map(|clip| AudioMixClip {
                path: clip.path.clone(),
                source_start_seconds: clip.source_start.to_seconds_f64(),
                source_end_seconds: clip.source_end.to_seconds_f64(),
                timeline_start_seconds: clip.timeline_start.to_seconds_f64(),
                gain: clip.gain(),
                gain_keys: gain_keys(&clip.volume),
                fade_in_seconds: clip.fade_in_seconds,
                fade_out_seconds: clip.fade_out_seconds,
                fade_in_equal_power: clip.fade_in_curve == editor::CrossfadeCurve::EqualPower,
                fade_out_equal_power: clip.fade_out_curve == editor::CrossfadeCurve::EqualPower,
                pan: clip.pan,
            })
            .collect()
    } else {
        Vec::new()
    };
    // Whole-mix loudness: the mix rendered, mastered and played as one clip.
    let mix_dir = std::env::temp_dir().join(format!(
        "opencut-mix-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos())
    ));
    let (audio, master_report) = match job.loudness {
        Some(loudness) if !audio.is_empty() => {
            let (clip, report) = mastered_mix(ffmpeg, &audio, duration.to_seconds_f64(), loudness, &mix_dir)?;
            (vec![clip], Some(report))
        }
        _ => (audio, None),
    };
    let encode = EncodeSettings {
        output: job.output.clone(),
        width,
        height,
        fps,
        format: job.format,
        quality: job.quality,
        platform: job.platform,
        codec: job.codec,
        video_bitrate: job.video_bitrate,
        hardware: job.hardware,
        duration_seconds: duration.to_seconds_f64(),
        audio,
        disclosure: project.face_swap_disclosure().map(str::to_owned),
    };
    let result = encode_frames(renderer, project, job, ffmpeg, &encode, frames_total, cancelled, progress);
    if master_report.is_some() {
        std::fs::remove_dir_all(&mix_dir).ok();
    }
    result.map(|path| (path, master_report))
}

#[allow(clippy::too_many_arguments)]
fn encode_frames(
    renderer: &mut Renderer,
    project: &Project,
    job: &ExportJob,
    ffmpeg: &Arc<Ffmpeg>,
    encode: &EncodeSettings,
    frames_total: u64,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let fps_rate = project.settings.fps;
    let canvas = project.settings.canvas;
    let (width, height) = (canvas.width & !1, canvas.height & !1);
    let mut encoder = Encoder::start(ffmpeg, encode).map_err(|error| error.to_string())?;

    for frame_index in 0..frames_total {
        if cancelled() {
            return Err("Export cancelled".to_owned());
        }
        let time = MediaTime::from_frame(frame_index as i64, fps_rate).unwrap_or(MediaTime::ZERO);
        let frame = renderer
            .render(project, time, width, height)
            .map_err(|error| error.to_string())?;
        encoder.write_frame(&frame.bgra).map_err(|error| error.to_string())?;
        if frame_index % 5 == 0 || frame_index + 1 == frames_total {
            progress(frame_index + 1, frames_total);
        }
    }
    let output = encoder.finish().map_err(|error| error.to_string())?;
    // The project's cover picture, when it has one to embed (MP4 only).
    if job.format == EncodeFormat::Mp4 {
        if let Err(error) = crate::cover::embed_cover(renderer, ffmpeg, project, &output) {
            eprintln!("cover not embedded: {error}");
        }
    }
    Ok(output)
}

/// Volume keys for the encoder. A Bézier segment is sampled into short
/// linear pieces (ffmpeg's expression cannot solve the curve); the other
/// segments map one to one.
pub fn gain_keys(volume: &Channel) -> Vec<GainKey> {
    const BEZIER_PIECES: i64 = 8;
    let mut keys = Vec::new();
    for (index, key) in volume.keys.iter().enumerate() {
        let next = volume.keys.get(index + 1);
        match (key.segment_to_next, next) {
            (Segment::Bezier, Some(next)) => {
                let span = (next.time - key.time).as_ticks();
                for piece in 0..BEZIER_PIECES {
                    let time = key.time + MediaTime::from_ticks(span * piece / BEZIER_PIECES);
                    keys.push(GainKey {
                        seconds: time.to_seconds_f64(),
                        db: volume.value_at(time).unwrap_or(key.value),
                        ease: GainEase::Linear,
                    });
                }
            }
            (segment, _) => keys.push(GainKey {
                seconds: key.time.to_seconds_f64(),
                db: key.value,
                ease: match segment {
                    Segment::Step => GainEase::Hold,
                    _ => GainEase::Linear,
                },
            }),
        }
    }
    keys
}

/// The times, in seconds, that stills should be taken at: every `every`
/// seconds from 0 (never past the end), plus any listed times inside the
/// project, sorted and deduplicated to the frame.
pub fn still_times(duration_seconds: f64, fps: f64, every: Option<f64>, at: &[f64]) -> Vec<f64> {
    let mut times: Vec<f64> = Vec::new();
    if let Some(step) = every.filter(|step| *step > 0.0) {
        let mut t = 0.0;
        while t < duration_seconds {
            times.push(t);
            t += step;
        }
    }
    times.extend(at.iter().copied().filter(|t| *t >= 0.0 && *t < duration_seconds));
    let fps = fps.max(1.0);
    let mut frames: Vec<i64> = times.into_iter().map(|t| (t * fps).round() as i64).collect();
    frames.sort_unstable();
    frames.dedup();
    frames.into_iter().map(|frame| frame as f64 / fps).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_times_merge_a_cadence_with_listed_times() {
        assert_eq!(still_times(2.5, 10.0, Some(1.0), &[0.55, 2.0, 9.0]), vec![0.0, 0.6, 1.0, 2.0]);
        assert_eq!(still_times(1.0, 30.0, None, &[]), Vec::<f64>::new());
        assert_eq!(still_times(1.0, 30.0, None, &[0.5, 0.5]), vec![0.5]);
    }

    #[test]
    fn bezier_volume_is_sampled_and_steps_hold() {
        let mut channel = Channel::default();
        channel.upsert(MediaTime::ZERO, 0.0, Some(Segment::Bezier));
        channel.upsert(MediaTime::from_seconds_f64(2.0).unwrap(), -12.0, Some(Segment::Step));
        channel.upsert(MediaTime::from_seconds_f64(3.0).unwrap(), -6.0, None);
        let keys = gain_keys(&channel);
        assert_eq!(keys.len(), 8 + 2, "eight pieces for the curve, then two plain keys");
        assert_eq!(keys[8].ease, GainEase::Hold);
        assert_eq!(keys[9].db, -6.0);
    }
}
