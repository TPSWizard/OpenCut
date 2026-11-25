//! A freehand drawing through the desktop's real renderer: the strokes the
//! web editor made with Draw (a red marker line and a yellow highlighter),
//! over a dark clip, read the same on both sides (`preview.scope`
//! histogram on a 640 x 360 canvas: luma 1% 32, 99% 109, 0.7135% of pixels
//! with a channel at 255; drawn on over 2 s, 0.5 s in: 99% at 81 on the web, 80 here).

use editor::{Command as EditorCommand, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

const STROKES: &str = r##"{"strokes":[{"tool":"marker","color":"#ff3b30","width":0.012,"points":[[0.1336,0.7018,0.5,0.0],[0.5105,0.5577,0.5,0.0013],[0.8874,0.4136,0.5,0.0068]]},{"tool":"highlighter","color":"#ffeb3b","width":0.035,"points":[[0.2,0.3,0.5,0.1568],[0.5,0.25,0.5,0.4568],[0.8,0.6,0.5,0.7568]]}]}"##;

#[test]
fn a_drawing_renders_as_on_the_web() {
    let mut renderer = match Renderer::new(RendererOptions { ffmpeg: None, fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => renderer,
        Err(RenderError::Gpu(error)) => {
            eprintln!("no GPU adapter ({error}); skipping");
            return;
        }
        Err(error) => panic!("{error}"),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dark.png");
    image::RgbaImage::from_pixel(1280, 720, image::Rgba([0x20, 0x20, 0x20, 255])).save(&path).unwrap();
    let mut project = Project::new("Drawing");
    project.settings.canvas = editor::CanvasSize { width: 640, height: 360 };
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "dark".to_owned(),
            name: "dark.png".to_owned(),
            kind: MediaKind::Image,
            path,
            duration: None,
            width: Some(1280),
            height: Some(720),
            fps: None,
            has_audio: false,
        }),
    )
    .unwrap();
    let clip = Element::from_media(project.media_item("dark").unwrap(), MediaTime::ZERO);
    apply(&mut project, &EditorCommand::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();
    let drawing = Element::drawing(STROKES, MediaTime::from_seconds_f64(1.0).unwrap());
    let drawing_id = drawing.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element: drawing, placement: TrackPlacement::Auto }).unwrap();

    let stats = |renderer: &mut Renderer, project: &Project, seconds: f64| {
        let frame = renderer.render(project, MediaTime::from_seconds_f64(seconds).unwrap(), 640, 360).unwrap();
        scopes::draw(scopes::Scope::Histogram, &frame.bgra, frame.width as usize, frame.height as usize, scopes::PixelOrder::Bgra).stats
    };
    let full = stats(&mut renderer, &project, 2.0);
    eprintln!("full: black {} white {} at 255 {:.6}", full.black_level, full.white_level, full.at_white);
    assert_eq!((full.black_level, full.white_level), (32, 109));
    assert!((full.at_white - 0.007135416666666667).abs() < 0.0005, "{}", full.at_white);

    // Drawn on over 2 s: half a second in, the highlighter is under way.
    let mut params = editor::Params::new();
    params.set("drawOn", 2.0);
    apply(&mut project, &EditorCommand::UpdateElement { element_id: drawing_id, patch: editor::ElementPatch { params: Some(params), ..Default::default() } })
        .unwrap();
    let half = stats(&mut renderer, &project, 1.5);
    eprintln!("half: white {}", half.white_level);
    // The web reads 81: within a level (the reveal point lands a hair apart).
    assert!((half.white_level as i32 - 81).abs() <= 1, "{}", half.white_level);
}
