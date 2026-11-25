//! Audio for clips whose speed changes, that play backwards or that have
//! audio effects (equaliser, compressor, limiter; `voice::apply_audio_fx`):
//! the clip's source span is decoded once, resampled along the retime map
//! and run through the effects into a WAV at timeline pace, cached by
//! content key, so the mixer and the exporter can treat it as a plain clip.
//! Pitch is not preserved.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use editor::AudioClipPlan;
use media::{AudioReader, AudioSettings, Ffmpeg};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

/// Whether the clip needs a rendered copy (anything but a plain 1x).
pub fn needs_render(clip: &AudioClipPlan) -> bool {
    clip.retime.as_ref().is_some_and(|retime| !retime.is_identity()) || !clip.fx.is_identity()
}

fn cache_key(clip: &AudioClipPlan) -> String {
    let retime = clip.retime.as_ref().map(|retime| serde_json::to_string(retime).unwrap_or_default()).unwrap_or_default();
    let text = format!(
        "{}|{}|{}|{}|{retime}|{}",
        clip.path.display(),
        clip.source_start.as_ticks(),
        clip.source_end.as_ticks(),
        clip.timeline_duration.as_ticks(),
        if clip.fx.is_identity() { String::new() } else { clip.fx.key() }
    );
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("retime-{hash:016x}")
}

/// A copy of `clip` that reads the rendered WAV at 1x from its start.
pub fn rendered_clip(ffmpeg: &Arc<Ffmpeg>, clip: &AudioClipPlan, cache_dir: &Path) -> Result<AudioClipPlan, String> {
    let path = cache_dir.join(format!("{}.wav", cache_key(clip)));
    if !path.exists() {
        render(ffmpeg, clip, &path)?;
    }
    Ok(AudioClipPlan {
        path,
        source_start: time::MediaTime::ZERO,
        source_end: clip.timeline_duration,
        retime: None,
        fx: voice::AudioFx::default(),
        ..clip.clone()
    })
}

fn render(ffmpeg: &Arc<Ffmpeg>, clip: &AudioClipPlan, output: &Path) -> Result<(), String> {
    let retime = clip.retime.clone().unwrap_or_default();
    let from = clip.source_start.to_seconds_f64();
    let to = clip.source_end.to_seconds_f64();
    let duration = clip.timeline_duration.to_seconds_f64();
    if to <= from || duration <= 0.0 {
        return Err("The clip's span holds no audio".to_owned());
    }
    let settings = AudioSettings { sample_rate: SAMPLE_RATE, channels: CHANNELS };
    let mut reader = AudioReader::open(ffmpeg, &clip.path, from, settings).map_err(|error| error.to_string())?;
    let wanted = ((to - from) * SAMPLE_RATE as f64).round() as usize * CHANNELS as usize;
    let mut source: Vec<f32> = Vec::with_capacity(wanted);
    let mut buffer = vec![0.0f32; SAMPLE_RATE as usize * CHANNELS as usize];
    while source.len() < wanted {
        let want = (wanted - source.len()).min(buffer.len());
        let got = reader.read(&mut buffer[..want]).map_err(|error| error.to_string())?;
        if got == 0 {
            break;
        }
        source.extend_from_slice(&buffer[..got]);
    }
    let frames = source.len() / CHANNELS as usize;
    if frames == 0 {
        return Err("No audio could be read from the clip".to_owned());
    }
    let out_frames = (duration * SAMPLE_RATE as f64).round() as usize;
    let mut planes: Vec<Vec<f32>> = vec![Vec::with_capacity(out_frames); CHANNELS as usize];
    for index in 0..out_frames {
        let seconds = index as f64 / SAMPLE_RATE as f64;
        let position = retime.source_offset(seconds, duration) * SAMPLE_RATE as f64;
        let lower = position.floor().max(0.0) as usize;
        let fraction = (position - lower as f64).clamp(0.0, 1.0) as f32;
        for (channel, plane) in planes.iter_mut().enumerate() {
            let a = source.get(lower * CHANNELS as usize + channel).copied().unwrap_or(0.0);
            let b = source.get((lower + 1) * CHANNELS as usize + channel).copied().unwrap_or(a);
            plane.push(a * (1.0 - fraction) + b * fraction);
        }
    }
    voice::apply_audio_fx(&mut planes, SAMPLE_RATE as f32, &clip.fx);
    let mut out: Vec<i16> = Vec::with_capacity(out_frames * CHANNELS as usize);
    for index in 0..out_frames {
        for plane in &planes {
            out.push((plane[index].clamp(-1.0, 1.0) * 32767.0).round() as i16);
        }
    }
    write_wav(output, &out, SAMPLE_RATE, CHANNELS)
}

