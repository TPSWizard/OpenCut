//! A real crossfade between two tones, made with the one-step command and
//! heard through the desktop's live mixer and through the export's FFmpeg
//! mix: at equal power the loudness stays steady through the join, and the
//! linear curve dips about 3 dB in the middle. Skips without FFmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;

use editor::{
    Command as EditCommand, CrossfadeCurve, Element, MediaItem, MediaKind, Project, TrackPlacement, apply, crossfade_commands, plan_audio,
};
use media::{AudioMixClip, AudioReader, AudioSettings, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg};
use render::Mixer;
use time::MediaTime;

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap()
}

fn tone(ffmpeg: &Ffmpeg, path: &Path, frequency: u32) {
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency={frequency}:sample_rate=48000:duration=4"))
        .args(["-af", "volume=4", "-ac", "2"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// Two whole 4 s tones butted together on one track, crossfaded over 1 s.
fn project(paths: &[PathBuf; 2], curve: CrossfadeCurve) -> Project {
    let mut project = Project::new("Crossfade");
    for (index, path) in paths.iter().enumerate() {
        let media = MediaItem {
            id: format!("m{index}"),
            name: format!("m{index}"),
            kind: MediaKind::Audio,
            path: path.clone(),
            duration: Some(seconds(4.0)),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        apply(&mut project, &EditCommand::AddMedia(media.clone())).unwrap();
        let mut element = Element::from_media(&media, seconds(index as f64 * 4.0));
        element.id = format!("a{index}");
        let placement = if index == 0 { TrackPlacement::Auto } else { TrackPlacement::Track(project.element("a0").unwrap().0.id.clone()) };
        apply(&mut project, &EditCommand::InsertElement { element, placement }).unwrap();
    }
    let outcome = crossfade_commands(&project, "a0", 1.0, curve).unwrap();
    apply(&mut project, &outcome.command).unwrap();
    project
}

/// RMS in decibels of each 0.1 s window from 2.5 s to 4.5 s (the join is 3 to 4 s).
fn windows(frames: &[[f32; 2]]) -> Vec<f64> {
    (0..20)
        .map(|index| {
            let start = (2.5 + index as f64 * 0.1) * 48_000.0;
            let slice = &frames[start as usize..(start + 4_800.0) as usize];
            let power = slice.iter().map(|frame| (frame[0] as f64).powi(2)).sum::<f64>() / slice.len() as f64;
            10.0 * power.log10()
        })
        .collect()
}

fn live_mix(ffmpeg: &std::sync::Arc<Ffmpeg>, project: &Project) -> Vec<[f32; 2]> {
    let mut mixer = Mixer::new(ffmpeg.clone(), AudioSettings::default());
    mixer.set_clips(plan_audio(project));
    let mut frames = Vec::new();
    let mut chunk = vec![0f32; 1024 * 2];
    while frames.len() < 48_000 * 5 {
        mixer.fill(&mut chunk);
        frames.extend(chunk.chunks_exact(2).map(|pair| [pair[0], pair[1]]));
    }
    frames
}

fn exported_mix(ffmpeg: &std::sync::Arc<Ffmpeg>, project: &Project, output: &Path) -> Vec<[f32; 2]> {
    // As `render::export_project` maps the plans.
    let audio = plan_audio(project)
        .into_iter()
        .map(|clip| AudioMixClip {
            path: clip.path.clone(),
            source_start_seconds: clip.source_start.to_seconds_f64(),
            source_end_seconds: clip.source_end.to_seconds_f64(),
            timeline_start_seconds: clip.timeline_start.to_seconds_f64(),
            gain: clip.gain(),
            gain_keys: Vec::new(),
            fade_in_seconds: clip.fade_in_seconds,
            fade_out_seconds: clip.fade_out_seconds,
            fade_in_equal_power: clip.fade_in_curve == CrossfadeCurve::EqualPower,
            fade_out_equal_power: clip.fade_out_curve == CrossfadeCurve::EqualPower,
            pan: clip.pan,
            track_pan: clip.track_pan,
        })
        .collect();
    let settings = EncodeSettings {
        output: output.to_owned(),
        width: 64,
        height: 64,
        fps: 10.0,
        format: EncodeFormat::Mp4,
        quality: EncodeQuality::Medium,
        platform: None,
        codec: None,
        video_bitrate: None,
        hardware: false,
        duration_seconds: 6.0,
        audio,
        disclosure: None,
        alpha: None,
    };
    let mut encoder = Encoder::start(ffmpeg, &settings).unwrap();
    for _ in 0..60 {
        encoder.write_frame(&vec![255u8; 64 * 64 * 4]).unwrap();
    }
    let written = encoder.finish().unwrap();
    let mut reader = AudioReader::open(ffmpeg, &written, 0.0, AudioSettings::default()).unwrap();
    let mut samples = vec![0f32; 48_000 * 2 * 5];
    let read = reader.read(&mut samples).unwrap();
    samples[..read].chunks_exact(2).map(|pair| [pair[0], pair[1]]).collect()
}

#[test]
fn equal_power_holds_the_level_through_the_join() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let ffmpeg = std::sync::Arc::new(ffmpeg);
    let dir = tempfile::tempdir().unwrap();
    let paths = [dir.path().join("a.wav"), dir.path().join("b.wav")];
    tone(&ffmpeg, &paths[0], 440);
    tone(&ffmpeg, &paths[1], 620);

    for curve in [CrossfadeCurve::EqualPower, CrossfadeCurve::Linear] {
        let project = project(&paths, curve);
        let live = windows(&live_mix(&ffmpeg, &project));
        let exported = windows(&exported_mix(&ffmpeg, &project, &dir.path().join(format!("{curve:?}.mp4"))));
        eprintln!("{curve:?} live {live:.2?}");
        eprintln!("{curve:?} export {exported:.2?}");
        for levels in [&live, &exported] {
            let outside = levels[0];
            let middle = levels[10]; // 3.5 s, the middle of the join
            let spread = levels.iter().cloned().fold(f64::MIN, f64::max) - levels.iter().cloned().fold(f64::MAX, f64::min);
            match curve {
                CrossfadeCurve::EqualPower => assert!(spread < 1.0, "{curve:?}: level moves {spread:.2} dB: {levels:.2?}"),
                CrossfadeCurve::Linear => {
                    assert!((outside - middle - 3.0).abs() < 0.7, "{curve:?}: dip {:.2} dB: {levels:.2?}", outside - middle)
                }
            }
        }
        // The live mixer and the export agree within half a decibel.
        for (live, exported) in live.iter().zip(&exported) {
            assert!((live - exported).abs() < 0.5, "{curve:?}: live {live:.2} vs export {exported:.2}");
        }
    }
}
