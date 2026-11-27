//! Fit music to a length (row 101) on a background thread: the part of
//! the clip's source that plays is decoded by FFmpeg to mono at the shared
//! crate's rate and fitted (`musicfit::fit`), as the web editor does with
//! its own decoder.

use std::sync::Arc;

use editor::{MUSIC_FIT_SAMPLE_RATE, MusicFit, Project, music_fit_clip, plan_audio};
use media::{AudioReader, AudioSettings, Ffmpeg};
use time::MediaTime;

/// The clip `element_id`'s playing span, mono at the crate's rate.
fn clip_samples(ffmpeg: &Arc<Ffmpeg>, project: &Project, element_id: &str) -> Result<Vec<f32>, String> {
    let (_, element) = project.element(element_id).ok_or_else(|| crate::i18n::t("The clip is no longer on the timeline").to_owned())?;
    let clip = plan_audio(project)
        .into_iter()
        .find(|clip| clip.element_id == element_id)
        .ok_or_else(|| crate::i18n::t("The clip has no audio to analyse").to_owned())?;
    let from = element.trim_start.to_seconds_f64();
    let rate = MUSIC_FIT_SAMPLE_RATE as f64;
    let settings = AudioSettings { sample_rate: MUSIC_FIT_SAMPLE_RATE, channels: 1 };
    let mut reader = AudioReader::open(ffmpeg, &clip.path, from, settings).map_err(|error| error.to_string())?;
    let wanted = (element.duration.to_seconds_f64() * rate).round() as usize;
    let mut samples: Vec<f32> = Vec::with_capacity(wanted);
    let mut buffer = vec![0.0f32; MUSIC_FIT_SAMPLE_RATE as usize];
    while samples.len() < wanted {
        let want = (wanted - samples.len()).min(buffer.len());
        let got = reader.read(&mut buffer[..want]).map_err(|error| error.to_string())?;
        if got == 0 {
            break;
        }
        samples.extend_from_slice(&buffer[..got]);
    }
    if samples.is_empty() {
        return Err(crate::i18n::t("No audio could be read from the clip").to_owned());
    }
    Ok(samples)
}

/// Fits the clip to `length`.
pub(crate) fn fit_clip(ffmpeg: &Arc<Ffmpeg>, project: &Project, element_id: &str, length: MediaTime) -> Result<MusicFit, String> {
    let (_, element) = project.element(element_id).ok_or_else(|| crate::i18n::t("The clip is no longer on the timeline").to_owned())?;
    let clip = music_fit_clip(element);
    let samples = clip_samples(ffmpeg, project, element_id)?;
    editor::musicfit_crate::fit(&samples, MUSIC_FIT_SAMPLE_RATE, &clip, length.as_ticks())
        .map_err(|error| crate::i18n::t(error.message()).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Element, Lane, MediaItem, MediaKind, Track, TrackKind, apply, music_fit_command};

    /// A 120 BPM song with two alternating sections, written as a WAV,
    /// fitted through FFmpeg to a shorter and a longer length.
    #[test]
    fn fits_a_clip_through_ffmpeg() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = std::env::temp_dir().join(format!("opencut-musicfit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("song.wav");
        let rate = 48_000u32;
        let seconds = 24.0;
        let samples: Vec<f32> = (0..(seconds * rate as f64) as usize)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let beat = (t / 0.5).floor() as usize;
                let since = t - beat as f64 * 0.5;
                let bar = beat / 4;
                let note = if (4..8).contains(&bar) { 440.0 } else { 330.0 };
                let accent = if beat % 4 == 0 { 0.6 } else { 0.3 };
                let kick = if since < 0.08 { accent * (-since * 40.0).exp() * (2.0 * std::f64::consts::PI * 60.0 * since).sin() } else { 0.0 };
                (kick + 0.2 * (-since * 6.0).exp() * (2.0 * std::f64::consts::PI * note * since).sin()) as f32
            })
            .collect();
        media::write_wav_float(&path, &[samples], rate).unwrap();

        let mut project = Project::new("Fit");
        let item = MediaItem {
            id: "m-song".to_owned(),
            name: "song".to_owned(),
            kind: MediaKind::Audio,
            path: path.clone(),
            duration: Some(MediaTime::from_seconds_f64(seconds).unwrap()),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        let mut element = Element::from_media(&item, MediaTime::ZERO);
        element.id = "song".to_owned();
        project.media.push(item);
        let mut track = Track::new(TrackKind::Audio, Lane::Audio, "Audio");
        track.elements.push(element);
        project.tracks.push(track);

        for target in [16.0, 40.0] {
            let length = MediaTime::from_seconds_f64(target).unwrap();
            let fit = fit_clip(&ffmpeg, &project, "song", length).unwrap();
            assert!(fit.pieces.len() >= 2, "{target}: {:?}", fit.kind);
            let bpm = fit.bpm.unwrap();
            assert!((bpm - 120.0).abs() < 1.5, "{bpm}");
            let total: i64 = fit.pieces.iter().map(|piece| piece.duration).sum();
            assert!(total <= length.as_ticks() && total >= length.as_ticks() - 6_000, "{total}");
            for jump in &fit.jumps {
                assert!(jump.score > 0.9, "{target}: {jump:?}");
            }
            let mut fitted = project.clone();
            let command = music_fit_command(&fitted, "song", &fit).unwrap();
            apply(&mut fitted, &command).unwrap();
            let track = fitted.element("song").unwrap().0;
            assert_eq!(track.elements.len(), fit.pieces.len());
            // As it plays, each piece runs 0.05 s past its join and the next
            // starts 0.05 s before it: a 0.1 s equal-power crossfade.
            let mut played: Vec<_> = plan_audio(&fitted).into_iter().collect();
            played.sort_by_key(|clip| clip.timeline_start);
            assert_eq!(played.len(), fit.pieces.len());
            for pair in played.windows(2) {
                let overlap = (pair[0].timeline_start + pair[0].timeline_duration).as_ticks() - pair[1].timeline_start.as_ticks();
                assert_eq!(overlap, 12_000, "{target}");
                assert!((pair[0].fade_out_seconds - 0.1).abs() < 1e-9 && (pair[1].fade_in_seconds - 0.1).abs() < 1e-9);
                assert_eq!(pair[0].fade_out_curve, editor::CrossfadeCurve::EqualPower);
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
