//! Whole-mix loudness at export (row 82): a quiet music bed and a loud
//! clicky clip on two tracks are exported at the streaming target, then the
//! MP4's audio is decoded and measured. Skips without FFmpeg or a GPU.

use std::path::Path;

use editor::{CanvasSize, Command as EditorCommand, Element, MediaItem, MediaKind, Project, TrackPlacement, apply, measure_loudness};
use media::{AudioReader, AudioSettings, Ffmpeg};
use render::{ExportJob, MixLoudness, RenderError, Renderer, RendererOptions};
use time::MediaTime;

fn tone(ffmpeg: &Ffmpeg, path: &Path, expression: &str) {
    let status = std::process::Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("aevalsrc={expression}:s=48000:d=4"))
        .args(["-ac", "2"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

fn decoded(ffmpeg: &Ffmpeg, path: &Path) -> Vec<Vec<f32>> {
    let mut reader = AudioReader::open(ffmpeg, path, 0.0, AudioSettings::default()).unwrap();
    let mut interleaved = Vec::new();
    let mut buffer = vec![0f32; 48_000 * 2];
    loop {
        let read = reader.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        interleaved.extend_from_slice(&buffer[..read]);
    }
    editor::deinterleave(&interleaved, 2)
}

#[test]
fn the_export_lands_on_the_target_under_the_ceiling() {
    let Ok(ffmpeg) = Ffmpeg::locate() else { return };
    let mut renderer = match Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => renderer,
        Err(RenderError::Gpu(error)) => return eprintln!("no GPU adapter ({error}); skipping"),
        Err(error) => panic!("{error}"),
    };
    let dir = tempfile::tempdir().unwrap();
    // A quiet 220 Hz bed, and 1 kHz with sharp clicks every half second.
    let bed = dir.path().join("bed.wav");
    tone(&ffmpeg, &bed, "0.05*sin(2*PI*220*t)");
    let clicks = dir.path().join("clicks.wav");
    tone(&ffmpeg, &clicks, "0.03*sin(2*PI*1000*t)+0.9*lt(mod(t\\,0.5)\\,0.002)");

    let mut project = Project::new("Mix");
    project.settings.canvas = CanvasSize { width: 160, height: 90 };
    for (id, path) in [("bed", &bed), ("clicks", &clicks)] {
        let media = MediaItem {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: MediaKind::Audio,
            path: path.clone(),
            duration: MediaTime::from_seconds_f64(4.0),
            width: None,
            height: None,
            fps: None,
            has_audio: true,
        };
        apply(&mut project, &EditorCommand::AddMedia(media.clone())).unwrap();
        // Each on its own audio track.
        apply(&mut project, &EditorCommand::AddTrack { kind: editor::TrackKind::Audio }).unwrap();
        let track = project.tracks.iter().rev().find(|track| track.kind == editor::TrackKind::Audio && track.elements.is_empty()).unwrap().id.clone();
        apply(&mut project, &EditorCommand::InsertElement { element: Element::from_media(&media, MediaTime::ZERO), placement: TrackPlacement::Track(track) })
            .unwrap();
    }

    let mut levels = Vec::new();
    for loudness in [None, MixLoudness::target("streaming"), MixLoudness::target("broadcast")] {
        let output = dir.path().join(format!("out-{}.mp4", levels.len()));
        let job = ExportJob {
            output: output.clone(),
            format: media::EncodeFormat::Mp4,
            quality: media::EncodeQuality::Low,
            platform: None,
            codec: None,
            video_bitrate: None,
            hardware: false,
            include_audio: true,
            cache_dir: None,
            loudness,
            alpha: None,
        };
        let (path, report) = render::export_project_with_report(&mut renderer, &ffmpeg, &project, &job, &|| false, &mut |_, _| {}).unwrap();
        assert_eq!(report.is_some(), loudness.is_some());
        let measured = measure_loudness(&decoded(&ffmpeg, &path), 48_000);
        eprintln!("{loudness:?}: report {report:?}; measured {:.2} LUFS, {:.2} dBTP", measured.integrated_lufs, measured.true_peak_dbtp);
        if let (Some(loudness), Some(report)) = (loudness, report) {
            // AAC moves the level a little; within half a unit of the target.
            assert!((measured.integrated_lufs - loudness.target_lufs).abs() < 0.5, "{} vs {}", measured.integrated_lufs, loudness.target_lufs);
            assert!((report.output_lufs - loudness.target_lufs).abs() < 0.25, "{report:?}");
            assert!(report.output_true_peak_dbtp <= loudness.ceiling_dbtp, "{report:?}");
            // The encoder's own overshoot stays small.
            assert!(measured.true_peak_dbtp < loudness.ceiling_dbtp + 0.6, "{}", measured.true_peak_dbtp);
        }
        levels.push(measured);
    }
    // Unmastered the clicks peak near full scale and the mix is quieter than -14.
    assert!(levels[0].true_peak_dbtp > -4.0 && levels[0].integrated_lufs < -20.0, "{:?}", levels[0]);
    // Streaming needed the limiter to get there.
}
