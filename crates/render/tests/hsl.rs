//! The HSL effect through the desktop's real renderer (catalogue params to
//! shader uniforms): taking the saturation out of blue turns the blue bar
//! grey and leaves the other bars alone. The web editor reads the same
//! frame identically (`preview.scope`: 66.7% -> 58.3% with a channel at 0).

use editor::{Command as EditorCommand, Effect, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

const HUES: [f64; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];

fn hue_rgb(hue: f64) -> [u8; 3] {
    let hp = hue / 60.0;
    let x = 1.0 - (hp % 2.0 - 1.0).abs();
    let (r, g, b) = match hp as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * 255.0f64).round() as u8, (g * 255.0f64).round() as u8, (b * 255.0f64).round() as u8]
}

#[test]
fn hsl_greys_out_only_the_blue_bar() {
    let mut renderer = match Renderer::new(RendererOptions { ffmpeg: None, fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => renderer,
        Err(RenderError::Gpu(error)) => {
            eprintln!("no GPU adapter ({error}); skipping");
            return;
        }
        Err(error) => panic!("{error}"),
    };
    // The web check's picture: eight pure hue bars over the top two thirds,
    // a skin tone and mid grey under them.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hues.png");
    image::RgbaImage::from_fn(1280, 720, |x, y| {
        let [r, g, b] = if y < 480 {
            hue_rgb(HUES[(x / 160) as usize])
        } else if x < 640 {
            [224, 172, 140]
        } else {
            [128, 128, 128]
        };
        image::Rgba([r, g, b, 255])
    })
    .save(&path)
    .unwrap();
    let mut project = Project::new("HSL");
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "hues".to_owned(),
            name: "hues.png".to_owned(),
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
    let mut element = Element::from_media(project.media_item("hues").unwrap(), MediaTime::ZERO);
    let mut effect = Effect::from_catalogue("hsl").expect("HSL is in the catalogue");
    effect.params.set("blueSaturation", -100.0);
    element.effects.push(effect);
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    let frame = renderer.render(&project, MediaTime::from_seconds_f64(1.0).unwrap(), 640, 360).unwrap();
    let pixel = |x: u32, y: u32| {
        let index = ((y * frame.width + x) * 4) as usize;
        let p = &frame.bgra[index..index + 4];
        [p[2], p[1], p[0]]
    };
    for (bar, hue) in HUES.iter().enumerate() {
        let got = pixel(bar as u32 * 80 + 40, 120);
        let want = if *hue == 240.0 { [128, 128, 128] } else { hue_rgb(*hue) };
        for channel in 0..3 {
            assert!((got[channel] as i32 - want[channel] as i32).abs() <= 2, "bar {hue}: {got:?} vs {want:?}");
        }
    }
    let stats = scopes::draw(scopes::Scope::Histogram, &frame.bgra, frame.width as usize, frame.height as usize, scopes::PixelOrder::Bgra).stats;
    eprintln!("at 0 {:.4} at 255 {:.4} black {} white {}", stats.at_black, stats.at_white, stats.black_level, stats.white_level);
    assert!((stats.at_black - 7.0 / 12.0).abs() < 1e-9, "{}", stats.at_black);
}
