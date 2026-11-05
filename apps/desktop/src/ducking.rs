//! Measures the voice clips of a project over a music clip's span, for
//! auto-ducking. Runs on a background thread: each voice clip is decoded
//! by FFmpeg to 16 kHz mono and its energy summed into timeline windows.

use std::sync::Arc;

use editor::{DuckingOptions, Project, plan_audio};
use media::{AudioReader, AudioSettings, Ffmpeg};

/// Sample rate the voice is measured at; speech needs no more.
const ANALYSIS_SAMPLE_RATE: u32 = 16_000;

pub(crate) struct VoiceLevels {
    /// Where the music clip starts on the timeline, in seconds.
    pub clip_start: f64,
    pub clip_duration: f64,
    /// RMS level in dBFS of each window from the clip's start.
    pub levels: Vec<f64>,
    /// Ids of the clips the voice was measured from.
    pub voice_ids: Vec<String>,
}

/// The voice over the clip `music_id`: every other audible clip, or only
/// `voice_id` when given. Trims and placement are followed; a clip's own
/// volume is not, since speech is detected on the source.
pub(crate) fn measure_voice(
    ffmpeg: &Arc<Ffmpeg>,
    project: &Project,
    music_id: &str,
    voice_id: Option<&str>,
    options: &DuckingOptions,
) -> Result<VoiceLevels, String> {
    let music = project
        .tracks
        .iter()
        .flat_map(|track| track.elements.iter())
        .find(|element| element.id == music_id)
        .ok_or_else(|| "The clip to duck is gone".to_owned())?;
    let clip_start = music.start_time.to_seconds_f64();
    let clip_duration = music.duration.to_seconds_f64();
    let clip_end = clip_start + clip_duration;
    let window = ((ANALYSIS_SAMPLE_RATE as f64 * options.window_seconds).round() as usize).max(1);
    let count = ((clip_duration / options.window_seconds).ceil().max(0.0)) as usize;
    let mut energy = vec![0.0f64; count];
    let mut counts = vec![0u64; count];

    let voices: Vec<_> = plan_audio(project)
        .into_iter()
        .filter(|clip| clip.element_id != music_id)
        .filter(|clip| voice_id.is_none_or(|id| clip.element_id == id))
        .filter(|clip| clip.timeline_end().to_seconds_f64() > clip_start && clip.timeline_start.to_seconds_f64() < clip_end)
        .collect();
    if voices.is_empty() {
        return Err(match voice_id {
            Some(_) => "The chosen voice clip has no audio over this clip".to_owned(),
            None => "There is no other audio clip playing over this one".to_owned(),
        });
    }

    let settings = AudioSettings { sample_rate: ANALYSIS_SAMPLE_RATE, channels: 1 };
    let mut buffer = vec![0.0f32; window * 16];
    for clip in &voices {
        let timeline_start = clip.timeline_start.to_seconds_f64();
        let source_start = clip.source_start.to_seconds_f64();
        let source_end = clip.source_end.to_seconds_f64();
        // Start reading where the music clip begins, if the voice began earlier.
        let skip = (clip_start - timeline_start).max(0.0);
        let read_from = source_start + skip;
        let read_to = source_end.min(source_start + (clip_end - timeline_start));
        if read_to <= read_from {
            continue;
        }
        let mut reader = AudioReader::open(ffmpeg, &clip.path, read_from, settings).map_err(|error| error.to_string())?;
        let mut position = timeline_start + skip; // timeline seconds of the next sample
        let mut remaining = ((read_to - read_from) * ANALYSIS_SAMPLE_RATE as f64).round() as usize;
        while remaining > 0 {
            let want = remaining.min(buffer.len());
            let got = reader.read(&mut buffer[..want]).map_err(|error| error.to_string())?;
            if got == 0 {
                break;
            }
            for (offset, sample) in buffer[..got].iter().enumerate() {
                let time = position + offset as f64 / ANALYSIS_SAMPLE_RATE as f64;
                let index = ((time - clip_start) / options.window_seconds).floor();
                if index >= 0.0 && (index as usize) < count {
                    let index = index as usize;
                    energy[index] += (*sample as f64) * (*sample as f64);
                    counts[index] += 1;
                }
            }
            position += got as f64 / ANALYSIS_SAMPLE_RATE as f64;
            remaining -= got;
        }
    }

    let levels = energy
        .iter()
        .zip(&counts)
        .map(|(energy, count)| editor::mean_square_to_db(if *count > 0 { energy / *count as f64 } else { 0.0 }))
        .collect();
    Ok(VoiceLevels {
        clip_start,
        clip_duration,
        levels,
        voice_ids: voices.into_iter().map(|clip| clip.element_id).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Command, Element, MediaItem, MediaKind, TrackPlacement, apply};
    use std::process::Command as Process;

    /// Two synthetic clips: a steady tone as the music and a "voice" that
    /// sounds for one second in every two. Needs FFmpeg; skips otherwise.
    #[test]
    fn ducks_the_music_while_the_voice_sounds() {
        let Ok(ffmpeg) = Ffmpeg::locate() else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let ffmpeg = Arc::new(ffmpeg);
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("music.wav");
        let voice = dir.path().join("voice.wav");
        for (path, source) in [
            (&music, "sine=frequency=220:duration=6"),
            (&voice, "aevalsrc=0.5*sin(440*2*PI*t)*gt(mod(t\\,2)\\,1):d=6"),
        ] {
            let status = Process::new(ffmpeg.ffmpeg_path())
                .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", source])
                .arg(path)
                .status()
                .unwrap();
            assert!(status.success(), "ffmpeg could not write {}", path.display());
        }

        let mut project = Project::new("Ducking test");
        let mut ids = Vec::new();
        for (id, path) in [("music", &music), ("voice", &voice)] {
            let info = media::probe(&ffmpeg, path).unwrap();
            apply(
                &mut project,
                &Command::AddMedia(MediaItem {
                    id: id.to_owned(),
                    name: format!("{id}.wav"),
                    kind: MediaKind::Audio,
                    path: path.clone(),
                    duration: info.duration,
                    width: None,
                    height: None,
                    fps: None,
                    has_audio: true,
                }),
            )
            .unwrap();
            let element = Element::from_media(project.media_item(id).unwrap(), time::MediaTime::ZERO);
            ids.push(element.id.clone());
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        let options = DuckingOptions::default();
        let measured = measure_voice(&ffmpeg, &project, &ids[0], None, &options).unwrap();
        assert_eq!(measured.voice_ids, vec![ids[1].clone()]);
        assert!((measured.clip_duration - 6.0).abs() < 0.05, "{}", measured.clip_duration);

        let intervals = editor::speech_intervals(&measured.levels, &options, measured.clip_start);
        assert_eq!(intervals.len(), 3, "{intervals:?}");
        for (interval, expected) in intervals.iter().zip([1.0, 3.0, 5.0]) {
            assert!((interval.start - expected).abs() < 0.1, "{interval:?}");
            assert!((interval.end - expected - 1.0).abs() < 0.1, "{interval:?}");
        }

        let keys = editor::build_ducking_keys(&intervals, measured.clip_start, measured.clip_duration, 0.0, &options);
        apply(
            &mut project,
            &Command::SetKeyframes {
                element_id: ids[0].clone(),
                path: "volume".to_owned(),
                keys: keys
                    .iter()
                    .map(|(seconds, db)| (time::MediaTime::from_seconds_f64(*seconds).unwrap(), *db))
                    .collect(),
            },
        )
        .unwrap();
        let channel = project.element(&ids[0]).unwrap().1.animations.get("volume").unwrap();
        assert_eq!(channel.keys.len(), keys.len());
        let at = |seconds: f64| channel.value_at(time::MediaTime::from_seconds_f64(seconds).unwrap()).unwrap();
        assert!((at(0.5) - 0.0).abs() < 1e-6, "quiet before speech: {}", at(0.5));
        assert!((at(1.5) + 12.0).abs() < 1e-6, "ducked during speech: {}", at(1.5));
        assert!((at(2.6) - 0.0).abs() < 1e-6, "back up after release: {}", at(2.6));
    }
}
