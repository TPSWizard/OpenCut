//! More export kinds (row 96): a short project with a moving test pattern
//! and a tone exported as a GIF, as WAV / AAC / Opus audio, as a still PNG
//! and as a PNG sequence, each read back. Skips without FFmpeg or a GPU.

use std::path::Path;

use editor::{CanvasSize, Command as EditorCommand, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use image::AnimationDecoder;
use media::{AudioFileFormat, Ffmpeg};
use render::{GifJob, RenderError, Renderer, RendererOptions};
use time::MediaTime;

fn clip(ffmpeg: &Ffmpeg, path: &Path) {
    let status = std::process::Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=160x90:rate=30:d=2"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:d=2", "-shortest", "-pix_fmt", "yuv420p", "-c:a", "aac"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn gif_audio_and_pictures_read_back() {
    let Ok(ffmpeg) = Ffmpeg::locate() else { return };
    let mut renderer = match Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => renderer,
        Err(RenderError::Gpu(error)) => return eprintln!("no GPU adapter ({error}); skipping"),
        Err(error) => panic!("{error}"),
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("pattern.mp4");
    clip(&ffmpeg, &source);
    let mut project = Project::new("Kinds");
    project.settings.canvas = CanvasSize { width: 160, height: 90 };
    let media = MediaItem {
        id: "pattern".to_owned(),
        name: "pattern.mp4".to_owned(),
        kind: MediaKind::Video,
        path: source,
        duration: MediaTime::from_seconds_f64(2.0),
        width: Some(160),
        height: Some(90),
        fps: Some(30.0),
        has_audio: true,
    };
    apply(&mut project, &EditorCommand::AddMedia(media.clone())).unwrap();
    apply(&mut project, &EditorCommand::InsertElement { element: Element::from_media(&media, MediaTime::ZERO), placement: TrackPlacement::Auto }).unwrap();

    // GIF: 80 px wide at 10 fps, so 20 frames over 2 s.
    let gif_path = dir.path().join("out.gif");
    let job = GifJob { output: gif_path.clone(), width: 80, fps: 10.0, colours: 255, dither: true };
    let mut steps = (0, 0);
    render::export_gif(&mut renderer, &project, &job, &|| false, &mut |done, total| steps = (done, total)).unwrap();
    assert_eq!(steps.0, steps.1, "progress reaches the end");
    let bytes = std::fs::read(&gif_path).unwrap();
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
    let frames = decoder.into_frames().collect_frames().unwrap();
    let total_cs: u32 = frames
        .iter()
        .map(|frame| {
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            numerator / denominator / 10
        })
        .sum();
    eprintln!("GIF {} bytes, {} frames, {} cs", bytes.len(), frames.len(), total_cs);
    assert_eq!(total_cs, 200, "the GIF lasts the project's 2 s");
    assert!(frames.len() > 10, "the pattern moves: {}", frames.len());
    let first = frames[0].buffer();
    assert_eq!((first.width(), first.height()), (80, 44));
    // The first GIF frame looks like the rendered frame (palette + dither error only).
    let rendered = renderer.render(&project, MediaTime::ZERO, 80, 44).unwrap();
    let mut error = 0u64;
    for (gif, bgra) in first.as_raw().chunks_exact(4).zip(rendered.bgra.chunks_exact(4)) {
        for (a, b) in [(gif[0], bgra[2]), (gif[1], bgra[1]), (gif[2], bgra[0])] {
            error += u64::from(a.abs_diff(b));
        }
    }
    let mean = error as f64 / (80.0 * 44.0 * 3.0);
    eprintln!("mean error per channel {mean:.2}");
    assert!(mean < 10.0, "{mean}");

    // Audio only: each format decodes to about two seconds of the tone.
    for format in [AudioFileFormat::Wav, AudioFileFormat::Aac, AudioFileFormat::Opus] {
        let output = dir.path().join(format!("sound.{}", format.extension()));
        let (path, report) = render::export_audio(&ffmpeg, &project, &output, format, None, None).unwrap();
        assert!(report.is_none());
        let info = media::probe(&ffmpeg, &path).unwrap();
        let seconds = info.duration.unwrap().to_seconds_f64();
        eprintln!("{format:?}: {} bytes, {seconds:.3} s, video {}", std::fs::metadata(&path).unwrap().len(), info.has_video);
        assert!(info.has_audio && !info.has_video);
        assert!((seconds - 2.0).abs() < 0.1, "{seconds}");
    }
    let (_, report) = render::export_audio(&ffmpeg, &project, &dir.path().join("loud.wav"), AudioFileFormat::Wav, render::MixLoudness::target("streaming"), None).unwrap();
    assert!(report.is_some_and(|report| (report.output_lufs + 14.0).abs() < 0.3));

    // A still and the sequence.
    let still = render::export_png(&mut renderer, &project, MediaTime::from_seconds_f64(1.0).unwrap(), &dir.path().join("still.png")).unwrap();
    assert_eq!(image::image_dimensions(&still).unwrap(), (160, 90));
    let folder = dir.path().join("frames");
    let (_, count) = render::export_png_sequence(&mut renderer, &project, &folder, &|| false, &mut |_, _| {}).unwrap();
    assert_eq!(count, 60, "2 s at 30 fps");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 60);
    assert!(folder.join(render::sequence_file_name(60)).exists());
}
