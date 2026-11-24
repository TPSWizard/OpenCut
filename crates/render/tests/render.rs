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
    match Renderer::new(RendererOptions { ffmpeg, fonts_dir: None, stickers_dir: None, proxy_dir: None }) {
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

fn lavfi_clip(ffmpeg: &Ffmpeg, path: &Path, source: &str) {
    let status = Command::new(ffmpeg.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", source, "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn preview_decodes_proxies_and_export_reads_originals() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let proxies = dir.path().join("proxies");
    let clip = dir.path().join("big.mp4");
    lavfi_clip(&ffmpeg, &clip, "color=c=red:size=2560x1440:rate=30:duration=1");
    let info = probe(&ffmpeg, &clip).unwrap();

    // A real proxy has the right size and the source's length.
    let made = proxies.join("made.mp4");
    let size = media::make_proxy(&ffmpeg, &clip, &made, (2560, 1440)).unwrap();
    assert_eq!(size, (960, 540));
    let made_info = probe(&ffmpeg, &made).unwrap();
    assert_eq!((made_info.width, made_info.height), (Some(960), Some(540)));
    assert!(!made_info.has_audio);
    let lengths = (made_info.duration.unwrap().to_seconds_f64(), info.duration.unwrap().to_seconds_f64());
    assert!((lengths.0 - lengths.1).abs() < 0.1, "{lengths:?}");

    // Stand a blue clip in as the proxy so the frame tells which file was read.
    let proxy = media::proxy_path(&proxies, &clip).unwrap();
    lavfi_clip(&ffmpeg, &proxy, "color=c=blue:size=960x540:rate=30:duration=1");

    let mut project = Project::new("Proxy");
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "big".to_owned(),
            name: "big.mp4".to_owned(),
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
    let element = Element::from_media(project.media_item("big").unwrap(), seconds(0.0));
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();

    let Ok(mut preview) = Renderer::new(RendererOptions {
        ffmpeg: Some(ffmpeg.clone()),
        fonts_dir: None,
        stickers_dir: None,
        proxy_dir: Some(proxies.clone()),
    }) else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let frame = preview.render(&project, seconds(0.5), 192, 108).unwrap();
    let p = pixel(&frame, 96, 54);
    assert!(p[0] > 200 && p[2] < 40, "preview reads the proxy (blue): {p:?}");

    // Turning proxies off goes back to the original.
    preview.set_proxy_dir(None);
    let frame = preview.render(&project, seconds(0.5), 192, 108).unwrap();
    let p = pixel(&frame, 96, 54);
    assert!(p[2] > 200 && p[0] < 40, "without proxies the original (red) is read: {p:?}");

    let Some(mut export) = renderer(Some(ffmpeg.clone())) else { return };
    let frame = export.render(&project, seconds(0.5), 192, 108).unwrap();
    let p = pixel(&frame, 96, 54);
    assert!(p[2] > 200 && p[0] < 40, "a renderer without a proxy folder reads the original: {p:?}");
}

/// A cut-out shows the clip only where its matte is white: a red clip with
/// a matte whose left half is white keeps the left half; inverted, the right.
#[test]
fn cutout_mattes_hide_the_background() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let Some(mut renderer) = renderer(Some(ffmpeg.clone())) else { return };
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("red.mp4");
    lavfi_clip(&ffmpeg, &clip, "color=c=red:size=128x72:rate=30:duration=2");
    let matte = dir.path().join("matte.mp4");
    lavfi_clip(
        &ffmpeg,
        &matte,
        "color=c=black:size=128x72:rate=15:duration=2,drawbox=x=0:y=0:w=64:h=72:color=white:t=fill",
    );
    let mut project = Project::new("Cut-out");
    for (id, path) in [("red", &clip), ("matte", &matte)] {
        let info = probe(&ffmpeg, path).unwrap();
        apply(
            &mut project,
            &EditorCommand::AddMedia(MediaItem {
                id: id.to_owned(),
                name: format!("{id}.mp4"),
                kind: MediaKind::Video,
                path: path.clone(),
                duration: info.duration,
                width: info.width,
                height: info.height,
                fps: info.fps,
                has_audio: false,
            }),
        )
        .unwrap();
    }
    let mut element = Element::from_media(project.media_item("red").unwrap(), seconds(0.0));
    element.cutout = Some(editor::Cutout { matte_id: "matte".to_owned(), offset: 0.0, inverted: false });
    let element_id = element.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    // The canvas is 16:9 like the clip, so it fills the frame.
    let frame = renderer.render(&project, seconds(1.0), 192, 108).unwrap();
    let left = pixel(&frame, 40, 54);
    let right = pixel(&frame, 150, 54);
    assert!(left[2] > 200 && left[1] < 40, "left half kept: {left:?}");
    assert!(right[2] < 30 && right[1] < 30 && right[0] < 30, "right half hidden: {right:?}");

    apply(
        &mut project,
        &EditorCommand::UpdateElement {
            element_id,
            patch: editor::ElementPatch {
                cutout: Some(Some(editor::Cutout { matte_id: "matte".to_owned(), offset: 0.0, inverted: true })),
                ..Default::default()
            },
        },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(1.0), 192, 108).unwrap();
    let (left, right) = (pixel(&frame, 40, 54), pixel(&frame, 150, 54));
    assert!(left[2] < 30, "inverted hides the left: {left:?}");
    assert!(right[2] > 200, "inverted keeps the right: {right:?}");
}

/// A clip's own start transition fades it in from nothing, rising steadily
/// to full. Known: a fade to or from an empty side currently follows
/// progress squared (the transition mixes straight-alpha colours, so the
/// empty side's black darkens the picture as well as fading it); the end
/// fade out to nothing does the same.
#[test]
fn start_transitions_fade_in_from_nothing() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let Some(mut renderer) = renderer(Some(ffmpeg.clone())) else { return };
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("red.mp4");
    lavfi_clip(&ffmpeg, &clip, "color=c=red:size=128x72:rate=30:duration=3");
    let info = probe(&ffmpeg, &clip).unwrap();
    let mut project = Project::new("Start fade");
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
    let element_id = element.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    apply(
        &mut project,
        &EditorCommand::SetTransitionIn {
            element_id,
            transition: Some(editor::Transition {
                kind: "Fade".to_owned(),
                duration: seconds(1.0),
                params: editor::Params::new(),
            }),
        },
    )
    .unwrap();
    let red_at = |renderer: &mut Renderer, project: &Project, time: f64| pixel(&renderer.render(project, seconds(time), 192, 108).unwrap(), 96, 54)[2];
    let half = red_at(&mut renderer, &project, 0.5);
    let full = red_at(&mut renderer, &project, 1.5);
    let quarter = red_at(&mut renderer, &project, 0.25);
    let three = red_at(&mut renderer, &project, 0.75);
    let id = project.tracks.iter().flat_map(|t| t.elements.iter()).next().unwrap().id.clone();
    apply(&mut project, &EditorCommand::SetTransitionIn { element_id: id.clone(), transition: None }).unwrap();
    apply(&mut project, &EditorCommand::SetTransition { element_id: id, transition: Some(editor::Transition { kind: "Fade".to_owned(), duration: seconds(1.0), params: editor::Params::new() }) }).unwrap();
    let out_half = red_at(&mut renderer, &project, 2.5);
    assert!(full > 220, "after the fade: {full}");
    assert!(quarter < half && half < three && three < full, "rises: {quarter} {half} {three} {full}");
    assert!(quarter < 40, "starts from nothing: {quarter}");
    assert_eq!(half, out_half, "the start fade mirrors the end fade");
}

