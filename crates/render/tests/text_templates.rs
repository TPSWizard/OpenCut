//! Text templates and credit rolls (row 98) through the real renderer: the
//! end credits are off the frame at the clip's start and end and pass the
//! middle half way; the news crawl moves from right to left; a lower third
//! draws its boxes in the lower left. Skips without a GPU.

use editor::{CanvasSize, Command as EditorCommand, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

fn project_with(template: &str) -> Project {
    let mut project = Project::new("Templates");
    project.settings.canvas = CanvasSize { width: WIDTH, height: HEIGHT };
    let elements = editor::text_template_elements(editor::text_template(template).unwrap(), project.settings.canvas, MediaTime::ZERO);
    for element in elements {
        apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    }
    project
}

/// Rows and columns that have bright (text) pixels: (first row, last row, first column, last column).
fn ink(bgra: &[u8], threshold: u8) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            if bgra[i] > threshold && bgra[i + 1] > threshold && bgra[i + 2] > threshold {
                bounds = Some(match bounds {
                    None => (y, y, x, x),
                    Some((top, bottom, left, right)) => (top.min(y), bottom.max(y), left.min(x), right.max(x)),
                });
            }
        }
    }
    bounds
}

/// Saves a frame as a PNG when `TEMPLATE_FRAMES` names a folder (for a look by eye).
fn save(name: &str, bgra: &[u8]) {
    if let Some(dir) = std::env::var_os("TEMPLATE_FRAMES") {
        let rgba: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        image::RgbaImage::from_raw(WIDTH, HEIGHT, rgba).unwrap().save(std::path::Path::new(&dir).join(format!("{name}.png"))).unwrap();
    }
}

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

#[test]
fn end_credits_roll_up_through_the_frame() {
    let Some(mut renderer) = renderer() else { return };
    let project = project_with("end-credits");
    let at = |seconds: f64| MediaTime::from_seconds_f64(seconds).unwrap();
    let start = renderer.render(&project, at(0.0), WIDTH, HEIGHT).unwrap();
    let middle = renderer.render(&project, at(6.0), WIDTH, HEIGHT).unwrap();
    let later = renderer.render(&project, at(7.0), WIDTH, HEIGHT).unwrap();
    let end = renderer.render(&project, at(11.99), WIDTH, HEIGHT).unwrap();
    save("credits-6s", &middle.bgra);
    save("credits-7s", &later.bgra);
    assert_eq!(ink(&start.bgra, 200), None, "nothing shows yet");
    assert!(ink(&end.bgra, 200).is_none(), "the credits have left: {:?}", ink(&end.bgra, 200));
    let (top_a, bottom_a, _, _) = ink(&middle.bgra, 200).expect("credits half way");
    let (top_b, bottom_b, _, _) = ink(&later.bgra, 200).expect("credits a second later");
    eprintln!("half way rows {top_a}..{bottom_a}, a second later {top_b}..{bottom_b}");
    assert!(top_a < 40 && bottom_a > HEIGHT - 40, "the block fills the frame half way");
    // The block moved up: compare where a line's ink is (the first gap row).
    let row_profile = |bgra: &[u8]| -> Vec<bool> {
        (0..HEIGHT)
            .map(|y| (0..WIDTH).any(|x| bgra[((y * WIDTH + x) * 4) as usize] > 200))
            .collect()
    };
    let (a, b) = (row_profile(&middle.bgra), row_profile(&later.bgra));
    let shift = (1..120)
        .min_by_key(|shift| (0..HEIGHT as usize - shift).filter(|y| a[y + shift] != b[*y]).count())
        .unwrap();
    eprintln!("moved up {shift} px in a second");
    // About (frame + block) / 12 s: 360 px plus some 370 px of credits.
    assert!((45..80).contains(&shift), "{shift}");
}

#[test]
fn the_news_crawl_moves_left() {
    let Some(mut renderer) = renderer() else { return };
    let project = project_with("news-crawl");
    let at = |seconds: f64| MediaTime::from_seconds_f64(seconds).unwrap();
    // The red bar's left edge: the first column of strongly red pixels on its row.
    let bar_left = |seconds: f64, renderer: &mut Renderer| -> Option<u32> {
        let frame = renderer.render(&project, at(seconds), WIDTH, HEIGHT).unwrap();
        save(&format!("crawl-{seconds}"), &frame.bgra);
        let y = (HEIGHT as f64 * 0.92) as u32;
        (0..WIDTH).find(|x| {
            let i = ((y * WIDTH + x) * 4) as usize;
            frame.bgra[i + 2] > 150 && frame.bgra[i + 1] < 60
        })
    };
    assert_eq!(bar_left(0.0, &mut renderer), None, "the bar starts past the right edge");
    let early = bar_left(0.5, &mut renderer).expect("the bar comes in");
    assert!(early > WIDTH / 2, "it enters from the right: {early}");
    // A moment before the end only the bar's tail is left at the left edge.
    let frame = renderer.render(&project, at(9.95), WIDTH, HEIGHT).unwrap();
    let y = (HEIGHT as f64 * 0.92) as u32;
    let red = (0..WIDTH)
        .filter(|x| {
            let i = ((y * WIDTH + x) * 4) as usize;
            frame.bgra[i + 2] > 150 && frame.bgra[i + 1] < 60
        })
        .count() as u32;
    assert!(red < WIDTH / 20, "nearly gone at the end: {red} columns");
}

#[test]
fn a_lower_third_sits_bottom_left() {
    let Some(mut renderer) = renderer() else { return };
    let project = project_with("name-and-role");
    let frame = renderer.render(&project, MediaTime::from_seconds_f64(2.0).unwrap(), WIDTH, HEIGHT).unwrap();
    save("lower-third", &frame.bgra);
    // The amber role strip: strongly red and green, little blue.
    let mut amber = (u32::MAX, 0u32, u32::MAX, 0u32);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            let (b, g, r) = (frame.bgra[i], frame.bgra[i + 1], frame.bgra[i + 2]);
            if r > 200 && g > 150 && b < 80 {
                amber = (amber.0.min(y), amber.1.max(y), amber.2.min(x), amber.3.max(x));
            }
        }
    }
    eprintln!("amber strip rows {}..{} columns {}..{}", amber.0, amber.1, amber.2, amber.3);
    assert!(amber.0 > HEIGHT * 3 / 4, "in the lower part");
    let centre = (amber.2 + amber.3) / 2;
    assert!(centre < WIDTH / 2 && centre.abs_diff((WIDTH as f64 * 0.26) as u32) < 8, "centred 24% left of the middle: {centre}");
}
