//! `opencut-render`: the native renderer from the command line. Takes a
//! project directory (one with `project.json`) or an `.opencut.zip` bundle
//! and writes a video, stills at chosen times, or a contact sheet of
//! those stills, through the same crates the desktop app uses.
//!
//! ```text
//! opencut-render --project ~/.opencut/projects/<id> --out promo.mp4
//! opencut-render --bundle promo.opencut.zip --out shorts.mp4 --preset youtube-shorts
//! opencut-render --project <dir> --stills every=1 --stills-dir qa/ --contact-sheet qa/sheet.png
//! opencut-render --project <dir> --frame-at 2.5 --out frame.png
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use editor::{CanvasSize, ImportReport, Project, ProjectStore, import_bundle, platform_encode_settings, platform_preset};
use media::{EncodeFormat, EncodeQuality, Ffmpeg, PlatformEncode};
use render::{ExportJob, Renderer, RendererOptions, export_project, still_times};
use time::{FrameRate, MediaTime};

const USAGE: &str = "\
opencut-render: render a ConzaCut project from the command line

  --project <dir>          a project directory (holds project.json)
  --bundle <file>          an .opencut.zip bundle (imported to a temporary store)
  --out <file>             the video (.mp4 or .webm) or, with --frame-at, a .png
  --format mp4|webm        container when --out has no telling extension
  --quality low|medium|high|very_high   encoder quality (default high)
  --preset <id>            platform preset: youtube, youtube-shorts, tiktok,
                           instagram-reels, linkedin, x (MP4, H.264 High, closed GOP,
                           faststart); sets the canvas and fps to the platform's
  --fps <n>                render at this frame rate instead of the project's
  --size <w>x<h>           render at this canvas size instead of the project's
  --no-audio               leave the audio out
  --frame-at <s>           write one PNG still at this time to --out
  --stills every=<s>|at=<s,s,...>   write PNG stills to --stills-dir
  --stills-dir <dir>       where stills go (default: stills/ next to --out, or ./stills)
  --contact-sheet <png>    tile the stills (320 px wide each) into one image
  --quiet                  no progress on stderr
  --help                   this text

Needs FFmpeg on PATH (or OPENCUT_FFMPEG) for video and audio; fonts and
stickers fetched by the desktop under ~/.opencut are used when present.
";

struct Args {
    project: Option<PathBuf>,
    bundle: Option<PathBuf>,
    out: Option<PathBuf>,
    format: Option<EncodeFormat>,
    quality: EncodeQuality,
    preset: Option<String>,
    fps: Option<f64>,
    size: Option<(u32, u32)>,
    include_audio: bool,
    frame_at: Option<f64>,
    stills_every: Option<f64>,
    stills_at: Vec<f64>,
    stills_dir: Option<PathBuf>,
    contact_sheet: Option<PathBuf>,
    quiet: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        project: None,
        bundle: None,
        out: None,
        format: None,
        quality: EncodeQuality::High,
        preset: None,
        fps: None,
        size: None,
        include_audio: true,
        frame_at: None,
        stills_every: None,
        stills_at: Vec::new(),
        stills_dir: None,
        contact_sheet: None,
        quiet: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--project" => args.project = Some(PathBuf::from(value()?)),
            "--bundle" => args.bundle = Some(PathBuf::from(value()?)),
            "--out" => args.out = Some(PathBuf::from(value()?)),
            "--format" => {
                args.format = Some(match value()?.as_str() {
                    "mp4" => EncodeFormat::Mp4,
                    "webm" => EncodeFormat::Webm,
                    other => return Err(format!("unknown format {other}")),
                })
            }
            "--quality" => {
                args.quality = match value()?.as_str() {
                    "low" => EncodeQuality::Low,
                    "medium" => EncodeQuality::Medium,
                    "high" => EncodeQuality::High,
                    "very_high" | "best" => EncodeQuality::VeryHigh,
                    other => return Err(format!("unknown quality {other}")),
                }
            }
            "--preset" => args.preset = Some(value()?),
            "--fps" => args.fps = Some(value()?.parse().map_err(|_| "--fps needs a number".to_owned())?),
            "--size" => {
                let text = value()?;
                let (w, h) = text.split_once('x').ok_or_else(|| "--size needs <w>x<h>".to_owned())?;
                args.size = Some((
                    w.parse().map_err(|_| "--size needs numbers".to_owned())?,
                    h.parse().map_err(|_| "--size needs numbers".to_owned())?,
                ));
            }
            "--no-audio" => args.include_audio = false,
            "--frame-at" => args.frame_at = Some(value()?.parse().map_err(|_| "--frame-at needs seconds".to_owned())?),
            "--stills" => {
                let text = value()?;
                if let Some(every) = text.strip_prefix("every=") {
                    args.stills_every = Some(every.parse().map_err(|_| "--stills every=<seconds>".to_owned())?);
                } else if let Some(list) = text.strip_prefix("at=") {
                    for part in list.split(',').filter(|p| !p.trim().is_empty()) {
                        args.stills_at.push(part.trim().parse().map_err(|_| format!("--stills at: {part} is not seconds"))?);
                    }
                } else {
                    return Err("--stills takes every=<s> or at=<s,s,...>".to_owned());
                }
            }
            "--stills-dir" => args.stills_dir = Some(PathBuf::from(value()?)),
            "--contact-sheet" => args.contact_sheet = Some(PathBuf::from(value()?)),
            "--quiet" => args.quiet = true,
            "--help" | "-h" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    if args.project.is_none() && args.bundle.is_none() {
        return Err(format!("give --project <dir> or --bundle <file>\n\n{USAGE}"));
    }
    if args.out.is_none() && args.stills_every.is_none() && args.stills_at.is_empty() && args.frame_at.is_none() {
        return Err("nothing to do: give --out, --frame-at or --stills".to_owned());
    }
    Ok(args)
}

