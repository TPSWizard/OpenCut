//! Beat markers for a clip on a background thread: the part of the source
//! that plays is decoded by FFmpeg to 16 kHz mono and beat-tracked with
//! `editor::track_beats`.

use std::sync::Arc;

use editor::{BeatAnalysis, BeatOptions, Project, plan_audio, track_beats};
use media::{AudioReader, AudioSettings, Ffmpeg};

const SAMPLE_RATE: u32 = 16_000;

/// The beats of the clip `element_id`, in seconds of its source.
pub(crate) fn clip_beats(ffmpeg: &Arc<Ffmpeg>, project: &Project, element_id: &str) -> Result<(BeatAnalysis, f64), String> {
    let clip = plan_audio(project)
        .into_iter()
        .find(|clip| clip.element_id == element_id)
        .ok_or_else(|| "The clip has no audio to analyse".to_owned())?;
    let from = clip.source_start.to_seconds_f64();
    let to = clip.source_end.to_seconds_f64();
    if to <= from {
        return Err("The clip's span holds no audio".to_owned());
    }
    let settings = AudioSettings { sample_rate: SAMPLE_RATE, channels: 1 };
    let mut reader = AudioReader::open(ffmpeg, &clip.path, from, settings).map_err(|error| error.to_string())?;
    let wanted = ((to - from) * SAMPLE_RATE as f64).round() as usize;
    let mut samples: Vec<f32> = Vec::with_capacity(wanted);
    let mut buffer = vec![0.0f32; SAMPLE_RATE as usize];
    while samples.len() < wanted {
        let want = (wanted - samples.len()).min(buffer.len());
        let got = reader.read(&mut buffer[..want]).map_err(|error| error.to_string())?;
        if got == 0 {
            break;
        }
        samples.extend_from_slice(&buffer[..got]);
    }
    if samples.is_empty() {
        return Err("No audio could be read from the clip".to_owned());
    }
    Ok((track_beats(&samples, SAMPLE_RATE, &BeatOptions::default()), from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Element, Lane, MediaItem, MediaKind, Track, TrackKind};
    use std::path::PathBuf;
    use time::MediaTime;

    /// `OPENCUT_BEATS_CHECK=<audio file>` beat-tracks that file as a clip
    /// (optionally `OPENCUT_BEATS_EXPECT=<bpm>` to assert within 1.5).
    #[test]
    fn tracks_a_file_as_a_clip() {
        let Ok(path) = std::env::var("OPENCUT_BEATS_CHECK") else { return };
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let path = PathBuf::from(path);
        let probed = media::probe(&ffmpeg, &path).expect("probe");
        let mut project = Project::new("Beats");
        let media = MediaItem {
            id: "m-beats".to_owned(),
            name: "beats".to_owned(),
            kind: MediaKind::Audio,
            path,
            duration: probed.duration,
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        let element = Element::from_media(&media, MediaTime::ZERO);
        let element_id = element.id.clone();
        project.media.push(media);
        let mut track = Track::new(TrackKind::Audio, Lane::Audio, "Audio");
        track.elements.push(element);
        project.tracks.push(track);
        let (analysis, from) = clip_beats(&ffmpeg, &project, &element_id).expect("beats");
        eprintln!("bpm {:?} confidence {:.2} beats {} from {from}", analysis.bpm, analysis.confidence, analysis.beats.len());
        if let Ok(expected) = std::env::var("OPENCUT_BEATS_EXPECT") {
            let expected: f64 = expected.parse().unwrap();
            assert!((analysis.bpm.unwrap() - expected).abs() < 1.5, "{:?} vs {expected}", analysis.bpm);
        }
    }
}
