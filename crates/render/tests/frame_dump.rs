//! Renders single frames of the project `<root>/<id>/project.json` named by
//! `OPENCUT_FRAME_PROJECT=<root>/<id>` at the times (seconds, comma
//! separated) in `OPENCUT_FRAME_TIMES` into PNGs under
//! `OPENCUT_FRAME_OUTPUT` (a directory). A developer aid for looking at one
//! frame without exporting; skipped otherwise.

use std::path::PathBuf;

use media::Ffmpeg;
use render::{Renderer, RendererOptions};
use time::MediaTime;

#[test]
fn renders_the_frames_named_by_the_environment() {
    let Ok(path) = std::env::var("OPENCUT_FRAME_PROJECT") else {
        return;
    };
    let output = PathBuf::from(std::env::var("OPENCUT_FRAME_OUTPUT").unwrap_or_else(|_| ".".to_owned()));
    let times: Vec<f64> = std::env::var("OPENCUT_FRAME_TIMES")
        .unwrap_or_else(|_| "0".to_owned())
        .split(',')
        .filter_map(|text| text.trim().parse().ok())
        .collect();
    let path = PathBuf::from(path);
    let id = path.file_name().unwrap().to_string_lossy().into_owned();
    let store = editor::ProjectStore::new(path.parent().unwrap());
    let project = store.load(&id).expect("project loads");
    let ffmpeg = Ffmpeg::locate().ok();
    let mut renderer = Renderer::new(RendererOptions { ffmpeg, fonts_dir: None, stickers_dir: None, proxy_dir: None }).expect("renderer");
    let (width, height) = (project.settings.canvas.width, project.settings.canvas.height);
    std::fs::create_dir_all(&output).unwrap();
    for seconds in times {
        let time = MediaTime::from_seconds_f64(seconds).unwrap();
        let frame = renderer.render(&project, time, width, height).expect("frame renders");
        let mut rgba = frame.bgra.clone();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let image = image::RgbaImage::from_raw(frame.width, frame.height, rgba).unwrap();
        let file = output.join(format!("{id}-{seconds}.png"));
        image.save(&file).unwrap();
        eprintln!("wrote {}", file.display());
    }
}
