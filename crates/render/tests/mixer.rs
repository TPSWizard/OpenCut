//! Mixes a real tone through FFmpeg and checks that a keyframed volume
//! fades it. Skips when FFmpeg is not installed.

use std::process::Command;

use editor::{AudioClipPlan, Channel};
use media::{AudioSettings, Ffmpeg};
use render::Mixer;
use time::MediaTime;

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap()
}

#[test]
fn follows_volume_keyframes() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let tone = dir.path().join("tone.wav");
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args([
            "-hide_banner", "-loglevel", "error", "-y",
            // `sine` is generated at 1/8 of full scale; bring it to full scale.
            "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=2",
            "-af", "volume=8", "-ac", "2",
        ])
        .arg(&tone)
        .status()
        .unwrap();
    assert!(status.success());

    // 0 dB at the start, -40 dB at the end, linear in between.
    let mut volume = Channel::default();
    volume.upsert(seconds(0.0), 0.0, None);
    volume.upsert(seconds(2.0), -40.0, None);
    let clip = AudioClipPlan {
        element_id: "tone".to_owned(),
        media_id: "tone".to_owned(),
        path: tone,
        timeline_start: seconds(0.0),
        source_start: seconds(0.0),
        source_end: seconds(2.0),
        volume_db: 0.0,
        volume,
    };

    let settings = AudioSettings::default();
    let mut mixer = Mixer::new(ffmpeg.clone(), settings);
    mixer.set_clips(vec![clip]);
    let channels = settings.channels as usize;
    let mut chunk = vec![0f32; 1024 * channels];
    let (mut first, mut last) = ((0.0f64, 0usize), (0.0f64, 0usize));
    while mixer.position_seconds() < 2.0 {
        let start = mixer.position_seconds();
        mixer.fill(&mut chunk);
        let energy: f64 = chunk.iter().map(|sample| (*sample as f64).powi(2)).sum();
        if (0.05..0.3).contains(&start) {
            first = (first.0 + energy, first.1 + chunk.len());
        } else if (1.75..1.95).contains(&start) {
            last = (last.0 + energy, last.1 + chunk.len());
        }
    }
    let rms = |(energy, count): (f64, usize)| (energy / count as f64).sqrt();
    let (first, last) = (rms(first), rms(last));
    eprintln!("start RMS {first}, end RMS {last}");
    // The window near the start averages about -3.5 dB; the one near the end
    // sits near -37 dB, some 33 dB (a factor of 45) quieter. The absolute
    // level depends on the decoder's start-up, so it is only bounded loosely.
    assert!(first > 0.2, "start RMS {first}");
    assert!(last < first * 0.05, "end RMS {last} vs start {first}");
    assert!(last > first * 0.005, "end RMS {last} is too quiet for a -40 dB key");
}
