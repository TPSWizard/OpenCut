//! Track volume, pan and solo (row 83) with real tones, heard through the
//! live mixer and through the export's FFmpeg mix, and the mixer's
//! per-track meters. Skips without FFmpeg.

use std::path::Path;
use std::sync::Arc;

use editor::{Command, Element, MediaItem, MediaKind, Project, TrackKind, TrackPlacement, apply, plan_audio};
use media::{AudioMixClip, AudioReader, AudioSettings, EncodeFormat, EncodeQuality, EncodeSettings, Encoder, Ffmpeg};
use render::Mixer;
use time::MediaTime;

fn tone(ffmpeg: &Ffmpeg, path: &Path, frequency: u32) {
    let status = std::process::Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency={frequency}:sample_rate=48000:duration=3"))
        .args(["-af", "volume=4", "-ac", "2"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// RMS in dB of the left and right channels from 1 s to 2 s.
fn levels(frames: &[[f32; 2]]) -> (f64, f64) {
    let window = &frames[48_000..96_000];
    let db = |channel: usize| {
        let power = window.iter().map(|frame| (frame[channel] as f64).powi(2)).sum::<f64>() / window.len() as f64;
        10.0 * power.log10()
    };
    (db(0), db(1))
}

fn live(ffmpeg: &Arc<Ffmpeg>, project: &Project) -> (Vec<[f32; 2]>, Vec<(String, f64)>) {
    let mut mixer = Mixer::new(ffmpeg.clone(), AudioSettings::default());
    mixer.set_clips(plan_audio(project));
    let mut frames = Vec::new();
    let mut chunk = vec![0f32; 1024 * 2];
    while frames.len() < 48_000 * 2 {
        mixer.fill(&mut chunk);
        frames.extend(chunk.chunks_exact(2).map(|pair| [pair[0], pair[1]]));
    }
    let meters = mixer.track_meters();
    let meters = meters.lock().unwrap();
    let mut readings: Vec<(String, f64)> = meters.iter().map(|(id, meter)| (id.clone(), meter.readings().momentary_lufs)).collect();
    readings.sort_by(|a, b| a.0.cmp(&b.0));
    (frames, readings)
}

fn exported(ffmpeg: &Arc<Ffmpeg>, project: &Project, output: &Path) -> Vec<[f32; 2]> {
    let audio = plan_audio(project)
        .into_iter()
        .map(|clip| AudioMixClip {
            path: clip.path.clone(),
            source_start_seconds: clip.source_start.to_seconds_f64(),
            source_end_seconds: clip.source_end.to_seconds_f64(),
            timeline_start_seconds: clip.timeline_start.to_seconds_f64(),
            gain: clip.gain(),
            gain_keys: Vec::new(),
            fade_in_seconds: 0.0,
            fade_out_seconds: 0.0,
            fade_in_equal_power: false,
            fade_out_equal_power: false,
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
        duration_seconds: 3.0,
        audio,
        disclosure: None,
    };
    let mut encoder = Encoder::start(ffmpeg, &settings).unwrap();
    for _ in 0..30 {
        encoder.write_frame(&vec![255u8; 64 * 64 * 4]).unwrap();
    }
    let written = encoder.finish().unwrap();
    let mut reader = AudioReader::open(ffmpeg, &written, 0.0, AudioSettings::default()).unwrap();
    let mut samples = vec![0f32; 48_000 * 2 * 3];
    let read = reader.read(&mut samples).unwrap();
    samples[..read].chunks_exact(2).map(|pair| [pair[0], pair[1]]).collect()
}

#[test]
fn track_volume_pan_and_solo_are_heard_live_and_in_exports() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let ffmpeg = Arc::new(ffmpeg);
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Tracks");
    let mut tracks = Vec::new();
    for (id, frequency) in [("low", 300), ("high", 700)] {
        let path = dir.path().join(format!("{id}.wav"));
        tone(&ffmpeg, &path, frequency);
        let media = MediaItem {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: MediaKind::Audio,
            path,
            duration: MediaTime::from_seconds_f64(3.0),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        apply(&mut project, &Command::AddMedia(media.clone())).unwrap();
        apply(&mut project, &Command::AddTrack { kind: TrackKind::Audio }).unwrap();
        let track = project.tracks.iter().rev().find(|track| track.kind == TrackKind::Audio && track.elements.is_empty()).unwrap().id.clone();
        apply(&mut project, &Command::InsertElement { element: Element::from_media(&media, MediaTime::ZERO), placement: TrackPlacement::Track(track.clone()) })
            .unwrap();
        tracks.push(track);
    }

    // One tone alone, as it is: its level on each side.
    apply(&mut project, &Command::SetTrackMuted { track_id: tracks[1].clone(), muted: true }).unwrap();
    let (plain, _) = live(&ffmpeg, &project);
    let (plain_left, plain_right) = levels(&plain);
    eprintln!("plain {plain_left:.2} / {plain_right:.2}");

    // The same track at -6 dB, panned fully left: the right goes silent and
    // the left gets the right's share too (+3 dB for a centred tone).
    apply(&mut project, &Command::SetTrackMix { track_id: tracks[0].clone(), solo: None, volume: Some(-6.0), pan: Some(-1.0) }).unwrap();
    let (panned, _) = live(&ffmpeg, &project);
    let exported_panned = exported(&ffmpeg, &project, &dir.path().join("panned.mp4"));
    for (name, frames) in [("live", &panned), ("export", &exported_panned)] {
        let (left, right) = levels(frames);
        eprintln!("{name}: {left:.2} / {right:.2}");
        assert!((left - (plain_left - 6.0 + 6.02)).abs() < 0.3, "{name}: left {left} vs plain {plain_left}");
        assert!(right < plain_right - 40.0, "{name}: right {right} should be silent");
    }

    // Solo the other (unmuted) track: only it plays, and its meter moves.
    apply(&mut project, &Command::SetTrackMuted { track_id: tracks[1].clone(), muted: false }).unwrap();
    apply(&mut project, &Command::SetTrackMix { track_id: tracks[1].clone(), solo: Some(true), volume: None, pan: None }).unwrap();
    let plans = plan_audio(&project);
    assert_eq!(plans.iter().map(|plan| plan.media_id.as_str()).collect::<Vec<_>>(), vec!["high"]);
    let (soloed, meters) = live(&ffmpeg, &project);
    let (left, right) = levels(&soloed);
    assert!((left - plain_left).abs() < 0.3 && (right - plain_right).abs() < 0.3, "{left} {right}");
    assert_eq!(meters.len(), 1, "{meters:?}");
    assert_eq!(meters[0].0, tracks[1]);
    eprintln!("soloed meter {:?}", meters);
    // A full-scale-ish stereo tone at 0.5: about -6 dB a side, -3 LUFS more for two channels.
    assert!(meters[0].1 > -12.0 && meters[0].1 < -3.0, "{meters:?}");
}