/// Loads the project from a directory or a bundle (kept in `keep_alive`).
fn load_project(args: &Args, keep_alive: &mut Option<tempfile::TempDir>) -> Result<Project, String> {
    if let Some(dir) = &args.project {
        let dir = dir.canonicalize().map_err(|error| format!("{}: {error}", dir.display()))?;
        let id = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| "--project needs a directory".to_owned())?;
        let root = dir.parent().ok_or_else(|| "--project needs a directory inside a store".to_owned())?;
        return ProjectStore::new(root).load(&id).map_err(|error| format!("{}: {error}", dir.display()));
    }
    let bundle = args.bundle.as_ref().expect("checked");
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ProjectStore::new(temp.path().join("projects"));
    let ImportReport { project, skipped } = import_bundle(bundle, &store).map_err(|error| format!("{}: {error}", bundle.display()))?;
    if !skipped.is_empty() && !args.quiet {
        eprintln!("opencut-render: left out of the bundle: {}", skipped.join("; "));
    }
    *keep_alive = Some(temp);
    Ok(project)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

fn renderer_options(ffmpeg: Option<Arc<Ffmpeg>>) -> RendererOptions {
    let opencut = home_dir().map(|home| home.join(".opencut"));
    RendererOptions {
        ffmpeg,
        fonts_dir: opencut.as_ref().map(|dir| dir.join("fonts")),
        stickers_dir: opencut.as_ref().map(|dir| dir.join("stickers")),
    }
}

fn write_png(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<(), String> {
    let mut rgba = bgra.to_vec();
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8).map_err(|error| format!("{}: {error}", path.display()))
}