/// The shared Lottie fixture through the whole desktop pipeline: fitted to
/// the canvas, animated by the clip's time, looping past its 2 seconds.
#[test]
fn draws_lottie_frames_by_clip_time() {
    let Some(mut renderer) = renderer(None) else { return };
    let mut project = Project::new("Lottie");
    project.settings.background = editor::Background::Color { color: "#000000".to_owned() };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lottie/bounce.json");
    let media = MediaItem {
        id: "anim".to_owned(),
        name: "bounce.json".to_owned(),
        kind: MediaKind::Lottie,
        path,
        duration: Some(seconds(2.0)),
        width: Some(200),
        height: Some(200),
        fps: Some(30.0),
        has_audio: false,
    };
    apply(&mut project, &EditorCommand::AddMedia(media)).unwrap();
    let mut clip = Element::from_media(project.media_item("anim").unwrap(), seconds(0.0));
    clip.duration = seconds(4.0);
    apply(&mut project, &EditorCommand::InsertElement { element: clip, placement: TrackPlacement::Auto }).unwrap();

    // 200 × 200 fitted into 320 × 180 is 180 × 180 from x = 70: Lottie
    // point (x, y) lands at (70 + 0.9x, 0.9y).
    let at = |x: f64, y: f64| ((70.0 + 0.9 * x).round() as u32, (0.9 * y).round() as u32);
    let near = |got: [u8; 4], want: [u8; 4]| got.iter().zip(want).all(|(&g, w)| (g as i32 - w as i32).abs() <= 40);
    let (red, blue, black) = ([0, 0, 255, 255], [255, 0, 0, 255], [0, 0, 0, 255]);

    let frame = renderer.render(&project, seconds(0.0), 320, 180).unwrap();
    let (x, y) = at(100.0, 100.0);
    assert!(near(pixel(&frame, x, y), red), "square centre: {:?}", pixel(&frame, x, y));
    let (x, y) = at(30.0, 30.0);
    assert!(near(pixel(&frame, x, y), blue), "dot at its start: {:?}", pixel(&frame, x, y));
    assert!(near(pixel(&frame, 10, 90), black), "outside the animation is background");

    for time in [1.0, 3.0] {
        let frame = renderer.render(&project, seconds(time), 320, 180).unwrap();
        let (x, y) = at(30.0, 30.0);
        assert!(near(pixel(&frame, x, y), black), "{time}s: the dot has left: {:?}", pixel(&frame, x, y));
        let (x, y) = at(100.0, 30.0);
        assert!(near(pixel(&frame, x, y), blue), "{time}s: dot halfway: {:?}", pixel(&frame, x, y));
    }
}

