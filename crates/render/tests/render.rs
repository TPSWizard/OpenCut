//! Renders real frames through the GPU compositor. Skips when no adapter is
//! available (headless CI); the video part also needs FFmpeg.

use std::path::Path;
use std::process::Command;

use editor::{Command as EditorCommand, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
use media::{Ffmpeg, probe};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap()
}

fn renderer(ffmpeg: Option<std::sync::Arc<Ffmpeg>>) -> Option<Renderer> {
    match Renderer::new(RendererOptions { ffmpeg, fonts_dir: None }) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::Gpu(error)) => {
            eprintln!("no GPU adapter ({error}); skipping");
            None
        }
        Err(error) => panic!("{error}"),
    }
}

fn pixel(frame: &render::RenderedFrame, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * frame.width + x) * 4) as usize;
    frame.bgra[index..index + 4].try_into().unwrap()
}

fn add_image(project: &mut Project, dir: &Path) -> String {
    let path = dir.join("square.png");
    // A solid green 100x100 image.
    let image = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 255, 0, 255]));
    image.save(&path).unwrap();
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
    apply(project, &EditorCommand::AddMedia(media)).unwrap();
    "img".to_owned()
}

#[test]
fn composites_image_and_text_over_background() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Render");
    project.settings.background = editor::Background::Color {
        color: "#0000ff".to_owned(),
    };
    let image_id = add_image(&mut project, dir.path());

    // The image fills the canvas height (contain) and is shrunk to half, so
    // it covers the middle of the frame only.
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    image.params.set("transform.scaleX", 0.5).set("transform.scaleY", 0.5);
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element: image,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();
    let mut text = Element::text("OpenCut", seconds(1.0));
    // 10 × 1080 / 90 = 120 canvas pixels tall.
    text.params
        .set("fontSize", 10.0)
        .set("color", "#ffffff")
        .set("transform.positionY", -400.0);
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element: text,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();

    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!((frame.width, frame.height), (320, 180));
    assert_eq!(pixel(&frame, 2, 2), [255, 0, 0, 255], "background is blue (BGRA)");
    assert_eq!(pixel(&frame, 160, 90), [0, 255, 0, 255], "image is green in the middle");
    assert_eq!(pixel(&frame, 160, 10), [255, 0, 0, 255], "above the shrunk image is background");

    let frame = renderer.render(&project, seconds(2.0), 320, 180).unwrap();
    let white_near_top = (0..320)
        .flat_map(|x| (0..40).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = pixel(&frame, x, y);
            p[0] > 200 && p[1] > 200 && p[2] > 200
        })
        .count();
    assert!(white_near_top > 50, "text pixels near the top: {white_near_top}");
    assert_eq!(pixel(&frame, 160, 90), [0, 255, 0, 255], "image still in the middle");

    // Half opacity blends the image with the background.
    let image_id = project.tracks[project.main_track_index()].elements[0].id.clone();
    let mut params = editor::Params::new();
    params.set("opacity", 0.5);
    apply(
        &mut project,
        &EditorCommand::UpdateElement {
            element_id: image_id,
            patch: editor::ElementPatch {
                params: Some(params),
                ..Default::default()
            },
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    let p = pixel(&frame, 160, 90);
    assert!(p[0] > 100 && p[1] > 100 && p[2] < 20, "blend of blue and green: {p:?}");
}

#[test]
fn blurred_background_covers_the_frame_with_the_main_track() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Backdrop");
    let image_id = add_image(&mut project, dir.path());
    // A small image in the middle, so the frame's edges show the background.
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    image.params.set("transform.scaleX", 0.25).set("transform.scaleY", 0.25);
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element: image,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();

    project.settings.background = editor::Background::Color {
        color: "#000000".to_owned(),
    };
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&frame, 4, 4), [0, 0, 0, 255], "plain black corner");
    assert_eq!(pixel(&frame, 160, 90), [0, 255, 0, 255], "image in the middle");

    // Blurred: the green image, scaled to cover the frame, fills the corners
    // too. A solid image stays solid however hard it is blurred.
    project.settings.background = editor::Background::Blur { blur_intensity: 200.0 };
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    let corner = pixel(&frame, 4, 4);
    assert!(corner[1] > 200 && corner[0] < 30 && corner[2] < 30, "green backdrop in the corner: {corner:?}");
    assert_eq!(pixel(&frame, 160, 90), [0, 255, 0, 255], "image still on top");

    // Nothing on the main track at this time: the backdrop is gone.
    let frame = renderer.render(&project, seconds(20.0), 320, 180).unwrap();
    assert_eq!(pixel(&frame, 4, 4), [0, 0, 0, 255], "no backdrop without a clip");
}

