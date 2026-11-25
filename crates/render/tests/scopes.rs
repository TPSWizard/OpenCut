//! Video scopes of a real preview frame, as the desktop draws them: the
//! renderer's BGRA frame through the shared `scopes` crate. Skips when no
//! GPU adapter is available.

use editor::{Command as EditorCommand, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

const BARS: [[u8; 3]; 7] = [[191, 191, 191], [191, 191, 0], [0, 191, 191], [0, 191, 0], [191, 0, 191], [191, 0, 0], [0, 0, 191]];

#[test]
fn colour_bars_land_on_the_vectorscope_targets() {
    let renderer = match Renderer::new(RendererOptions { ffmpeg: None, fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => renderer,
        Err(RenderError::Gpu(error)) => {
            eprintln!("no GPU adapter ({error}); skipping");
            return;
        }
        Err(error) => panic!("{error}"),
    };
    let mut renderer = renderer;
    // The web check's picture: 75% bars over the top two thirds, a black to
    // white ramp under them.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bars.png");
    let picture = image::RgbaImage::from_fn(1280, 720, |x, y| {
        if y < 480 {
            let [r, g, b] = BARS[(x as usize * 7 / 1280).min(6)];
            image::Rgba([r, g, b, 255])
        } else {
            let level = (x as f64 / 1279.0 * 255.0).round() as u8;
            image::Rgba([level, level, level, 255])
        }
    });
    picture.save(&path).unwrap();
    let mut project = Project::new("Scopes");
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "bars".to_owned(),
            name: "bars.png".to_owned(),
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
    let element = Element::from_media(project.media_item("bars").unwrap(), MediaTime::ZERO);
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    let frame = renderer.render(&project, MediaTime::from_seconds_f64(1.0).unwrap(), 640, 360).unwrap();

    let scope = scopes::draw(scopes::Scope::Vectorscope, &frame.bgra, frame.width as usize, frame.height as usize, scopes::PixelOrder::Bgra);
    let lit = |x: usize, y: usize| scope.rgba[(y * scope.width + x) * 4 + 1];
    for [r, g, b] in BARS {
        let (x, y) = scopes::vector_position(r, g, b);
        assert_eq!(lit(x, y), 255, "{r},{g},{b} lights its target at {x},{y}");
    }
    let stats = scope.stats;
    eprintln!(
        "samples {} black level {} white level {} at 0 {:.4} at 255 {:.4}",
        stats.samples, stats.black_level, stats.white_level, stats.at_black, stats.at_white
    );
    // Bars cover two thirds; four of the seven have a channel at 0, and the
    // ramp's darkest pixels add a few more.
    assert!(stats.at_black > 0.5 && stats.at_black < 0.62, "{}", stats.at_black);

    // The waveform puts each bar's luma where Rec. 709 says.
    let wave = scopes::draw(scopes::Scope::Waveform, &frame.bgra, frame.width as usize, frame.height as usize, scopes::PixelOrder::Bgra);
    for (index, [r, g, b]) in BARS.into_iter().enumerate() {
        let column = (index * 2 + 1) * wave.width / 14;
        let row = 255 - scopes::luma(r, g, b) as usize;
        assert!(wave.rgba[(row * wave.width + column) * 4 + 1] > 100, "bar {index} at row {row}");
    }
}
