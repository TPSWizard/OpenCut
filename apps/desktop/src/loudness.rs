//! Measures a clip's loudness (EBU R128) on a background thread: the part
//! of the source that plays is decoded by FFmpeg to 48 kHz stereo and run
//! through `editor::measure_loudness`.

use std::sync::Arc;

use editor::{LoudnessReport, Project, deinterleave, measure_loudness, plan_audio};
use media::{AudioReader, AudioSettings, Ffmpeg};

/// The rate the standard's figures are given for.
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

/// The loudness of the clip `element_id` as it plays: its trimmed source
/// span, before its own volume.
pub(crate) fn measure_clip(ffmpeg: &Arc<Ffmpeg>, project: &Project, element_id: &str) -> Result<LoudnessReport, String> {
    let clip = plan_audio(project)
        .into_iter()
        .find(|clip| clip.element_id == element_id)
        .ok_or_else(|| "The clip has no audio to measure".to_owned())?;
    let from = clip.source_start.to_seconds_f64();
    let to = clip.source_end.to_seconds_f64();
    if to <= from {
        return Err("The clip's span holds no audio".to_owned());
    }
    let settings = AudioSettings { sample_rate: SAMPLE_RATE, channels: CHANNELS };
    let mut reader = AudioReader::open(ffmpeg, &clip.path, from, settings).map_err(|error| error.to_string())?;
    let wanted = ((to - from) * SAMPLE_RATE as f64).round() as usize * CHANNELS as usize;
    let mut samples: Vec<f32> = Vec::with_capacity(wanted);
    let mut buffer = vec![0.0f32; SAMPLE_RATE as usize * CHANNELS as usize];
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
    let channels = deinterleave(&samples, CHANNELS as usize);
    Ok(measure_loudness(&channels, SAMPLE_RATE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Element, Lane, MediaItem, MediaKind, Project, Track, TrackKind};
    use std::path::PathBuf;
    use time::MediaTime;

    /// `OPENCUT_LOUDNESS_CHECK=<audio file>` measures that file as a clip
    /// (optionally `OPENCUT_LOUDNESS_EXPECT=<LUFS>` to assert within 0.2).
    #[test]
    fn measures_a_file_as_a_clip() {
        let Ok(path) = std::env::var("OPENCUT_LOUDNESS_CHECK") else { return };
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let path = PathBuf::from(path);
        let probed = media::probe(&ffmpeg, &path).expect("probe");
        let mut project = Project::new("Loudness");
        let media = MediaItem {
            id: "m-loud".to_owned(),
            name: "loud".to_owned(),
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
        let report = measure_clip(&ffmpeg, &project, &element_id).expect("measure");
        eprintln!("{report:?}");
        if let Ok(expected) = std::env::var("OPENCUT_LOUDNESS_EXPECT") {
            let expected: f64 = expected.parse().unwrap();
            assert!((report.integrated_lufs - expected).abs() < 0.2, "{} vs {expected}", report.integrated_lufs);
        }
    }
}