#[test]
fn text_masks_cut_the_element_to_the_glyphs() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Text mask");
    project.settings.background = editor::Background::Color {
        color: "#0000ff".to_owned(),
    };
    let image_id = add_image(&mut project, dir.path());
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    image.params.set("transform.scaleX", 1.0).set("transform.scaleY", 1.0);
    let mut mask = editor::Mask::shape(editor::Mask::TEXT);
    // 40 × 180 / 90 = 80 px glyphs on the 180 px element.
    mask.params.set("content", "MASK").set("fontSize", 40.0);
    image.masks.push(mask);
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element: image,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    let green = |x: u32, y: u32| {
        let p = pixel(&frame, x, y);
        p[1] > 200 && p[0] < 40 && p[2] < 40
    };
    assert!(!green(80, 10), "the element's corner is masked away");
    assert!(!green(160, 20), "above the text line");
    let glyph_pixels = (70..250).flat_map(|x| (50..130).map(move |y| (x, y))).filter(|&(x, y)| green(x, y)).count();
    assert!(glyph_pixels > 500 && glyph_pixels < 180 * 80 / 2, "glyphs show the image: {glyph_pixels}");
    assert_eq!(pixel(&frame, 2, 2), [255, 0, 0, 255], "background elsewhere");
}

#[test]
fn draws_mask_strokes_over_the_element() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Stroke");
    project.settings.background = editor::Background::Color {
        color: "#0000ff".to_owned(),
    };
    let image_id = add_image(&mut project, dir.path());
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    // A square 180 px image centred in a 320x180 frame, masked to its middle
    // 50% (45..135 across) with a thick red outline. Stroke width is in
    // canvas pixels; the 1080p canvas is six times this frame, so 48 is 8 px.
    image.params.set("transform.scaleX", 1.0).set("transform.scaleY", 1.0);
    let mut mask = editor::Mask::rectangle();
    mask.params
        .set("width", 0.5)
        .set("height", 0.5)
        .set("strokeWidth", 48.0)
        .set("strokeColor", "#ff0000");
    image.masks.push(mask);
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element: image,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&frame, 160, 90), [0, 255, 0, 255], "green inside the mask");
    assert_eq!(pixel(&frame, 160, 135 + 10), [255, 0, 0, 255], "blue outside it");
    let edge = pixel(&frame, 160, 135);
    assert!(edge[2] > 200 && edge[1] < 40 && edge[0] < 40, "red stroke on the mask edge: {edge:?}");
    let inner = pixel(&frame, 160, 135 - 3);
    assert!(inner[2] > 200 && inner[1] < 40, "the band is 8 px wide: {inner:?}");
}

