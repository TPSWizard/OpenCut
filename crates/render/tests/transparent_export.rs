//! Transparent export (row 97): a square test pattern at half opacity in
//! the middle of a wider canvas, exported with nothing behind it as ProRes
//! 4444 in MOV, VP9 with alpha in WebM and transparent PNGs. Each file is
//! decoded back with FFmpeg and its alpha compared with the rendered frame.
//! Skips without FFmpeg or a GPU.

use std::path::Path;

use editor::{CanvasSize, Command as EditorCommand, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use media::{AlphaCodec, EncodeFormat, EncodeQuality, Ffmpeg};
use render::{ExportJob, RenderError, Renderer, RendererOptions};
use time::MediaTime;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;

fn square_pattern(ffmpeg: &Ffmpeg, path: &Path) {
    let status = std::process::Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=90x90:rate=30:d=2"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:d=2", "-shortest", "-pix_fmt", "yuv420p", "-c:a", "aac"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// Frame `index` of `path` as straight RGBA. VP9's alpha plane needs the
/// libvpx decoder (FFmpeg's own VP9 decoder drops it).
fn decode_rgba(ffmpeg: &Ffmpeg, path: &Path, index: usize, libvpx: bool) -> Vec<u8> {
    let mut command = std::process::Command::new(ffmpeg.ffmpeg_path());
    command.args(["-hide_banner", "-loglevel", "error"]);
    if libvpx {
        command.args(["-c:v", "libvpx-vp9"]);
    }
    let output = command
        .arg("-i")
        .arg(path)
        .args(["-fps_mode", "passthrough", "-pix_fmt", "rgba", "-f", "rawvideo", "-"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let frame = (WIDTH * HEIGHT * 4) as usize;
    assert!(output.stdout.len() >= frame * (index + 1), "only {} bytes", output.stdout.len());
    output.stdout[frame * index..frame * (index + 1)].to_vec()
}

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

/// Mean alpha error, and mean colour error where the frame is mostly opaque
/// enough for colour to matter.
fn errors(decoded: &[u8], rendered_bgra: &[u8]) -> (f64, f64) {
    let (mut alpha, mut colour, mut coloured) = (0u64, 0u64, 0u64);
    for (d, r) in decoded.chunks_exact(4).zip(rendered_bgra.chunks_exact(4)) {
        alpha += u64::from(d[3].abs_diff(r[3]));
        if r[3] >= 100 {
            colour += u64::from(d[0].abs_diff(r[2])) + u64::from(d[1].abs_diff(r[1])) + u64::from(d[2].abs_diff(r[0]));
            coloured += 3;
        }
    }
    (alpha as f64 / (WIDTH * HEIGHT) as f64, colour as f64 / coloured.max(1) as f64)
}

/// Mean colour error of `decoded` against `reference` over the pixels
/// that `mask_bgra` covers (alpha of at least 100).
fn colour_error_where(decoded: &[u8], reference_bgra: &[u8], mask_bgra: &[u8]) -> f64 {
    let (mut total, mut count) = (0u64, 0u64);
    for ((d, r), m) in decoded.chunks_exact(4).zip(reference_bgra.chunks_exact(4)).zip(mask_bgra.chunks_exact(4)) {
        if m[3] >= 100 {
            total += u64::from(d[0].abs_diff(r[2])) + u64::from(d[1].abs_diff(r[1])) + u64::from(d[2].abs_diff(r[0]));
            count += 3;
        }
    }
    total as f64 / count.max(1) as f64
}

#[test]
fn transparent_videos_and_pngs_keep_the_alpha() {
    let Ok(ffmpeg) = Ffmpeg::locate() else { return };
    let mut renderer = match Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => renderer,
        Err(RenderError::Gpu(error)) => return eprintln!("no GPU adapter ({error}); skipping"),
        Err(error) => panic!("{error}"),
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("square.mp4");
    square_pattern(&ffmpeg, &source);
    let mut project = Project::new("Transparent");
    project.settings.canvas = CanvasSize { width: WIDTH, height: HEIGHT };
    // A white background that a transparent export must leave out.
    project.settings.background = editor::Background::Color { color: "#ffffff".to_owned() };
    let media = MediaItem {
        id: "square".to_owned(),
        name: "square.mp4".to_owned(),
        kind: MediaKind::Video,
        path: source,
        duration: MediaTime::from_seconds_f64(2.0),
        width: Some(90),
        height: Some(90),
        fps: Some(30.0),
        has_audio: true,
    };
    apply(&mut project, &EditorCommand::AddMedia(media.clone())).unwrap();
    let mut element = Element::from_media(&media, MediaTime::ZERO);
    element.params.set("opacity", 0.5);
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();

    // The rendered frame at 1 s: the square in the middle at half alpha,
    // its full colour (not darkened by the missing background), nothing at the sides.
    let transparent = render::transparent_project(&project);
    let time = MediaTime::from_seconds_f64(1.0).unwrap();
    let rendered = renderer.render(&transparent, time, WIDTH, HEIGHT).unwrap();
    let opaque = renderer.render(&project, time, WIDTH, HEIGHT).unwrap();
    let at = |bgra: &[u8], x: u32, y: u32| {
        let i = ((y * WIDTH + x) * 4) as usize;
        [bgra[i + 2], bgra[i + 1], bgra[i], bgra[i + 3]]
    };
    assert_eq!(at(&rendered.bgra, 10, 45)[3], 0, "the side is empty");
    assert_eq!(at(&opaque.bgra, 10, 45), [255, 255, 255, 255], "the background as set is white");
    let middle = at(&rendered.bgra, 80, 45);
    assert!(middle[3].abs_diff(128) <= 1, "half alpha in the middle: {middle:?}");
    // Over white at half opacity the colour is (c + 255) / 2; the transparent pixel keeps c.
    let over_white = at(&opaque.bgra, 80, 45);
    for channel in 0..3 {
        let expected = (u32::from(middle[channel]) + 255).div_ceil(2);
        assert!(u32::from(over_white[channel]).abs_diff(expected) <= 2, "colour {middle:?} over white {over_white:?}");
    }

    // A plain VP9 WebM of the pattern at full opacity (the same colours as
    // the transparent pixels), at the same quality: how far VP9 (4:2:0,
    // lossy) moves this busy pattern's colours anyway, over the square.
    let job = |output: std::path::PathBuf, format: EncodeFormat, alpha: Option<AlphaCodec>| ExportJob {
        output,
        format,
        quality: EncodeQuality::High,
        platform: None,
        codec: None,
        video_bitrate: None,
        hardware: alpha.is_some(),
        include_audio: true,
        cache_dir: None,
        loudness: None,
        alpha,
    };
    let mut full = project.clone();
    for track in &mut full.tracks {
        for element in &mut track.elements {
            element.params.set("opacity", 1.0);
        }
    }
    let full_frame = renderer.render(&full, time, WIDTH, HEIGHT).unwrap();
    let plain = render::export_project(&mut renderer, &ffmpeg, &full, &job(dir.path().join("plain.webm"), EncodeFormat::Webm, None), &|| false, &mut |_, _| {}).unwrap();
    let plain_colour_error = colour_error_where(&decode_rgba(&ffmpeg, &plain, 30, false), &full_frame.bgra, &rendered.bgra);
    eprintln!("plain VP9 at full opacity: colour error over the square {plain_colour_error:.2}");

    // ProRes 4444 and VP9 with alpha, each decoded back. VP9's colour may
    // be no further off than the plain VP9's.
    for (codec, libvpx, alpha_limit, colour_limit) in [
        (AlphaCodec::ProRes4444, false, 1.0, 1.0),
        (AlphaCodec::Vp9, true, 3.0, plain_colour_error * 1.25 + 1.0),
    ] {
        let output = dir.path().join(format!("overlay.{}", codec.extension()));
        let job = job(output.clone(), EncodeFormat::Mp4, Some(codec));
        let mut frames = (0, 0);
        let written = render::export_project(&mut renderer, &ffmpeg, &project, &job, &|| false, &mut |done, total| frames = (done, total)).unwrap();
        assert_eq!(frames, (60, 60));
        let info = media::probe(&ffmpeg, &written).unwrap();
        assert!(info.has_video && info.has_audio, "{codec:?} keeps the sound");
        let decoded = decode_rgba(&ffmpeg, &written, 30, libvpx);
        let (alpha_error, colour_error) = errors(&decoded, &rendered.bgra);
        let side = pixel(&decoded, 10, 45);
        let centre = pixel(&decoded, 80, 45);
        eprintln!(
            "{codec:?}: {} bytes, alpha error {alpha_error:.2}, colour error {colour_error:.2}, side {side:?}, centre {centre:?}",
            std::fs::metadata(&written).unwrap().len()
        );
        assert!(side[3] <= 2, "{codec:?}: the side stays transparent: {side:?}");
        assert!(centre[3].abs_diff(128) <= 3, "{codec:?}: half alpha in the middle: {centre:?}");
        assert!(alpha_error < alpha_limit, "{codec:?}: alpha error {alpha_error}");
        assert!(colour_error < colour_limit, "{codec:?}: colour error {colour_error}");
    }

    // A transparent still keeps the alpha too.
    let still = render::export_png(&mut renderer, &transparent, time, &dir.path().join("still.png")).unwrap();
    let png = image::open(&still).unwrap().to_rgba8();
    assert_eq!(png.get_pixel(10, 45).0[3], 0);
    assert!(png.get_pixel(80, 45).0[3].abs_diff(128) <= 1);
}