/// Chroma key: a picture of green screen (two shades, as uneven lighting
/// gives), a greenish-grey fringe and red. The greens turn transparent
/// over the blue background, red stays, and spill removal takes the green
/// cast out of the fringe without hiding it.
#[test]
fn chroma_key_hides_the_screen_and_removes_spill() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.png");
    let image = image::RgbaImage::from_fn(160, 90, |x, _| match x {
        0..40 => image::Rgba([0, 177, 64, 255]),
        40..80 => image::Rgba([20, 140, 60, 255]),
        80..120 => image::Rgba([120, 170, 120, 255]),
        _ => image::Rgba([220, 30, 30, 255]),
    });
    image.save(&path).unwrap();
    let mut project = Project::new("Chroma key");
    project.settings.background = editor::Background::Color { color: "#0000ff".to_owned() };
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "screen".to_owned(),
            name: "screen.png".to_owned(),
            kind: MediaKind::Image,
            path,
            duration: None,
            width: Some(160),
            height: Some(90),
            fps: None,
            has_audio: false,
        }),
    )
    .unwrap();
    let element = Element::from_media(project.media_item("screen").unwrap(), seconds(0.0));
    let element_id = element.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    let before = renderer.render(&project, seconds(0.5), 160, 90).unwrap();
    let fringe_before = pixel(&before, 100, 45);
    apply(
        &mut project,
        &EditorCommand::AddEffect { element_id, effect: editor::Effect::from_catalogue("chroma-key").unwrap() },
    )
    .unwrap();
    let frame = renderer.render(&project, seconds(0.5), 160, 90).unwrap();
    // BGRA.
    for x in [20, 60] {
        let p = pixel(&frame, x, 45);
        assert!(p[0] > 240 && p[1] < 20 && p[2] < 20, "green at x={x} shows the blue background: {p:?}");
    }
    let red = pixel(&frame, 140, 45);
    assert!(red[2] > 200 && red[0] < 50, "red kept: {red:?}");
    let fringe = pixel(&frame, 100, 45);
    assert!(fringe[0] < 160, "fringe kept, not keyed: {fringe:?}");
    let cast = |p: [u8; 4]| p[1] as i32 - (p[2] as i32 + p[0] as i32) / 2;
    // The default removes half the cast towards the key.
    assert!(cast(fringe) * 10 < cast(fringe_before) * 7, "spill removed: {fringe_before:?} -> {fringe:?}");
}