#[test]
fn applies_masks_blur_and_keyframes() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Masked");
    project.settings.background = editor::Background::Color {
        color: "#0000ff".to_owned(),
    };
    let image_id = add_image(&mut project, dir.path());
    // The square image fills the canvas height (contain), so at 320x180 it
    // covers x in 70..250.
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    let mut mask = editor::Mask::ellipse();
    mask.params.set("width", 0.5).set("height", 0.5);
    image.masks.push(mask);
    let element_id = image.id.clone();
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element: image,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();

    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&frame, 160, 90), [0, 255, 0, 255], "inside the ellipse");
    assert_eq!(pixel(&frame, 78, 8), [255, 0, 0, 255], "image corner is masked away");
    assert_eq!(pixel(&frame, 2, 2), [255, 0, 0, 255], "background");

    // Inverting shows the corner and hides the middle.
    let mask_id = project.element(&element_id).unwrap().1.masks[0].id.clone();
    let mut inverted = editor::Params::new();
    inverted.set("inverted", true);
    apply(
        &mut project,
        &EditorCommand::UpdateMask {
            element_id: element_id.clone(),
            mask_id: mask_id.clone(),
            params: inverted,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&frame, 160, 90), [255, 0, 0, 255], "middle now shows the background");
    assert_eq!(pixel(&frame, 78, 8), [0, 255, 0, 255], "corner now shows the image");

    // A feathered edge blends.
    let mut feathered = editor::Params::new();
    feathered.set("inverted", false).set("feather", 120.0);
    apply(
        &mut project,
        &EditorCommand::UpdateMask {
            element_id: element_id.clone(),
            mask_id,
            params: feathered,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    let p = pixel(&frame, 160, 90 - 44);
    assert!(p[0] > 20 && p[1] > 20, "feathered edge mixes image and background: {p:?}");

    // Blur softens the image edge: without it the pixel just outside the
    // square is pure background, with it some green bleeds over.
    let mask_id = project.element(&element_id).unwrap().1.masks[0].id.clone();
    apply(
        &mut project,
        &EditorCommand::RemoveMask {
            element_id: element_id.clone(),
            mask_id,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&frame, 69, 90), [255, 0, 0, 255], "sharp edge without blur");
    let mut blur = editor::Effect::blur();
    blur.params.set("intensity", 100.0);
    apply(
        &mut project,
        &EditorCommand::AddEffect {
            element_id: element_id.clone(),
            effect: blur,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    let p = pixel(&frame, 69, 90);
    assert!(p[1] > 15 && p[0] < 250, "blurred edge bleeds green: {p:?}");

    // Keyframed opacity fades the image in over the first two seconds.
    for (time, value) in [(0.0, 0.0), (2.0, 1.0)] {
        apply(
            &mut project,
            &EditorCommand::SetKeyframe {
                element_id: element_id.clone(),
                path: "opacity".to_owned(),
                time: seconds(time),
                value,
                segment: None,
            },
        )
        .unwrap();
    }
    let start = pixel(&renderer.render(&project, seconds(0.0), 320, 180).unwrap(), 160, 90);
    let middle = pixel(&renderer.render(&project, seconds(1.0), 320, 180).unwrap(), 160, 90);
    let end = pixel(&renderer.render(&project, seconds(2.0), 320, 180).unwrap(), 160, 90);
    assert_eq!(start, [255, 0, 0, 255], "invisible at the first key");
    assert!(middle[1] > 90 && middle[1] < 170 && middle[0] > 90, "half way: {middle:?}");
    assert!(end[1] > 240 && end[0] < 15, "fully visible at the last key: {end:?}");
}

#[test]
fn renders_video_frames_through_ffmpeg() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let Some(mut renderer) = renderer(Some(ffmpeg.clone())) else { return };
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("red.mp4");
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=64x64:rate=30:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip)
        .status()
        .unwrap();
    assert!(status.success());
    let info = probe(&ffmpeg, &clip).unwrap();

    let mut project = Project::new("Video");
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "red".to_owned(),
            name: "red.mp4".to_owned(),
            kind: MediaKind::Video,
            path: clip,
            duration: info.duration,
            width: info.width,
            height: info.height,
            fps: info.fps,
            has_audio: false,
        }),
    )
    .unwrap();
    let element = Element::from_media(project.media_item("red").unwrap(), seconds(0.0));
    apply(
        &mut project,
        &EditorCommand::InsertElement {
            element,
            placement: TrackPlacement::Auto,
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(1.0), 192, 108).unwrap();
    let p = pixel(&frame, 96, 54);
    assert!(p[2] > 200 && p[1] < 40 && p[0] < 40, "video frame is red: {p:?}");
    let edge = pixel(&frame, 2, 54);
    assert_eq!(edge, [0, 0, 0, 255], "letterboxed sides show the black background");
}