fn contact_sheet(stills: &[(f64, PathBuf)], output: &Path) -> Result<(), String> {
    if stills.is_empty() {
        return Err("no stills to tile".to_owned());
    }
    const TILE_WIDTH: u32 = 320;
    let mut tiles = Vec::with_capacity(stills.len());
    for (_, path) in stills {
        let decoded = image::open(path).map_err(|error| format!("{}: {error}", path.display()))?.to_rgba8();
        let (w, h) = decoded.dimensions();
        let tile_height = ((h as f64 * TILE_WIDTH as f64) / w.max(1) as f64).round().max(1.0) as u32;
        tiles.push(image::imageops::resize(&decoded, TILE_WIDTH, tile_height, image::imageops::FilterType::Triangle));
    }
    let tile_height = tiles.iter().map(|tile| tile.height()).max().unwrap_or(1);
    let columns = (stills.len() as f64).sqrt().ceil().max(1.0) as u32;
    let rows = (stills.len() as u32).div_ceil(columns);
    let mut sheet = image::RgbaImage::from_pixel(columns * TILE_WIDTH, rows * tile_height, image::Rgba([0, 0, 0, 255]));
    for (index, tile) in tiles.iter().enumerate() {
        let x = (index as u32 % columns) * TILE_WIDTH;
        let y = (index as u32 / columns) * tile_height;
        image::imageops::overlay(&mut sheet, tile, x as i64, y as i64);
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    sheet.save(output).map_err(|error| format!("{}: {error}", output.display()))
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut keep_alive = None;
    let mut project = load_project(&args, &mut keep_alive)?;

    // Overrides: preset first, then explicit size and fps on top.
    let mut platform: Option<PlatformEncode> = None;
    if let Some(id) = &args.preset {
        let preset = platform_preset(id).ok_or_else(|| format!("unknown preset {id}; see --help"))?;
        project.settings.canvas = preset.canvas;
        project.settings.fps = preset.fps;
        let encode = platform_encode_settings(preset.canvas, preset.fps.as_f64().unwrap_or(30.0));
        platform = Some(PlatformEncode {
            video_bitrate: encode.video_bitrate,
            audio_bitrate: encode.audio_bitrate,
            gop_frames: encode.gop_frames,
        });
    }
    if let Some((width, height)) = args.size {
        project.settings.canvas = CanvasSize { width: width.max(2), height: height.max(2) };
    }
    if let Some(fps) = args.fps {
        let rate = if (fps - fps.round()).abs() < 1e-6 {
            FrameRate { numerator: fps.round() as u32, denominator: 1 }
        } else {
            FrameRate { numerator: (fps * 1001.0).round() as u32, denominator: 1001 }
        };
        project.settings.fps = rate;
        if let Some(encode) = platform.as_mut() {
            encode.gop_frames = ((fps / 2.0).round() as u32).max(1);
        }
    }

    let ffmpeg = match Ffmpeg::locate() {
        Ok(ffmpeg) => Some(ffmpeg),
        Err(error) => {
            if !args.quiet {
                eprintln!("opencut-render: {error}; video clips and audio will be missing");
            }
            None
        }
    };
    let mut renderer = Renderer::new(renderer_options(ffmpeg.clone())).map_err(|error| error.to_string())?;
    let canvas = project.settings.canvas;
    let fps = project.settings.fps.as_f64().unwrap_or(30.0);
    let duration = project.duration().to_seconds_f64();
    if !args.quiet {
        eprintln!(
            "opencut-render: \"{}\" {}x{} @ {fps:.2} fps, {duration:.2} s",
            project.name, canvas.width, canvas.height
        );
    }

    // One still.
    if let Some(at) = args.frame_at {
        let out = args.out.as_ref().ok_or_else(|| "--frame-at needs --out <file.png>".to_owned())?;
        let time = MediaTime::from_seconds_f64(at.max(0.0)).unwrap_or(MediaTime::ZERO);
        let frame = renderer.render(&project, time, canvas.width, canvas.height).map_err(|error| error.to_string())?;
        write_png(out, frame.width, frame.height, &frame.bgra)?;
        if !args.quiet {
            eprintln!("opencut-render: wrote {}", out.display());
        }
        return Ok(());
    }

    // Stills and a contact sheet.
    let times = still_times(duration, fps, args.stills_every, &args.stills_at);
    let mut stills: Vec<(f64, PathBuf)> = Vec::new();
    if !times.is_empty() {
        let dir = args
            .stills_dir
            .clone()
            .or_else(|| args.out.as_ref().and_then(|out| out.parent()).map(|parent| parent.join("stills")))
            .unwrap_or_else(|| PathBuf::from("stills"));
        std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        for (index, seconds) in times.iter().enumerate() {
            let time = MediaTime::from_seconds_f64(*seconds).unwrap_or(MediaTime::ZERO);
            let frame = renderer.render(&project, time, canvas.width, canvas.height).map_err(|error| error.to_string())?;
            let path = dir.join(format!("still-{index:03}-{:08.3}s.png", seconds));
            write_png(&path, frame.width, frame.height, &frame.bgra)?;
            stills.push((*seconds, path));
        }
        if !args.quiet {
            eprintln!("opencut-render: {} still(s) in {}", stills.len(), dir.display());
        }
        if let Some(sheet) = &args.contact_sheet {
            contact_sheet(&stills, sheet)?;
            if !args.quiet {
                eprintln!("opencut-render: contact sheet {}", sheet.display());
            }
        }
    } else if args.contact_sheet.is_some() {
        return Err("--contact-sheet needs --stills".to_owned());
    }

    // The video.
    if let Some(out) = &args.out {
        let ffmpeg = ffmpeg.ok_or_else(|| media::MediaError::FfmpegNotFound.to_string())?;
        let format = args.format.unwrap_or_else(|| EncodeFormat::from_path(out));
        let mut output = out.clone();
        if output.extension().is_none() {
            output.set_extension(format.extension());
        }
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
            }
        }
        let job = ExportJob {
            output,
            format,
            quality: args.quality,
            platform: platform.filter(|_| format == EncodeFormat::Mp4),
            include_audio: args.include_audio,
        };
        let quiet = args.quiet;
        let started = std::time::Instant::now();
        let written = export_project(&mut renderer, &ffmpeg, &project, &job, &|| false, &mut |done, total| {
            if !quiet && (done % 30 == 0 || done == total) {
                eprint!("\ropencut-render: frame {done}/{total}");
            }
        })?;
        if !quiet {
            eprintln!("\ropencut-render: wrote {} in {:.1} s", written.display(), started.elapsed().as_secs_f64());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("opencut-render: {message}");
            ExitCode::from(1)
        }
    }
}