#[test]
fn corner_pin_draws_the_clip_in_perspective_and_follows_keyframes() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Corner pin");
    project.settings.background = editor::Background::Color { color: "#0000ff".to_owned() };
    let image_id = add_image(&mut project, dir.path());
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    image.duration = seconds(4.0);
    // A trapezoid: the top corners pulled in and down.
    image
        .params
        .set(editor::CORNER_PIN_ENABLED, true)
        .set("cornerPin.topLeftX", 25.0)
        .set("cornerPin.topLeftY", 10.0)
        .set("cornerPin.topRightX", 75.0)
        .set("cornerPin.topRightY", 10.0);
    let element_id = image.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element: image, placement: TrackPlacement::Auto }).unwrap();
    // The top left corner glides out to the box's corner over two seconds.
    for (time, value) in [(0.0, 25.0), (2.0, 0.0)] {
        apply(
            &mut project,
            &EditorCommand::SetKeyframe {
                element_id: element_id.clone(),
                path: "cornerPin.topLeftX".to_owned(),
                time: seconds(time),
                value,
                segment: None,
            },
        )
        .unwrap();
    }

    // The 100×100 picture is contained in the 16:9 canvas: at 320×180 its
    // box is x 70..250, y 0..180. A box point to a frame pixel:
    let at = |frame: &render::RenderedFrame, bx: f64, by: f64| {
        pixel(frame, (70.0 + bx * 180.0).round() as u32, (by * 180.0).round().min(179.0) as u32)
    };
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [255, 0, 0, 255];
    let start = renderer.render(&project, seconds(0.0), 320, 180).unwrap();
    assert_eq!(at(&start, 0.5, 0.5), GREEN, "the middle is drawn");
    assert_eq!(at(&start, 0.5, 0.95), GREEN, "the bottom edge keeps the box's width");
    assert_eq!(at(&start, 0.03, 0.95), GREEN, "bottom left is still pinned to the box");
    assert_eq!(at(&start, 0.1, 0.05), BLUE, "above the pinned top edge is background");
    assert_eq!(at(&start, 0.1, 0.2), BLUE, "outside the slanted left edge is background");
    assert_eq!(at(&start, 0.9, 0.2), BLUE, "outside the slanted right edge is background");

    // Two seconds in, the top left corner is at the box's left edge.
    let later = renderer.render(&project, seconds(2.0), 320, 180).unwrap();
    assert_eq!(at(&later, 0.05, 0.2), GREEN, "the keyframed corner moved out");
    assert_eq!(at(&later, 0.9, 0.2), BLUE, "the right side is unchanged");

    // Switched off, the picture fills its box again.
    let mut params = editor::Params::new();
    params.set(editor::CORNER_PIN_ENABLED, false);
    apply(
        &mut project,
        &EditorCommand::UpdateElement {
            element_id,
            patch: editor::ElementPatch { params: Some(params), ..Default::default() },
        },
    )
    .unwrap();
    let off = renderer.render(&project, seconds(0.0), 320, 180).unwrap();
    assert_eq!(at(&off, 0.1, 0.05), GREEN, "no pin, no gap");
}

