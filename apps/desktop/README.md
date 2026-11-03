# OpenCut Desktop

The native editor, built with [GPUI](https://www.gpui.rs) on the Rust core in
`../../crates`. It opens the same kind of project as the web app (media,
multi-track timeline, text, transforms, audio) and renders through the shared
`compositor` crate on the GPU.

## What it does

- **Projects**: a project list with create, open and delete. Projects are JSON
  files in `~/.opencut/projects/<id>/project.json` (set `OPENCUT_PROJECTS` to
  use another folder). Media is referenced by path, never copied.
- **Media**: import video, audio and images (file picker or `Ctrl/Cmd-I`);
  thumbnails; put an item on the timeline at the playhead with **Add** or a
  double-click; remove it from the project.
- **Timeline**: main video track, overlay video and text tracks above it,
  audio tracks below. Drag clips to move them (between tracks too), drag their
  edges to trim against the source, split at the playhead, duplicate, delete,
  mute or hide a track, zoom (`Ctrl/Cmd` + wheel or `+`/`-`), scroll (shift +
  wheel). Overlapping is prevented; auto placement creates a new track when the
  target is busy.
- **Preview**: the composited frame at the playhead, rendered at the size it
  is shown; play/pause (`Space`), frame stepping (`←`/`→`), home/end. Audio
  plays through the default output device, mixed from every audible clip,
  and the picture follows the device's clock so the two never drift.
- **Inspector**: name, transform (position, scale, rotation, opacity), text
  (content, font, size, color, alignment, bold/italic, line height, letter
  spacing), audio volume (keyframeable) and mute; project name, background (a colour, or the
  main track blurred behind the canvas at the web app's light/medium/heavy presets), frame rate
  and canvas presets.
- **Keyframes**: the diamond next to a numeric field adds a key at the
  playhead (or removes the one there); once a parameter has keys, typing a
  value keys it at the playhead and the field follows the animation. Keys are
  linear by default; `step` and `bezier` segments with the web app's curve
  handles are read from project files and evaluated the same way (a Bézier
  without handles is a straight line, as in the web graph editor). Keys stay
  with their source frames when a clip is trimmed and are split with the
  clip, a cut Bézier keeping its shape on both sides. Colour keys from the
  web app (`color`, `background.color`: four channels in linear light) are
  blended the same way and show as key dots too. The selected clip shows its keys as dots; click one
  to jump to it, or use `Shift-←`/`Shift-→`. Audio volume keys are followed
  by the preview mixer and written into the export as a per-frame `volume`
  expression.
- **Effects**: a Gaussian blur per element (intensity 0–100, keyframeable),
  with the same strength scaling as the web app; several can be stacked and
  switched off without removing them.
- **Mask**: a rectangle, ellipse, diamond, heart, star, split line,
  cinematic bars or text (its own words, font and size) on video and image
  clips, sized and offset as percentages of the clip, rotated, feathered
  (GPU distance field) or inverted, with an optional stroke along the edge
  (width, colour, inside/centre/outside; on text with round joins); the same shapes and
  parameters as the web editor. **Draw path (pen)** draws a freeform mask:
  click points on the preview (drag while placing one to pull out its curve
  handles), double-click or `Enter` closes it (three points or more),
  `Escape` cancels; paths drawn in the web app render too.
- **Undo/redo** for every edit (`Ctrl/Cmd-Z`, `Ctrl/Cmd-Shift-Z`), autosave.
- **Export** to MP4 (H.264 + AAC) or WebM (VP9 + Opus) at the canvas size and
  frame rate, with the audio clips mixed, with progress and cancel.

### Shortcuts

| Keys | Action |
| --- | --- |
| `Space`, `K` | Play / pause |
| `←` / `→`, `Home` / `End` | Step a frame, go to start / end |
| `Shift-←` / `Shift-→` | Previous / next keyframe of the selected clip |
| `S` | Split at the playhead (selection, or everything under it) |
| `Delete` / `Backspace` | Delete the selection |
| `Ctrl/Cmd-D`, `Ctrl/Cmd-A`, `Esc` | Duplicate, select all, clear selection |
| `Ctrl/Cmd-Z`, `Ctrl/Cmd-Shift-Z` (`Ctrl-Y`) | Undo, redo |
| `T` | Add a text element at the playhead |
| `Ctrl/Cmd-I`, `Ctrl/Cmd-E`, `Ctrl/Cmd-S`, `Ctrl/Cmd-W` | Import, export, save, close project |

## Requirements

- **FFmpeg** (`ffmpeg` and `ffprobe`): video and audio decoding, encoding and
  probing go through the FFmpeg programs over pipes (see `crates/media`).
  They are looked for in `OPENCUT_FFMPEG_DIR`, in the pinned build that
  `moon run media:setup` puts in `.cache/media/`, next to the executable, and
  on `PATH`. Without them the app still runs with images and text, and says
  so in the status line.
- A GPU with a Vulkan, Metal or Direct3D 12 driver for the compositor (wgpu).
- The platform requirements of GPUI below.

## Running

Rust is pinned in `.prototools` at the repo root (`proto use` installs it).

```sh
moon run desktop:dev     # cargo run
moon run desktop:check   # cargo check
moon run desktop:test    # unit tests of the app itself
moon run desktop:build   # cargo build --release
```

The first build compiles GPUI from source and takes a while. The root `Cargo.lock` is committed.

## How it is put together

```
apps/desktop/src
├── main.rs            window, key bindings, FFmpeg lookup, project store
├── shell.rs           projects screen ⇄ editor layout, actions
├── state.rs           EditorState: the open project, selection, playback,
│                      preview requests, saving, export (one GPUI entity)
├── render_thread.rs   preview renderer and export jobs, off the UI thread
├── audio.rs           cpal output fed by render::Mixer on a mixer thread
├── panels/            projects, media browser, preview, inspector,
│                      timeline, export dialog
└── components/        button, badge, text input, resizable panels, …
```

Editing goes through `editor::Command`s applied to an `editor::Editor` (the
document plus undo history); the panels only call methods on `EditorState`.
Frames come from `render::Renderer`, which plans the frame with
`editor::plan_frame`, decodes through `media`, rasterises text with
cosmic-text and composites with the `compositor` crate, then reads the
pixels back for GPUI to show. Export runs the same renderer at full
resolution and pipes BGRA frames into `media::Encoder`.

## Platform requirements

- **macOS**: Xcode command line tools (Metal renderer).
- **Windows**: no extra dependencies (Win32 + DirectWrite).
- **Linux**: renders via Vulkan (Blade), windows via Wayland or X11 (both enabled by default). System packages (Debian/Ubuntu names): `libvulkan1` + working Vulkan drivers, `libwayland-dev`, `libx11-xcb-dev`, `libxkbcommon-x11-dev`, `libfontconfig-dev`, `libasound2-dev` (audio), plus a C toolchain and `cmake`.
- **WSL2/WSLg**: uses XWayland automatically when available. GPUI 0.2.2 requires `xdg_wm_base` v2–5, while WSLg advertises v1.

## Not there yet

- Only the blur effect exists natively. The web editor's stickers and
  captions are read from project files but not rendered; a pen path's
  handles cannot be edited after the point is placed; colour keys can be
  played and cut but not set here.
- Project bundles move projects between the two editors: **Save bundle…**
  in the header writes `<name>.opencut.zip` (the project in the web app's
  JSON shape plus copies of its media), and **Import bundle…** on the
  projects page reads one, from here or from the web app's *Export project…*
  menu item. Web-only content (stickers, graphics, effect tracks, library
  sounds, extra scenes) is left out of an import and listed on the page.
