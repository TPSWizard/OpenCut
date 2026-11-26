//! Word art (row 99) through the real renderer: the "Candy" style on bold
//! "WORD ART" at 1920x1080 draws a pink-to-orange gradient inside a white
//! then a berry outline, each band 6% of the em (13 px), as the web editor
//! does (its still of the same clip: berry box x 342..1576 y 417..627,
//! white 355..1563 / 430..614, gradient 368..1550 / 443..601, the plain
//! letters 368..1549 / 443..601; gradient green/blue at the top about
//! 118/170, at the bottom 173/81). Skips without a GPU.

use editor::{CanvasSize, Command as EditorCommand, Element, Project, TrackPlacement, apply};
use render::{RenderError, Renderer, RendererOptions};
use time::MediaTime;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

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

#[derive(Debug, Clone, Copy)]
struct Bounds {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    n: u32,
}

impl Bounds {
    fn new() -> Self {
        Self { x0: u32::MAX, y0: u32::MAX, x1: 0, y1: 0, n: 0 }
    }
    fn add(&mut self, x: u32, y: u32) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
        self.n += 1;
    }
}

#[test]
fn candy_word_art_matches_the_web() {
    let Some(mut renderer) = renderer() else { return };
    let mut project = Project::new("Word art");
    project.settings.canvas = CanvasSize { width: WIDTH, height: HEIGHT };
    let mut element = Element::text("WORD ART", MediaTime::ZERO);
    element.params.set("fontSize", 18.0).set("fontWeight", "bold");
    for (key, value) in editor::wordart::style_params("candy").unwrap() {
        match value {
            serde_json::Value::String(text) => element.params.set(key, text),
            serde_json::Value::Number(number) => element.params.set(key, number.as_f64().unwrap()),
            _ => &mut element.params,
        };
    }
    let mut plain_project = project.clone();
    let mut plain = element.clone();
    plain.params.set("wordArt.style", "none").set("wordArt.fill", "solid");
    for key in editor::wordart::keys::STROKE_WIDTH {
        plain.params.set(key, 0.0);
    }
    apply(&mut plain_project, &EditorCommand::InsertElement { element: plain, placement: TrackPlacement::Auto }).unwrap();
    apply(&mut project, &EditorCommand::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
    let frame = renderer.render(&project, MediaTime::from_seconds_f64(1.0).unwrap(), WIDTH, HEIGHT).unwrap();
    let plain_frame = renderer.render(&plain_project, MediaTime::from_seconds_f64(1.0).unwrap(), WIDTH, HEIGHT).unwrap();
    let mut plain_white = Bounds::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            if plain_frame.bgra[i] > 240 && plain_frame.bgra[i + 1] > 240 && plain_frame.bgra[i + 2] > 240 {
                plain_white.add(x, y);
            }
        }
    }
    if let Some(dir) = std::env::var_os("WORD_ART_FRAMES") {
        let rgba: Vec<u8> = frame.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        image::RgbaImage::from_raw(WIDTH, HEIGHT, rgba).unwrap().save(std::path::Path::new(&dir).join("candy.png")).unwrap();
    }

    let (mut berry, mut white, mut gradient) = (Bounds::new(), Bounds::new(), Bounds::new());
    let mut rows: Vec<(u32, u8, u8)> = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            let (b, g, r) = (frame.bgra[i] as i32, frame.bgra[i + 1] as i32, frame.bgra[i + 2] as i32);
            if (r - 194).abs() < 30 && (g - 24).abs() < 30 && (b - 91).abs() < 30 {
                berry.add(x, y);
            } else if r > 240 && g > 240 && b > 240 {
                white.add(x, y);
            } else if r > 230 && g > 100 && g < 190 && b > 60 && b < 190 {
                gradient.add(x, y);
                rows.push((y, g as u8, b as u8));
            }
        }
    }
    let span = (gradient.y1 - gradient.y0) as f64;
    let mean = |pick: &dyn Fn(u32) -> bool| {
        let chosen: Vec<_> = rows.iter().filter(|(y, _, _)| pick(*y)).collect();
        let n = chosen.len().max(1) as f64;
        (
            chosen.iter().map(|(_, g, _)| *g as f64).sum::<f64>() / n,
            chosen.iter().map(|(_, _, b)| *b as f64).sum::<f64>() / n,
        )
    };
    let top = mean(&|y| (y as f64) < gradient.y0 as f64 + span * 0.15);
    let bottom = mean(&|y| (y as f64) > gradient.y1 as f64 - span * 0.15);
    eprintln!("plain white {plain_white:?}");
    eprintln!("berry {berry:?}\nwhite {white:?}\ngradient {gradient:?}\ntop g/b {top:?} bottom g/b {bottom:?}");

    // Each band is 6% of a 216 px em: about 13 px.
    for (outer, inner) in [(berry, white), (white, gradient)] {
        for gap in [inner.x0 as i32 - outer.x0 as i32, outer.x1 as i32 - inner.x1 as i32, inner.y0 as i32 - outer.y0 as i32, outer.y1 as i32 - inner.y1 as i32] {
            assert!((11..=15).contains(&gap), "band {gap} px");
        }
    }
    // Where the web draws it (fonts differ a little between the editors).
    let near = |got: u32, want: u32, slack: u32| got.abs_diff(want) <= slack;
    // Word art leaves the letters where plain text has them; the outlines
    // reach 26 px past them, as on the web (whose plain letters sit 13 px
    // higher at this size: the editors already place large text a little
    // differently, about 6% of the em).
    for (got, want) in [(gradient.x0, plain_white.x0), (gradient.x1, plain_white.x1), (gradient.y0, plain_white.y0), (gradient.y1, plain_white.y1)] {
        assert!(near(got, want, 1), "letters moved: {got} vs {want}");
    }
    assert!(near(berry.y0 + 26, plain_white.y0, 3) && near(berry.y1, plain_white.y1 + 26, 3), "berry rows {}..{}", berry.y0, berry.y1);
    assert!(near(berry.x0, 342, 30) && near(berry.x1, 1576, 30), "berry columns {}..{}", berry.x0, berry.x1);
    // Pink (green 111, blue 181) at the top, orange (179, 71) at the bottom.
    assert!(top.0 < 135.0 && top.1 > 155.0, "top {top:?}");
    assert!(bottom.0 > 160.0 && bottom.1 < 100.0, "bottom {bottom:?}");
}