#[test]
fn motion_blur_smears_a_moving_clip_along_its_path_only() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Motion blur");
    project.settings.background = editor::Background::Color { color: "#0000ff".to_owned() };
    let image_id = add_image(&mut project, dir.path());
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    image.duration = seconds(2.0);
    image.params.set("transform.scaleX", 0.2).set("transform.scaleY", 0.2);
    let element_id = image.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element: image, placement: TrackPlacement::Auto }).unwrap();
    // 1800 canvas px a second, left to right through the middle.
    for (time, value) in [(0.0, -900.0), (1.0, 900.0)] {
        apply(
            &mut project,
            &EditorCommand::SetKeyframe {
                element_id: element_id.clone(),
                path: "transform.positionX".to_owned(),
                time: seconds(time),
                value,
                segment: None,
            },
        )
        .unwrap();
    }
    let set_blur = |project: &mut Project, amount: f64| {
        let mut params = editor::Params::new();
        params.set("motionBlur", amount);
        apply(
            project,
            &EditorCommand::UpdateElement {
                element_id: element_id.clone(),
                patch: editor::ElementPatch { params: Some(params), ..Default::default() },
            },
        )
        .unwrap();
    };

    // At 320x180 the 216-px box is 36 px wide, x 142..178, y 72..108.
    let sharp = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&sharp, 139, 90), [255, 0, 0, 255], "background left of the clip");
    assert_eq!(pixel(&sharp, 144, 90), [0, 255, 0, 255], "inside the clip");

    // A whole-frame shutter at 30 fps: 60 canvas px of travel, 10 px here,
    // so each side edge spreads over x 137..147 and 173..183.
    set_blur(&mut project, 100.0);
    let blurred = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    let report = |x: u32, y: u32| format!("({x}, {y}): {:?}", pixel(&blurred, x, y));
    for (x, low, high) in [(139, 30, 110), (144, 150, 230), (175, 150, 230), (180, 30, 110)] {
        let green = pixel(&blurred, x, 90)[1];
        assert!((low..=high).contains(&green), "partly covered {}", report(x, 90));
    }
    assert_eq!(pixel(&blurred, 135, 90), [255, 0, 0, 255], "{}", report(135, 90));
    assert_eq!(pixel(&blurred, 160, 90), [0, 255, 0, 255], "{}", report(160, 90));
    // Up and down it does not move, so the top and bottom stay sharp.
    assert_eq!(pixel(&blurred, 160, 71), [255, 0, 0, 255], "{}", report(160, 71));
    assert_eq!(pixel(&blurred, 160, 73), [0, 255, 0, 255], "{}", report(160, 73));
    // Half the shutter, half the smear: 137..147 becomes 139.5..144.5.
    set_blur(&mut project, 50.0);
    let half = renderer.render(&project, seconds(0.5), 320, 180).unwrap();
    assert_eq!(pixel(&half, 138, 90), [255, 0, 0, 255]);
    assert!((60..=200).contains(&pixel(&half, 142, 90)[1]), "{:?}", pixel(&half, 142, 90));
    // After the last key it holds still: sharp again.
    set_blur(&mut project, 100.0);
    let still = renderer.render(&project, seconds(1.5), 320, 180).unwrap();
    assert_eq!(pixel(&still, 160 + 150 - 18 - 2, 90), [255, 0, 0, 255]);
    assert_eq!(pixel(&still, 160 + 150 - 18 + 1, 90), [0, 255, 0, 255]);
}

/// The web editor's numbers for the same clip (dev editor, off-screen
/// render through the WASM compositor): a 90-px square at 640x360 sweeping
/// 500 px/s, a whole-frame shutter at 30 fps, row 180.
#[test]
fn motion_blur_matches_the_web_editor_on_the_same_clip() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Motion blur parity");
    project.settings.canvas = editor::CanvasSize { width: 640, height: 360 };
    project.settings.background = editor::Background::Color { color: "#000000".to_owned() };
    let image_id = add_image(&mut project, dir.path());
    let mut image = Element::from_media(project.media_item(&image_id).unwrap(), seconds(0.0));
    image.duration = seconds(2.0);
    image.params.set("transform.scaleX", 0.25).set("transform.scaleY", 0.25).set("motionBlur", 100.0);
    let element_id = image.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element: image, placement: TrackPlacement::Auto }).unwrap();
    for (time, value) in [(0.0, -250.0), (1.0, 250.0)] {
        apply(
            &mut project,
            &EditorCommand::SetKeyframe {
                element_id: element_id.clone(),
                path: "transform.positionX".to_owned(),
                time: seconds(time),
                value,
                segment: None,
            },
        )
        .unwrap();
    }
    let frame = renderer.render(&project, seconds(0.5), 640, 360).unwrap();
    let xs = [262, 266, 267, 270, 274, 279, 283, 284, 300, 320, 356, 360, 365, 370, 373, 374, 378];
    let web = [0, 0, 14, 57, 113, 198, 255, 255, 255, 255, 255, 198, 113, 42, 0, 0, 0];
    let desktop: Vec<u8> = xs.iter().map(|&x| pixel(&frame, x, 180)[1]).collect();
    eprintln!("desktop {desktop:?}");
    for ((x, d), w) in xs.iter().zip(&desktop).zip(web) {
        assert!((*d as i32 - w).abs() <= 3, "x {x}: desktop {d}, web {w} ({desktop:?})");
    }
    for (y, expected) in [(134, 0), (135, 255), (224, 255), (225, 0)] {
        assert_eq!(pixel(&frame, 320, y)[1], expected, "y {y}: top and bottom stay sharp");
    }
}

