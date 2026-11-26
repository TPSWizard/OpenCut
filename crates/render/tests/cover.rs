//! Cover pictures through the GPU renderer (row 70). Skips without a GPU
//! adapter; the embed part also needs FFmpeg.

use editor::{
    Background, CanvasSize, Command as EditorCommand, CoverSettings, CoverStyle, Element, MediaItem, MediaKind, Project, TrackPlacement,
    apply,
};
use media::Ffmpeg;
use render::{ExportJob, RenderError, Renderer, RendererOptions};
use time::MediaTime;

fn renderer(ffmpeg: Option<std::sync::Arc<Ffmpeg>>) -> Option<Renderer> {
    match Renderer::new(RendererOptions { ffmpeg, fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::Gpu(error)) => {
            eprintln!("no GPU adapter ({error}); skipping");
            None
        }
        Err(error) => panic!("{error}"),
    }
}

/// A 1920x1080 project, blue background, a green square picture for 3 s.
fn project(dir: &std::path::Path) -> Project {
    let mut project = Project::new("Cover");
    project.settings.canvas = CanvasSize { width: 320, height: 180 };
    project.settings.background = Background::Color { color: "#0000ff".to_owned() };
    let path = dir.join("square.png");
    image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 255, 0, 255])).save(&path).unwrap();
    let media = MediaItem {
        id: "img".to_owned(),
        name: "square.png".to_owned(),
        kind: MediaKind::Image,
        path,
        duration: None,
        width: Some(100),
        height: Some(100),
        fps: None,
        has_audio: false,
    };
    apply(&mut project, &EditorCommand::AddMedia(media)).unwrap();
    let mut clip = Element::from_media(project.media_item("img").unwrap(), MediaTime::ZERO);
    clip.duration = MediaTime::from_seconds_f64(3.0).unwrap();
    apply(&mut project, &EditorCommand::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();
    project
}

fn count(image: &image::RgbaImage, region: (u32, u32, u32, u32), test: impl Fn([u8; 4]) -> bool) -> usize {
    let (x0, y0, x1, y1) = region;
    (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|(x, y)| test(image.get_pixel(*x, *y).0)).count()
}

#[test]
fn draws_the_title_over_the_refitted_frame() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let size = CanvasSize { width: 180, height: 320 };
    let cover = CoverSettings { time: 1.0, title: "Hello cover".to_owned(), style: CoverStyle::Band, embed: true };
    let picture = render::render_cover(&mut renderer, &project, &cover, size).unwrap();
    assert_eq!((picture.width(), picture.height()), (180, 320));
    // `OPENCUT_COVER_DUMP=dir` keeps the pictures to look at.
    if let Some(dir) = std::env::var_os("OPENCUT_COVER_DUMP") {
        picture.save(std::path::Path::new(&dir).join("cover-band-9x16.png")).unwrap();
        let bold = CoverSettings { style: CoverStyle::Bold, title: "How I made $5,000 in one month".to_owned(), ..cover.clone() };
        render::render_cover(&mut renderer, &project, &bold, CanvasSize { width: 640, height: 360 }).unwrap().save(std::path::Path::new(&dir).join("cover-bold-16x9.png")).unwrap();
        let top = CoverSettings { style: CoverStyle::Top, title: "Watch till the end".to_owned(), ..cover.clone() };
        render::render_cover(&mut renderer, &project, &top, CanvasSize { width: 360, height: 640 }).unwrap().save(std::path::Path::new(&dir).join("cover-top-9x16.png")).unwrap();
    }
    // Fill: the square covers the tall frame, so the top is green, not the blue background.
    let green = |p: [u8; 4]| p[1] > 200 && p[0] < 60 && p[2] < 60;
    assert_eq!(count(&picture, (0, 0, 180, 40), green), 180 * 40, "covered");
    // The band title sits low: a dark box with white letters.
    let band = (0, 200, 180, 320);
    let dark = count(&picture, band, |p| p[0] < 40 && p[1] < 40 && p[2] < 40);
    let white = count(&picture, band, |p| p[0] > 200 && p[1] > 200 && p[2] > 200);
    assert!(dark > 500 && white > 30, "band {dark} dark, {white} white");
    // Without a title the same area is just the picture.
    let plain = render::render_cover(&mut renderer, &project, &CoverSettings { title: String::new(), ..cover }, size).unwrap();
    assert_eq!(count(&plain, band, green), 180 * 120);
    // A suggestion lands inside the timeline.
    let time = render::suggest_cover_time(&mut renderer, &project).unwrap();
    assert!((0.0..=3.0).contains(&time), "{time}");
}

#[test]
fn mp4_exports_carry_the_cover() {
    let Ok(ffmpeg) = Ffmpeg::locate() else { return };
    let Some(mut renderer) = renderer(Some(ffmpeg.clone())) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = project(dir.path());
    project.settings.cover = Some(CoverSettings { time: 1.0, title: "Title".to_owned(), style: CoverStyle::Bold, embed: true });
    let output = dir.path().join("out.mp4");
    let job = ExportJob {
        output: output.clone(),
        format: media::EncodeFormat::Mp4,
        quality: media::EncodeQuality::Low,
        platform: None,
        codec: None,
        video_bitrate: None,
        hardware: false,
        include_audio: false,
        cache_dir: None,
        loudness: None,
        alpha: None,
    };
    render::export_project(&mut renderer, &ffmpeg, &project, &job, &|| false, &mut |_, _| {}).unwrap();
    let probe = std::process::Command::new(ffmpeg.ffmpeg_path()).args(["-hide_banner", "-i"]).arg(&output).output().unwrap();
    let text = String::from_utf8_lossy(&probe.stderr);
    assert!(text.contains("attached pic"), "{text}");
    assert!(!output.with_extension("cover.png").exists());
}