fn write_wav(path: &Path, samples: &[i16], sample_rate: u32, channels: u16) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let data_len = (samples.len() * 2) as u32;
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|error| error.to_string())?);
    let mut header = Vec::with_capacity(44);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(36 + data_len).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16u32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    header.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    header.extend_from_slice(&(channels * 2).to_le_bytes());
    header.extend_from_slice(&16u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_len.to_le_bytes());
    file.write_all(&header).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())
}

/// The default cache directory: `<projects root>/../processed`.
pub fn default_cache_dir() -> PathBuf {
    editor::ProjectStore::default_root()
        .parent()
        .map(|root| root.join("processed"))
        .unwrap_or_else(|| PathBuf::from("processed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_change_with_the_retime_and_plain_clips_need_nothing() {
        let mut clip = AudioClipPlan {
            element_id: "e".into(),
            media_id: "m".into(),
            path: PathBuf::from("x.wav"),
            timeline_start: time::MediaTime::ZERO,
            timeline_duration: time::MediaTime::from_seconds_f64(2.0).unwrap(),
            source_start: time::MediaTime::ZERO,
            source_end: time::MediaTime::from_seconds_f64(4.0).unwrap(),
            volume_db: 0.0,
            fade_in_seconds: 0.0,
            fade_out_seconds: 0.0,
            fade_in_curve: Default::default(),
            fade_out_curve: Default::default(),
            track_id: String::new(),
            track_gain: 1.0,
            track_pan: 0.0,
            pan: 0.0,
            fx: Default::default(),
            volume: Default::default(),
            retime: Some(editor::Retime::constant(2.0)),
        };
        assert!(needs_render(&clip));
        let fast = cache_key(&clip);
        clip.retime = Some(editor::Retime { reverse: true, ..editor::Retime::constant(2.0) });
        assert_ne!(cache_key(&clip), fast);
        clip.retime = None;
        assert!(!needs_render(&clip));
        clip.retime = Some(editor::Retime::constant(1.0));
        assert!(!needs_render(&clip));
        // Audio effects alone need a rendered copy, keyed by their settings.
        let plain = cache_key(&clip);
        clip.fx = voice::AudioFx { low_db: 3.0, ..Default::default() };
        assert!(needs_render(&clip));
        assert_ne!(cache_key(&clip), plain);
    }

    /// A loud tone through a clip with a -12 dB limiter comes out of the
    /// cache under the ceiling, and the copy plays at 1x with no effects
    /// left to apply. Needs FFmpeg; skips otherwise.
    #[test]
    fn renders_audio_effects_into_the_cached_copy() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("tone.wav");
        let tone: Vec<i16> = (0..SAMPLE_RATE as usize)
            .flat_map(|i| {
                let value = (0.9 * (2.0 * std::f64::consts::PI * 300.0 * i as f64 / SAMPLE_RATE as f64).sin() * 32767.0) as i16;
                [value, value]
            })
            .collect();
        write_wav(&source, &tone, SAMPLE_RATE, CHANNELS).unwrap();
        let clip = AudioClipPlan {
            element_id: "e".into(),
            media_id: "m".into(),
            path: source,
            timeline_start: time::MediaTime::ZERO,
            timeline_duration: time::MediaTime::from_seconds_f64(1.0).unwrap(),
            source_start: time::MediaTime::ZERO,
            source_end: time::MediaTime::from_seconds_f64(1.0).unwrap(),
            volume_db: 0.0,
            fade_in_seconds: 0.0,
            fade_out_seconds: 0.0,
            fade_in_curve: Default::default(),
            fade_out_curve: Default::default(),
            track_id: String::new(),
            track_gain: 1.0,
            track_pan: 0.0,
            pan: 0.0,
            fx: voice::AudioFx { limiter: Some(-12.0), ..Default::default() },
            volume: Default::default(),
            retime: None,
        };
        let rendered = rendered_clip(&ffmpeg, &clip, dir.path()).unwrap();
        assert!(rendered.fx.is_identity() && rendered.retime.is_none() && !needs_render(&rendered));
        let bytes = std::fs::read(&rendered.path).unwrap();
        let peak = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap();
        let ceiling = (10f64.powf(-12.0 / 20.0) * 32767.0).round() as u16 + 1;
        assert!(peak <= ceiling && peak > ceiling - 400, "peak {peak}, ceiling {ceiling}");
    }
}