/// The web editor's numbers for the same text (dev editor, off-screen
/// render): Arial 9 white at 640x360, "WORKS" yellow at twice the size.
#[test]
fn rich_text_matches_the_web_editor_on_the_same_text() {
    let Some(mut renderer) = renderer(None) else { return };
    let mut project = Project::new("Rich text");
    project.settings.canvas = editor::CanvasSize { width: 640, height: 360 };
    project.settings.background = editor::Background::Color { color: "#000000".to_owned() };
    let mut text = Element::text("RICH TEXT WORKS", seconds(0.0));
    let rich = editor::RichText {
        text: "RICH TEXT WORKS".to_owned(),
        spans: vec![richtext::Span { start: 10, end: 15, color: Some("#ffd60a".into()), scale: Some(2.0), ..Default::default() }],
    };
    text.params
        .set("fontSize", 9.0)
        .set("fontFamily", "Arial")
        .set("color", "#ffffff")
        .set(editor::RICH_TEXT_PARAM, richtext::encode(&rich));
    apply(&mut project, &EditorCommand::InsertElement { element: text, placement: TrackPlacement::Auto }).unwrap();
    let frame = renderer.render(&project, seconds(1.0), 640, 360).unwrap();
    let (mut all, mut yellow, mut white) =
        ([u32::MAX, u32::MAX, 0, 0], [u32::MAX, u32::MAX, 0, 0], [u32::MAX, u32::MAX, 0, 0]);
    for y in 0..360 {
        for x in 0..640 {
            let [b, g, r, _] = pixel(&frame, x, y);
            let grow = |rect: &mut [u32; 4]| *rect = [rect[0].min(x), rect[1].min(y), rect[2].max(x), rect[3].max(y)];
            if r as u32 + g as u32 + b as u32 > 200 {
                grow(&mut all);
            }
            if r > 180 && g > 150 && b < 90 {
                grow(&mut yellow);
            }
            if r > 200 && g > 200 && b > 200 {
                grow(&mut white);
            }
        }
    }
    eprintln!("desktop all {all:?} yellow {yellow:?} white {white:?}");
    // The larger word shares the line's baseline and rises above it.
    let web = ([86, 149, 552, 202], [285, 149, 551, 202], [87, 176, 272, 201]);
    for (got, want, what) in [(all, web.0, "text"), (yellow, web.1, "yellow word"), (white, web.2, "plain words")] {
        for (g, w) in got.iter().zip(want) {
            assert!((*g as i32 - w).abs() <= 3, "{what}: desktop {got:?} vs web {want:?}");
        }
    }
}

/// A noisy skin oval on noisy blue, 640x360, from a seeded generator the
/// web check builds the same way.
const WEB_SKIN_MEAN: f64 = 173.06;
const WEB_SKIN_SPREAD: f64 = 5.45;

fn face_picture(path: &Path) {
    let mut x: u64 = 79;
    let mut next = || {
        x = (1103515245 * x + 12345) % (1 << 31);
        x as f64 / (1u64 << 31) as f64
    };
    let mut picture = image::RgbaImage::new(640, 360);
    for py in 0..360u32 {
        for px in 0..640u32 {
            let (dx, dy) = ((px as f64 - 320.0) / 140.0, (py as f64 - 180.0) / 120.0);
            let n = (next() - 0.5) * 40.0;
            let rgb = if dx * dx + dy * dy <= 1.0 {
                [222.0 + n, 170.0 + n * 0.8, 140.0 + n * 0.6]
            } else {
                [60.0 + n, 90.0 + n, 200.0 + n * 0.5]
            };
            let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
            picture.put_pixel(px, py, image::Rgba([c(rgb[0]), c(rgb[1]), c(rgb[2]), 255]));
        }
    }
    picture.save(path).unwrap();
}

/// Mean and spread of the green channel over a box, from BGRA.
fn green_stats(frame: &render::RenderedFrame, (x0, y0, x1, y1): (u32, u32, u32, u32)) -> (f64, f64) {
    let values: Vec<f64> = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| pixel(frame, x, y)[1] as f64).collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let spread = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64).sqrt();
    (mean, spread)
}

