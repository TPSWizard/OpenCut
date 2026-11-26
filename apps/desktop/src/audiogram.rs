//! Audiograms (row 100) on a background thread: the part of a clip's
//! source that plays is decoded by FFmpeg to mono at the shared crate's
//! rate and analysed once per frame (`audiogram::analyse`), as the web
//! editor does with its own decoder.

use std::sync::Arc;

use editor::{AudiogramLevels, Project, audiogram_crate as audiogram, audiogram_source_times, plan_audio};
use media::{AudioReader, AudioSettings, Ffmpeg};

/// The levels for an audiogram of the clip `element_id`.
pub(crate) fn clip_levels(ffmpeg: &Arc<Ffmpeg>, project: &Project, element_id: &str) -> Result<AudiogramLevels, String> {
    let (_, element) = project.element(element_id).ok_or_else(|| crate::i18n::t("The clip is no longer on the timeline").to_owned())?;
    let clip = plan_audio(project)
        .into_iter()
        .find(|clip| clip.element_id == element_id)
        .ok_or_else(|| crate::i18n::t("The clip has no audio to analyse").to_owned())?;
    let times = audiogram_source_times(element);
    let rate = audiogram::SAMPLE_RATE as f64;
    let margin = audiogram::window_size(rate) as f64 / rate;
    let first = times.iter().copied().fold(f64::INFINITY, f64::min);
    let last = times.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let from = (first - margin).max(0.0);
    let to = last + margin;
    let settings = AudioSettings { sample_rate: audiogram::SAMPLE_RATE, channels: 1 };
    let mut reader = AudioReader::open(ffmpeg, &clip.path, from, settings).map_err(|error| error.to_string())?;
    let wanted = ((to - from) * rate).round() as usize;
    let mut samples: Vec<f32> = Vec::with_capacity(wanted);
    let mut buffer = vec![0.0f32; audiogram::SAMPLE_RATE as usize];
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
    let relative: Vec<f64> = times.iter().map(|time| time - from).collect();
    Ok(audiogram::analyse(&samples, rate, &relative, audiogram::BANDS, audiogram::FPS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Element, MediaItem, MediaKind, Track, TrackKind, Lane};
    use time::MediaTime;

    /// Writes a WAV with a 1 kHz tone for its first second and silence for
    /// its second, makes it a clip, and checks the levels.
    #[test]
    fn analyses_a_clip_through_ffmpeg() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = std::env::temp_dir().join(format!("opencut-audiogram-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        let rate = 48_000u32;
        let samples: Vec<f32> = (0..rate * 2)
            .map(|i| if i < rate { (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / rate as f64).sin() as f32 * 0.5 } else { 0.0 })
            .collect();
        media::write_wav_float(&path, &[samples], rate).unwrap();

        let mut project = Project::new("Audiogram");
        let item = MediaItem {
            id: "m-tone".to_owned(),
            name: "tone".to_owned(),
            kind: MediaKind::Audio,
            path: path.clone(),
            duration: Some(MediaTime::from_seconds_f64(2.0).unwrap()),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        let mut element = Element::from_media(&item, MediaTime::ZERO);
        element.id = "clip".to_owned();
        project.media.push(item);
        let mut track = Track::new(TrackKind::Audio, Lane::Audio, "Audio");
        track.elements.push(element);
        project.tracks.push(track);

        let levels = clip_levels(&ffmpeg, &project, "clip").unwrap();
        assert_eq!(levels.frames(), 60);
        let edges: Vec<f64> = (0..=audiogram::BANDS)
            .map(|k| audiogram::MIN_HZ * (audiogram::MAX_HZ / audiogram::MIN_HZ).powf(k as f64 / audiogram::BANDS as f64))
            .collect();
        let band = edges.windows(2).position(|e| e[0] <= 1000.0 && 1000.0 < e[1]).unwrap();
        let at = |frame: usize, band: usize| levels.data[frame * audiogram::BANDS + band];
        // Half scale is -6 dB: 0.9 of the way up.
        assert!((220..=235).contains(&at(15, band)), "{}", at(15, band));
        assert!(at(15, 0) < 40);
        assert_eq!(at(59, band), 0, "fallen back by the end");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod check {
    use super::*;
    use editor::{Element, Lane, MediaItem, MediaKind, Track, TrackKind};
    use time::MediaTime;

    /// `OPENCUT_AUDIOGRAM_CHECK=<audio file>` analyses that file as a whole
    /// clip and prints, for a few frames, the loudest band, its level and
    /// the sum of the levels (to compare with the web editor).
    #[test]
    fn summarises_a_file() {
        let Ok(path) = std::env::var("OPENCUT_AUDIOGRAM_CHECK") else { return };
        let ffmpeg = Ffmpeg::locate().expect("ffmpeg");
        let path = std::path::PathBuf::from(path);
        let probed = media::probe(&ffmpeg, &path).expect("probe");
        let item = MediaItem {
            id: "m-check".to_owned(),
            name: "check".to_owned(),
            kind: MediaKind::Audio,
            path,
            duration: probed.duration,
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        let mut element = Element::from_media(&item, MediaTime::ZERO);
        element.id = "clip".to_owned();
        let mut project = Project::new("Check");
        project.media.push(item);
        let mut track = Track::new(TrackKind::Audio, Lane::Audio, "Audio");
        track.elements.push(element);
        project.tracks.push(track);
        let levels = clip_levels(&ffmpeg, &project, "clip").unwrap();
        let summary: Vec<(usize, usize, u8, u32)> = [5usize, 20, 37, 70, 100]
            .iter()
            .map(|&frame| {
                let row = &levels.data[frame * levels.bands..(frame + 1) * levels.bands];
                let max = *row.iter().max().unwrap();
                (frame, row.iter().position(|&v| v == max).unwrap(), max, row.iter().map(|&v| v as u32).sum())
            })
            .collect();
        eprintln!("frames {} summary {summary:?}", levels.frames());
        // The default look drawn 3.1 s in at 1920x1080: bounds of the bright pixels.
        let (w, h) = (1920usize, 1080usize);
        let rgba = audiogram::render(&levels, &editor::AudiogramLook::default(), 3.1, w, h);
        let lit = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3] > 128;
        let (mut x0, mut x1, mut y0, mut y1, mut n) = (usize::MAX, 0, usize::MAX, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if lit(x, y) {
                    n += 1;
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        let tallest = (x0..=x1).max_by_key(|&x| ((0..h).filter(|&y| lit(x, y)).count(), std::cmp::Reverse(x))).unwrap();
        eprintln!("at 3.1 s: x {x0}..{x1} y {y0}..{y1} lit {n} tallest column {tallest}");
    }
}
