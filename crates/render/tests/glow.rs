//! Bloom and light leak through the desktop's real renderer (catalogue
//! params to shader passes). The web editor reads the same Bloom frame
//! identically: `preview.capture` centre 18 (11 without), `preview.scope`
//! 87.92% with a channel at 0 (91.67% without).

use editor::{Command as EditorCommand, Effect, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

fn renderer() -> Option<Renderer> {
    match Renderer::new(RendererOptions { ffmpeg: None, fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::Gpu(error)) => {
            eprintln!("no GPU adapter ({error}); skipping");
            None
        }
        Err(error) => panic!("{error}"),
    }
}

/// Black, a white stripe at x 560..600 and a mid grey patch bottom left
/// (1280 x 720), as the web check's picture; with `effect` on the clip.
fn project_with(dir: &std::path::Path, effect: Option<Effect>) -> Project {
    let path = dir.join("stripe.png");
    image::RgbaImage::from_fn(1280, 720, |x, y| {
        let v = if (560..600).contains(&x) {
            255
        } else if x < 400 && y >= 600 {
            100
        } else {
            0
        };
        image::Rgba([v, v, v, 255])
    })
    .save(&path)
    .unwrap();
    let mut project = Project::new("Glow");
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "stripe".to_owned(),
            name: "stripe.png".to_owned(),
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
    let mut element = Element::from_media(project.media_item("stripe").unwrap(), MediaTime::ZERO);
    element.effects.extend(effect);
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    project
}

/// The web API's `preview.capture` centre: the mean of a centred box an
/// eighth of the shorter side, rounded.
fn centre(frame: &render::RenderedFrame) -> u32 {
    let side = ((frame.width.min(frame.height) as f64 / 8.0).round() as u32).max(2);
    let x0 = (frame.width as f64 / 2.0 - side as f64 / 2.0 + 0.5).floor() as u32;
    let y0 = (frame.height as f64 / 2.0 - side as f64 / 2.0 + 0.5).floor() as u32;
    let mut sum = 0u64;
    for y in y0..y0 + side {
        for x in x0..x0 + side {
            sum += frame.bgra[((y * frame.width + x) * 4 + 1) as usize] as u64;
        }
    }
    (sum as f64 / (side * side) as f64).round() as u32
}

#[test]
fn bloom_glows_around_the_stripe_as_on_the_web() {
    let Some(mut renderer) = renderer() else { return };
    let dir = tempfile::tempdir().unwrap();
    let at = MediaTime::from_seconds_f64(1.0).unwrap();
    let plain = renderer.render(&project_with(dir.path(), None), at, 640, 360).unwrap();
    let bloom = renderer.render(&project_with(dir.path(), Effect::from_catalogue("bloom")), at, 640, 360).unwrap();
    let share = |frame: &render::RenderedFrame| {
        scopes::draw(scopes::Scope::Histogram, &frame.bgra, frame.width as usize, frame.height as usize, scopes::PixelOrder::Bgra).stats.at_black
    };
    eprintln!("centre {} -> {}, at 0 {:.4} -> {:.4}", centre(&plain), centre(&bloom), share(&plain), share(&bloom));
    assert_eq!(centre(&plain), 11);
    assert_eq!(centre(&bloom), 18);
    assert!((share(&plain) - 0.916_666_666_666_666_6).abs() < 1e-9);
    assert!((share(&bloom) - 0.879_166_666_666_666_7).abs() < 1e-9);
    // The grey patch is under the threshold: unchanged.
    let grey = |frame: &render::RenderedFrame| frame.bgra[((340 * frame.width + 40) * 4) as usize];
    assert_eq!(grey(&bloom), grey(&plain));
}

#[test]
fn light_leak_warms_the_frame_and_moves_with_time() {
    let Some(mut renderer) = renderer() else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut leak = Effect::from_catalogue("light-leak").unwrap();
    leak.params.set("intensity", 1.0);
    leak.params.set("speed", 2.0);
    let project = project_with(dir.path(), Some(leak));
    let early = renderer.render(&project, MediaTime::from_seconds_f64(0.5).unwrap(), 320, 180).unwrap();
    let late = renderer.render(&project, MediaTime::from_seconds_f64(3.0).unwrap(), 320, 180).unwrap();
    // Warm light on black, from the right (30°): red over blue there.
    let pixel = |frame: &render::RenderedFrame, x: u32, y: u32| {
        let index = ((y * frame.width + x) * 4) as usize;
        [frame.bgra[index + 2], frame.bgra[index + 1], frame.bgra[index]]
    };
    let right = pixel(&early, 300, 60);
    eprintln!("right edge {right:?}, left edge {:?}", pixel(&early, 10, 120));
    assert!(right[0] > 120 && right[0] > right[2] + 60, "warm light near the right edge: {right:?}");
    assert!(pixel(&early, 10, 120)[0] < right[0], "fainter on the far side");
    assert_ne!(early.bgra, late.bgra, "it drifts over time");
}

/// The web editor's reading of the same leak (defaults, as an effect layer
/// 2.5 s in): 40.53125% of pixels with a channel at 0, 5.9375% at 255.
#[test]
fn light_leak_reads_as_on_the_web() {
    let Some(mut renderer) = renderer() else { return };
    let dir = tempfile::tempdir().unwrap();
    let project = project_with(dir.path(), Effect::from_catalogue("light-leak"));
    let frame = renderer.render(&project, MediaTime::from_seconds_f64(2.5).unwrap(), 640, 360).unwrap();
    let stats = scopes::draw(scopes::Scope::Vectorscope, &frame.bgra, frame.width as usize, frame.height as usize, scopes::PixelOrder::Bgra).stats;
    eprintln!("leak at 0 {:.6} at 255 {:.6}", stats.at_black, stats.at_white);
    assert!((stats.at_black - 0.4053125).abs() < 0.002, "{}", stats.at_black);
    assert!((stats.at_white - 0.059375).abs() < 0.002, "{}", stats.at_white);
}