/// The web editor's numbers for the same picture with Retouch at its
/// defaults (dev editor, off-screen render through the WASM compositor).
#[test]
fn retouch_smooths_and_lifts_skin_and_leaves_the_rest_like_the_web() {
    let Some(mut renderer) = renderer(None) else { return };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("face.png");
    face_picture(&path);
    let mut project = Project::new("Retouch");
    project.settings.canvas = editor::CanvasSize { width: 640, height: 360 };
    apply(
        &mut project,
        &EditorCommand::AddMedia(MediaItem {
            id: "face".to_owned(),
            name: "face.png".to_owned(),
            kind: MediaKind::Image,
            path: path.clone(),
            duration: None,
            width: Some(640),
            height: Some(360),
            fps: None,
            has_audio: false,
        }),
    )
    .unwrap();
    let image = Element::from_media(project.media_item("face").unwrap(), seconds(0.0));
    let element_id = image.id.clone();
    apply(&mut project, &EditorCommand::InsertElement { element: image, placement: TrackPlacement::Auto }).unwrap();
    let skin = (270, 140, 370, 220);
    let blue = (10, 10, 110, 80);
    let before = renderer.render(&project, seconds(0.5), 640, 360).unwrap();
    apply(
        &mut project,
        &EditorCommand::AddEffect { element_id, effect: editor::Effect::from_catalogue("retouch").unwrap() },
    )
    .unwrap();
    let after = renderer.render(&project, seconds(0.5), 640, 360).unwrap();
    let (skin_before, skin_after) = (green_stats(&before, skin), green_stats(&after, skin));
    eprintln!("skin before {skin_before:?} after {skin_after:?}; blue {:?}", green_stats(&after, blue));
    assert!(skin_after.1 < skin_before.1 * 0.75, "smoother: {skin_before:?} -> {skin_after:?}");
    assert!(skin_after.0 > skin_before.0 + 1.0, "lifted: {skin_before:?} -> {skin_after:?}");
    for y in blue.1..blue.3 {
        for x in blue.0..blue.2 {
            assert_eq!(pixel(&before, x, y), pixel(&after, x, y), "not skin, so untouched at ({x}, {y})");
        }
    }
    // The web editor, same picture and settings: skin mean and spread.
    let web = (WEB_SKIN_MEAN, WEB_SKIN_SPREAD);
    assert!((skin_after.0 - web.0).abs() < 1.0 && (skin_after.1 - web.1).abs() < 1.0, "desktop {skin_after:?} vs web {web:?}");
}

#[test]
fn curved_text_bends_into_an_arch_in_the_frame() {
    let Some(mut renderer) = renderer(None) else { return };
    let mut project = Project::new("Curved text");
    project.settings.canvas = editor::CanvasSize { width: 640, height: 360 };
    project.settings.background = editor::Background::Color { color: "#000000".to_owned() };
    let mut text = Element::text("CURVED TEXT ROUND THE TOP", seconds(0.0));
    text.params.set("fontSize", 7.0).set("fontFamily", "Arial").set("color", "#ffffff").set("curve", 60.0);
    apply(&mut project, &EditorCommand::InsertElement { element: text, placement: TrackPlacement::Auto }).unwrap();
    let frame = renderer.render(&project, seconds(0.5), 640, 360).unwrap();
    let white = |x: u32, y: u32| {
        let p = pixel(&frame, x, y);
        p[0] > 128 && p[1] > 128 && p[2] > 128
    };
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (640, 0, 360, 0);
    for y in 0..360 {
        for x in 0..640 {
            if white(x, y) {
                (min_x, max_x, min_y, max_y) = (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y));
            }
        }
    }
    let mean_row = |x0: u32, x1: u32| {
        let (mut sum, mut count) = (0.0, 0.0);
        for y in 0..360 {
            for x in x0..x1 {
                if white(x, y) {
                    sum += y as f64;
                    count += 1.0;
                }
            }
        }
        sum / count
    };
    let (middle, left, right) = (mean_row(315, 325), mean_row(min_x, min_x + 12), mean_row(max_x - 12, max_x + 1));
    eprintln!("desktop curved text: bbox ({min_x}, {min_y})-({max_x}, {max_y}), middle {middle:.1}, left {left:.1}, right {right:.1}");
    // The top of the arch stays at the text's place (the canvas middle)
    // and the ends drop well below it, symmetrically.
    assert!((middle - 180.0).abs() < 12.0, "middle row {middle}");
    assert!(left > middle + 80.0 && right > middle + 80.0, "ends {left} / {right} vs middle {middle}");
    assert!((left - right).abs() < 12.0, "symmetric: {left} vs {right}");
    assert!(((min_x + max_x) as f64 / 2.0 - 320.0).abs() < 8.0, "centred: {min_x}..{max_x}");
}
