//! Rendering a whole project to a file, frame by frame through the
//! renderer and FFmpeg's encoder, with the audio clips mixed. The desktop
//! export dialog and the `opencut-render` command line both come here.

use std::path::PathBuf;
use std::sync::Arc;

use editor::{Channel, Project, Segment, plan_audio};
use media::{AudioMixClip, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg, GainEase, GainKey, PlatformEncode};
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
    pub include_audio: bool,
    /// Where retimed audio is rendered to (`default_cache_dir()` when none).
    pub cache_dir: Option<PathBuf>,
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
            })
            .collect()
    } else {
        Vec::new()
    };
    let encode = EncodeSettings {
        output: job.output.clone(),
        width,
        height,
        fps,
        format: job.format,
        quality: job.quality,
        platform: job.platform,
        duration_seconds: duration.to_seconds_f64(),
        audio,
    };
    let mut encoder = Encoder::start(ffmpeg, &encode).map_err(|error| error.to_string())?;

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
    encoder.finish().map_err(|error| error.to_string())
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
