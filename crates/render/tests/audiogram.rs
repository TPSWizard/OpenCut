//! Audiograms (row 100) through the real renderer: mirrored bars sit
//! across the middle 70% of the frame, centred on the line three quarters
//! down, louder bands taller; the circle style rings the middle. Skips
//! without a GPU.

use editor::{AudiogramLevels, CanvasSize, Command as EditorCommand, Element, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

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

fn project(style: &str) -> Project {
    let mut project = Project::new("Audiogram");
    project.settings.canvas = CanvasSize { width: WIDTH, height: HEIGHT };
    let mut clip = Element::text("clip", MediaTime::ZERO);
    clip.duration = MediaTime::from_seconds_f64(2.0).unwrap();
    // Band b at b/31 of full scale, the same every frame.
    let bands = 32;
    let data: Vec<u8> = (0..60).flat_map(|_| (0..bands).map(|b| (b * 255 / (bands - 1)) as u8)).collect();
    let mut element = editor::audiogram_element(&clip, &AudiogramLevels { fps: 30.0, bands, data });
    element.params.set("style", style);
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    project
}

/// (first column, last column, first row, last row) of bright pixels, and
/// the bright rows in a column.
fn ink(bgra: &[u8]) -> (Option<(u32, u32, u32, u32)>, impl Fn(u32) -> u32 + '_) {
    let lit = |x: u32, y: u32| {
        let i = ((y * WIDTH + x) * 4) as usize;
        bgra[i] > 128 && bgra[i + 1] > 128 && bgra[i + 2] > 128
    };
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if lit(x, y) {
                bounds = Some(match bounds {
                    None => (x, x, y, y),
                    Some((x0, x1, y0, y1)) => (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
                });
            }
        }
    }
    (bounds, move |x: u32| (0..HEIGHT).filter(|&y| lit(x, y)).count() as u32)
}

#[test]
fn mirrored_bars_follow_the_levels() {
    let Some(mut renderer) = renderer() else { return };
    let frame = renderer.render(&project("mirror"), MediaTime::from_seconds_f64(0.5).unwrap(), WIDTH, HEIGHT).unwrap();
    let (bounds, column) = ink(&frame.bgra);
    let (x0, x1, y0, y1) = bounds.expect("bars drawn");
    eprintln!("bars {x0}..{x1} x {y0}..{y1}");
    // 70% of 640 centred: 96..544; the loudest bar is 20% of 360 = 72 px tall around row 270.
    assert!(x0.abs_diff(96) <= 4 && x1.abs_diff(544) <= 4, "{x0}..{x1}");
    assert!(y0.abs_diff(234) <= 2 && y1.abs_diff(306) <= 2, "{y0}..{y1}");
    // The first bar (quietest band) is the shortest, the last the tallest.
    let first = (x0..x0 + 6).map(&column).max().unwrap();
    let last = (x1 - 6..=x1).map(&column).max().unwrap();
    assert!(first <= 4 && last >= 70, "first {first} last {last}");
}

#[test]
fn the_circle_rings_its_centre() {
    let Some(mut renderer) = renderer() else { return };
    let frame = renderer.render(&project("circle"), MediaTime::from_seconds_f64(0.5).unwrap(), WIDTH, HEIGHT).unwrap();
    let (bounds, _) = ink(&frame.bgra);
    let (x0, x1, y0, y1) = bounds.expect("circle drawn");
    eprintln!("circle {x0}..{x1} x {y0}..{y1}");
    // Centre (320, 270); inner radius 70% of 360 / 4 = 63; bars up to 72 longer.
    let centre = |x: u32| {
        let i = ((270 * WIDTH + x) * 4) as usize;
        frame.bgra[i] > 128
    };
    assert!(!centre(320), "the middle stays clear");
    assert!(x0 >= 320 - 63 - 73 && x1 <= 320 + 63 + 73, "{x0}..{x1}");
    assert!(y0 >= 270 - 63 - 73 && y0 < 270 - 63, "top {y0}");
    assert!(y1 > 270 + 63 - 2, "bottom {y1}");
}
