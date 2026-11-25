# ConzaCut Desktop

The native editor (ConzaCut is this fork's name for OpenCut; the binary, data
folder, environment variables and `.opencut.zip` bundles keep the OpenCut
names so projects stay interchangeable with upstream). It is built with [GPUI](https://www.gpui.rs) on the Rust core in
`../../crates`. It opens the same kind of project as the web app (media,
multi-track timeline, text, transforms, audio) and renders through the shared
`compositor` crate on the GPU.

## What it does

- **Transitions**: a clip can lead out of its end with any of the 176
  gl-transitions shaders (`Fade`, `DirectionalWipe`, `CrossZoom`, `Cube`, …),
  chosen in the inspector's Transition section (type a name, Previous/Next,
  or a quick pick), with a duration in seconds and the shader's parameters.
  It plays centred on the cut to the next clip on the track, half over each
  side, from the clips' trimmed-off handles (an exhausted handle holds its
  edge frame); with nothing after the clip it fades out over the clip's end.
  The web app reads and writes the same `transition` field, and both draw it
  with the same compositor pass.
- **Captions**: a text element may carry timed `words` (the web editor's
  caption generator writes them, in step with the element's start), and its
  `caption.highlight` param says how the spoken word stands out: `word`
  colours it, `box` also draws a rounded box behind it, `fill` colours every
  word spoken so far, `none` leaves the text plain. The inspector's Captions
  section (shown for text with words) picks the mode and the two colours;
  the rasteriser colours the words as rich-text spans, so the layout never
  moves. Transcription itself runs only in the web editor. Words flagged
  `keyword` (Pick keywords in that section runs the web editor's local
  pick, `editor::pick_keyword_indices`; Clear keywords drops them) are drawn
  in `caption.keywordColor` at `caption.keywordScale` times the font size,
  which widens their line; the spoken-word colour still wins.

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
  clip, a cut Bézier keeping its shape on both sides. Colour fields (a
  text's colour, a mask's stroke colour) key too: a colour key is the web
  app's four channels in linear light under one key id, so keys set here
  and in the web app blend the same way. The selected clip shows its keys as dots; click one
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
  parameters as the web editor. **Draw path (pen)** (or `P`) draws a freeform
  mask: click points on the preview (drag while placing one to pull out its curve
  handles), double-click or `Enter` closes it (three points or more),
  `Escape` cancels; paths drawn in the web app render too. A closed path is
  edited on the preview as in the web app: drag a point to move it, click
  on a segment to add a point there, drag a segment to move the whole
  path, `Shift`-click to select several points and `Delete` to remove the
  selected ones (`Escape` drops the point selection first); a path left
  with fewer than three points opens and stops masking.
- **Undo/redo** for every edit (`Ctrl/Cmd-Z`, `Ctrl/Cmd-Shift-Z`), autosave.
- **Export** to MP4 (H.264 + AAC) or WebM (VP9 + Opus) at the canvas size and
  frame rate, with the audio clips mixed, with progress and cancel. A
  **Platform** row (YouTube, Shorts, TikTok, Reels, LinkedIn, X) encodes the
  way YouTube documents uploads: H.264 High profile at the bitrate from its
  SDR table, two B-frames, a closed GOP of half the frame rate, the moov atom
  first and 384 kbps AAC. The dialog lists what the platform would object to
  (canvas, frame rate, length, file size) with a button for each project
  setting that clears it. The table lives in `crates/editor/src/platforms.rs`
  and matches the web app's.
- **Auto-ducking** in the inspector's Audio section: *Duck under voice*
  measures every other audio clip (or one chosen as the voice) over the
  selected clip with FFmpeg at 16 kHz, finds speech (RMS above −40 dBFS per
  50 ms window, gaps under 0.3 s bridged) and writes the clip's volume keys:
  down by the chosen amount 0.1 s before each passage, back up 0.5 s after,
  in one undo step. *Clear* drops the keys. The arithmetic lives in
  `crates/editor/src/ducking.rs` and matches the web app's.
- **Silence auto-cut** next to it: *Cut silence* measures the selected
  clip's own audio, finds pauses under −40 dBFS longer than the chosen
  length (0.3 s to 2 s) and removes them in one undo step, keeping 0.1 s
  next to the speech. *Afterwards* picks whether every track, only this
  track or nothing closes up behind each cut. The command
  (`Command::RemoveRanges`) and the detection (`crates/editor/src/silence.rs`)
  match the web app's.
- **Native transcription**: the inspector's Captions section on a clip with
  audio runs whisper.cpp's `whisper-cli` (built once with
  `scripts/build-whisper-cli.sh`, which needs git, CMake and a C++
  toolchain, or pointed at with `OPENCUT_WHISPER_CLI`) on the clip's audio
  at 16 kHz with DTW token timestamps (flash attention off), grouping the
  tokens into words. The chosen ggml model (tiny, base or
  small) is fetched from Hugging Face into `~/.opencut/models/` on first
  use, checked by size, with its SHA-256 and provenance written beside it.
  The words become caption elements with the web app's chunking and
  karaoke highlight, on a new Captions track in one undo step.
- **Stickers** from the web app's icon sets (`icons:<set>:<name>`, through
  the Iconify API) and flags (`flags:<code>`) render here: the SVG is fetched
  once into `~/.opencut/stickers/` and rasterised with resvg at the size it
  is shown, sized by the intrinsic size the web app stored. Bundle import
  keeps sticker elements, including those on graphic tracks (which land on
  the video lane); shapes and other graphics stay web-only and are reported.
- **Lottie animations** (`.json` or dotLottie `.lottie` files) import like
  other media and go on the timeline as image clips one animation long; a
  longer clip loops. Frames are drawn on the CPU by `crates/lottie` (velato
  and vello_cpu), the same code the web editor runs through `opencut-wasm`,
  at the size they are shown (256 to 2048 px on the longer side, in 64 px
  steps). Text layers, embedded images, time remapping, dashes and layer
  effects are not drawn; files over 32 MB or outside the other limits are
  refused with the reason.
- **Waveforms** on audio and video clips in the timeline: peaks are read
  through FFmpeg (8 kHz mono, one peak per 20 ms) on the render thread the
  first time a clip is shown, drawn as bars scaled by the clip's gain, with
  the gain curve over them (mid-height is 0 dB) and a MUTED badge on muted
  clips.
- **Bundled sounds** placed in the web app (Kenney Interface Sounds, Kenney
  UI Audio and Juhani Junkala's 512 retro sounds, all CC0) travel inside a
  project bundle as ordinary audio media, so they play here after *Import
  bundle…*. Freesound previews still do not: they stay web-only and are
  listed as skipped on import.
- **Fonts from Fontsource**: a text element's *Font files* row fetches
  the named family's TTF files (regular and bold, with italics where they
  exist) from jsDelivr into `~/.opencut/fonts/<id>/`, together with the
  package's LICENSE and a `provenance.json`, and the renderer picks them up
  at once. Families chosen in the web app's *Open licence* font tab render
  the same here once fetched. The directory is loaded at start too.
- **Effects** on visual clips, the same fourteen as the web app and rendered
  by the same WGSL shaders in `crates/effects`: blur, colour correction
  (brightness, contrast, saturation, hue, exposure, gamma, warmth, tint),
  lift/gamma/gain (three-way colour with red, green and blue offsets),
  curves (a five-point master curve and three-point red, green and blue
  curves), levels (input and output black and white points, gamma),
  shadows and highlights, chroma key, LUT, vignette, film grain, RGB split,
  pixelate, sharpen, mirror, glitch and kaleidoscope. Chroma key makes a
  green or blue screen (or any key colour, typed as hex) transparent: it
  compares each pixel's chroma (BT.709 Cb/Cr) with the key's, in units of
  the key's own saturation, so uneven light on the screen matters less;
  Strength sets how close counts as screen, Edge softness the ramp, and
  Spill removal takes the key's tint out of the edges it leaves. The LUT effect's **Load…**
  button reads a `.cube` or `.3dl` file or a HALD CLUT image
  (`crates/editor/src/luts.rs`, the same parser as the web app's
  `src/luts/parse.ts`), keeps at most 33 points a side, and stores the
  table in the effect itself (`name|size|base64`), so it travels in the
  project and in bundles; the shader samples a 2D texture of the table
  with trilinear or tetrahedral interpolation and an intensity slider. The colour formulas are the
  textbook ones (lift/gamma/gain as `(in × gain + lift × (1 − in))^(1/γ)`,
  Catmull-Rom curves), written here, not copied from any editor. The
  inspector's Effects section adds any of them and draws
  a row per setting from `crates/editor/src/effect_catalogue.rs`; number
  settings can be keyframed, grain and glitch move with the clip's time.
- **Safe-area guides** under the preview (`Guide` row: TikTok, Reels, Shorts)
  shade the part of a vertical frame the platform's own controls cover. The
  margins are approximate, taken from third-party guides, since no platform
  publishes a fixed zone.

### Shortcuts

| Keys | Action |
| --- | --- |
| `Space`, `K` | Play / pause |
| `←` / `→`, `Home` / `End` | Step a frame, go to start / end |
| `Shift-←` / `Shift-→` | Previous / next keyframe of the selected clip |
| `S` | Split at the playhead (selection, or everything under it) |
| `Delete` / `Backspace` | Delete the selection (selected path points first) |
| `Ctrl/Cmd-D`, `Ctrl/Cmd-A`, `Esc` | Duplicate, select all, clear selection |
| `Ctrl/Cmd-Z`, `Ctrl/Cmd-Shift-Z` (`Ctrl-Y`) | Undo, redo |
| `T` | Add a text element at the playhead |
| `P`, `Enter` | Draw a freeform mask path on the selected clip, close the path |
| `Ctrl/Cmd-I`, `Ctrl/Cmd-E`, `Ctrl/Cmd-S`, `Ctrl/Cmd-W` | Import, export, save, close project |
| `Ctrl/Cmd-B` | Split (alias of `S`) |
| `I` / `O`, `Shift-I` / `Shift-O`, `Ctrl/Cmd-Shift-X` | Set the in / out mark at the playhead, go to a mark, clear the marks |
| `↑` / `↓` | Previous / next edit point (any clip's start or end) |
| `Shift-Delete` | Ripple delete: the selection goes and the gaps close on every track |
| `Alt-Delete` | Ripple delete the stretch between the in and out marks |

#### Keymap presets

The split, delete, ripple-delete and clear-marks keys follow a preset:
`opencut` (the table above), `premiere` (`Ctrl-K` adds an edit), `final-cut`
(`Delete` ripple-deletes and `Shift-Delete` leaves the gap, `Alt-X` clears
the marks) or `resolve` (`Ctrl-B` splits, `Alt-X` clears the marks). Pick one
with `OPENCUT_KEYMAP=premiere` or by writing `{"preset": "premiere"}` to
`~/.opencut/keymap.json`; the web editor offers the same presets in its
shortcuts dialog. The other editors' keys come from their published
shortcut lists and cover only the actions both editors share.

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

## Shot detection

The inspector's **Shots** section (video clips) runs FFmpeg's `scdet`
filter over the clip's source span at a small size, takes the frames whose
difference from the previous one passes the chosen sensitivity (scores
run 0–100; High 10, Normal 20, Low 35), merges cuts closer than half a
second, and **Split at shot changes** splits the clip at every cut in one
undo step. The web editor scores frames itself (colour-histogram
difference, ten frames a second, threshold 30 on its own scale) and can
also bookmark the cuts; the cut picking is the same code on both sides
(`crates/editor/src/shots.rs`, `apps/web/src/shots/detect.ts`). Speed
changes are not followed when mapping cuts onto the timeline.

## Stabilising and smooth slow motion

Two inspector sections on video clips rewrite the clip's file through
FFmpeg into `~/.opencut/processed/` and point the clip at the copy, in one
undo step (undo plays the original again). The copy is the same length,
with the audio, so trims, speed and keyframes carry over; it is ordinary
media, so bundles carry it and the web editor plays it. A copy already made
with the same settings is reused. Both live in `crates/media/src/motion.rs`.

- **Stabilise** runs vid.stab in two passes (`vidstabdetect`, then
  `vidstabtransform` with `optzoom=1`, which zooms just enough to hide the
  moving edges, and a light sharpen). Strength sets the smoothing window:
  Light 10, Medium 20, Strong 40 frames. vid.stab is only in GPL FFmpeg
  builds (gyan.dev's full build has it); with another build the button says
  so. FFmpeg's own `deshake` was measured as a fallback and left out: on the
  synthetic test clips it did not reduce the shake (28.3 to 24.9 px of
  camera movement a frame, against vid.stab's 6.1; and 5.1 to 5.3 px on a
  gentler shake, against 0.2).
- **Smooth slow motion** makes in-between frames with `minterpolate`
  (motion-compensated, bidirectional) at 2× or 4× the frame rate. Slow the
  clip under Speed afterwards and it plays smoothly instead of repeating
  frames. It is slow to make on the CPU: on the development PC a 720p clip
  took about 6 seconds per second of footage at 2× (stabilising took less
  than the clip's own length).

The whole source file is processed, not only the part on the timeline.

## Face swap (local, with your own FaceFusion)

**Face swap** in the inspector (video and picture clips) puts the face from
a photo onto a face in the clip with [FaceFusion](https://github.com/facefusion/facefusion),
run on this computer: nothing is uploaded. ConzaCut never downloads
FaceFusion or its models. Install it yourself (its Windows installer, or
its guide: a conda environment named `facefusion` and `python install.py
directml` or `cuda@12`) into `~/.opencut/tools/facefusion`, `~/facefusion` or `~/Desktop/facefusion`,
or point `OPENCUT_FACEFUSION` at its folder and `OPENCUT_FACEFUSION_PYTHON`
at its Python, then restart. FaceFusion is OpenRAIL-AS and its models carry
their own licences, most of them non-commercial or research-only (GHOST is
Apache-2.0, but the face detection and recognition models it relies on are
not).

Choose the model (HyperSwap 1a fast, 1c detailed, or GHOST 2), which face
changes (the largest, the first or second from the left in the clip's first
frame, or all), and optionally sharpen the face (GFPGAN 1.4). Confirm that
the face is yours or that the person agreed, and that the people in the clip
agreed too; the confirmation is asked again for each swap. A video clip has
only the part it plays swapped (`media::swap_face_video`: FFmpeg cuts the
span, FaceFusion runs `headless-run` from its own folder, FFmpeg puts the
span's own sound back), and the clip then plays the result in one undo step.
The result is marked as generated (tool `faceswap`), so the credits list
it, and every export that shows it says so in its metadata (comment and
description).

## B-roll from a script

**B-roll for this script** in the media panel's voice-over section lays out
one stock picture per line of the script from the playhead, on a new
"B-roll" track with each picture's credit, in one undo step. The shot list
is `editor::plan_shots` (`crates/editor/src/broll.rs`), the same planner as
the web editor's Stock tab (`fixtures/broll/cases.json`): the voice-over's
lines and pauses, timed at 2.5 words a second (at least 1.5 s), each with
the line's three longest uncommon words as the search query. The source is
the Stock tab's choice (Openverse pictures, or Pixabay pictures or videos
when the server has a key). **Claude search words: on** asks the API worker
to have Claude choose the words instead (`POST /broll/plan`); it needs the
server's `ANTHROPIC_API_KEY` and the sharing passcode (`shareKey`), and the
timings stay the same.

## AI upscaling

The **AI upscale** section (video and image clips) runs Real-ESRGAN's
ncnn/Vulkan program on the GPU, which `scripts/get-realesrgan.sh` fetches
from the project's own release into `~/.opencut/tools/realesrgan/`
(about 45 MB with its models; `OPENCUT_REALESRGAN` points at it elsewhere).
The app looks for it at start-up, so restart after installing.

- **Fast (animation)** uses `realesr-animevideov3` at 2× or 4×. It is made
  for animation but is also the practical choice for video: about 0.2 s a
  frame at 640×360 on the development PC (Intel Arc 140V).
- **Photo (slow)** uses `realesrgan-x4plus`, the general model, which only
  works at 4× (2× is its result scaled down). It is sharper on real footage
  but took about 6.5 s a frame at 640×360 here, so it suits pictures and
  short clips.

A video clip has only the source span it plays upscaled: FFmpeg writes those
frames out as PNG at the clip's frame rate (they need disk space for a
moment, about 1 MB a frame at 1080p), the program upscales the folder, and
FFmpeg encodes them back with the same span of audio. The clip is then
pointed at the copy with its trims reset (`ReplaceMediaSpan`), in one undo
step. Outputs larger than 3840 px are refused before anything runs. An image
clip gets an upscaled PNG.

Stem separation (demucs) is not here: its model weights' licence is not
stated.

## Filler words

**Remove filler words** in the inspector's Captions section cuts "um",
"uh" and their kin out of the selected clip. The word timings come from
caption elements over the clip when there are any (the Transcribe button
or an imported word-timed subtitle file); otherwise the clip is transcribed
with whisper.cpp first, with a prompt full of hesitations so the model
keeps them rather than tidying them away. The filler lists are per
language (English, German, French, Spanish, Portuguese, Italian; "Any"
uses only the fillers that are no real word in any of them) and live in
`crates/editor/src/fillers.rs`, the same lists as the web app's
`src/fillers/detect.ts`. Each filler is cut with 40 ms of padding, never
into the neighbouring words, runs of fillers merge, and the gaps close as
the Silence section's ripple setting says, in one undo step. Longer words
"like" and "so" are left alone: they are words as often as fillers.
The captions are retimed with the cut, as below.

## Text-based editing

The inspector's **Transcript** section (audio and video clips) shows the
words of the captions over the clip. Click words to strike them, Shift-click
to strike a run, then **Cut struck words**: each run is cut from the middle
of the pause before it to the middle of the pause after it (80 ms past the
first or last word of the transcript), so what is left keeps a natural
pause, and the gaps close as the Silence section's ripple setting says. The
captions follow in the same undo step: struck words leave their captions
(a caption with none left goes), and with gaps closed on every track the
words after the cut move left with the picture, even inside a caption that
straddles it. The piece after the first cut stays selected so editing goes
on. The arithmetic is `crates/editor/src/transcript.rs`, the twin of the
web app's `src/transcript/cuts.ts`. Transcribe the clip to captions first.

**Find retakes** strikes the earlier takes of a line and false starts for
review, so cutting the struck words keeps only the last take. The shared
`retakes` crate splits the transcript into phrases at sentence ends and
pauses of 0.7 s or more; a phrase is an earlier take of one of the next
three (starting within 30 s) when both open with the same words (its first
three, or all of a shorter one) and at least 60% of its words come back in
order (longest common subsequence). Short chatter between the takes ("sorry,
let me redo that", up to eight words) and hesitations before the kept take
go with it. Inside a phrase, a run whose opening words repeat a few words
later ("I think we should I think we should go") is a false start when
nearly all of it comes back. Hesitations ("um", "ähm", …) are skipped when
comparing. No model is needed; the web app runs the same crate through
WASM. Checked against `fixtures/retakes/cases.json`.

## Noise reduction

The inspector's **Noise** section (audio and video clips) runs FFmpeg's
`afftdn` denoiser over the clip's media at the chosen reduction (6, 12, 20
or 30 dB), with the filter's noise floor set 8 dB above the level of the
clip's quietest windows (its own noise tracking is left off; it gave up on
clips with pauses), and writes the result as a WAV under `~/.opencut/processed/`,
added to the project as a new media item. An audio clip is pointed at it;
a video clip keeps its picture, gets a new audio clip with the denoised
file at the same place and trims, and has its own audio muted. All of that
is one undo step. The web editor does the same with its own spectral
subtraction (`src/denoise/spectral.ts`: a 1024-point STFT, the steady
background taken as the 30th percentile of each bin over the clip,
subtracted from the time-smoothed power with a floor set by the
reduction), separating a video clip's audio first. Both need pauses in
the clip to measure the background; a clip that never goes quiet is
reduced less, never damaged. Neither side
uses RNNoise: `afftdn` ships with FFmpeg and needs no model file, and the
web pass keeps the app free of a WASM dependency.

## Animations

The inspector's **Animation** section (any clip with a picture) picks a
way **In** (fade, pop, grow, shrink, slide up, down, left or right, drop
and bounce, spin, and on text typewriter or word by word), a way **Out**
(the same moves, leaving) and a **Loop** (pulse, shake, wobble, float, or
Ken Burns, a slow push in and drift over the whole clip), each with its
length. They are the `animIn`, `animOut` and `animLoop` params the web
editor's Animation tab sets, worked out while the clip plays by
`editor::clip_animation` on top of its keyframes, so they follow the clip
when it is trimmed. A typewriter draws the letters not shown yet clear, so
the text does not move as it types.

## Resize project

With no clip selected, the inspector's **Project** section has the canvas
presets and a **Content** row choosing what a new canvas size does to the
content (`editor::resize_project`, `crates/editor/src/resize.rs`, the web
editor's `resize/plan.ts`; the export panel's platform fixes follow the same
choice). The old picture is scaled about the centre: **Fit** shows all of
it, **Fill** covers the new canvas with the edges cropped, **Keep sizes**
keeps pixel sizes, and **Canvas only** changes the canvas alone, as before.
Positions and their keyframes, outline width, shadow offset and blur, and
mask feather and stroke scale with it; each clip's own scale allows for its
base size following the canvas (pictures fit it, text follows its height),
and clips inside compounds are re-fitted too. Captions are placed again
centred at the bottom: a quarter of the height up on a tall canvas, clear of
the platforms' buttons, 5 % otherwise. One undo step. The web editor also
re-wraps caption lines to the new width; the desktop keeps their lines.

## Audio effects

An audio or video clip's inspector has **Audio effects**: presets (Voice
clarity, Podcast, Bass boost, Bright, Warm, Loud and even), **Bass**,
**Mids** and **Treble** (a 120 Hz low shelf, a 1 kHz bell and an 8 kHz high
shelf, ±12 dB), a **Compressor** (threshold and ratio; linked channels,
10 ms attack, 150 ms release, 6 dB soft knee, make-up gain, and a safety
limiter at -0.1 dBFS when the limiter is off) and a
**Limiter** (ceiling). They are kept as `audio.*` params and the clip's
media is not changed: when a clip has any, its audio is prepared through
`voice::apply_audio_fx` into the processed-audio cache (with any speed
change), which both the preview mixer and the export play, so what you hear
is what is exported. The web editor runs the same code as WASM.

## Retouch

**Retouch** is in the effects catalogue (Smooth, Brighten, Even tone,
Radius). The shared `retouch` shader (`crates/effects`) weighs each pixel
as skin by its chroma (an ellipse around skin tones in Cb/Cr, faded out in
the dark), runs an edge-preserving bilateral blur over the centre and two
rings of eight taps (Radius / 2 pixels apart at 1080 high), moves skin
towards the blur, gives it the blur's colour under its own light and shade
(Even tone), and lifts it a little (Brighten). Pixels that are not skin are
left exactly as they are. It needs no model: skin-coloured things that are
not faces (hands, wood) are softened too, unless **Area** is "Faces
only". **Faces → Retouch faces** in the inspector (video and
picture clips) finds the clip's faces, 5 looks a second over the part a
video plays, with FaceFusion's YOLO face model in FaceFusion's own Python
(`scripts/find_faces.py`; needs a FaceFusion install that has downloaded
the model), keeps them on the clip as a face track (`faces.track`, the
`facetrack` crate) and sets the Retouch effect to faces only, in one undo
step. Each frame the planner looks up the faces at the source time, the
renderer turns them into soft ovals in frame pixels (up to three) and the
shader retouches skin inside them only. The web editor finds faces with
MediaPipe's BlazeFace and renders the same track the same way. The web
editor runs the same shader through the WASM compositor.

## Face blur

**Faces → Blur faces** in the inspector finds the clip's faces the same
way and turns on the **Face blur** effect (catalogue: Style blur, pixelate
or black; Strength; Size; Soft edge), in one undo step. The shared
`face-blur` shader (`crates/effects`) hides each face's oval (the same
ovals as face-only retouch, grown by Size, up to three) with a Gaussian
blur in two passes (across, then down), blocks that are each the mean of
4 x 4 taps, or black, fading out over the oval's outer Soft edge. The reach
is Strength x half the grown oval's smaller radius, so a face far away and
a close-up are hidden alike. Frames without faces, and everything outside
the ovals, are left exactly as they are. Checked against
`fixtures/face-blur/cases.json` on a real GPU; the web editor draws the same
pixels through the WASM compositor.

## Multicam

Select two or more video clips (each on its own track, the one to line up
to first) and click **Sync by audio** in the multi-selection inspector.
Each clip's sound over the part it plays is read with FFmpeg (mono, 8 kHz),
turned into an onset envelope (log loudness rises, 200 a second) and
cross-correlated with the first's by FFT (`multicam` crate); each moves by
its lag (all later if one would start before zero), the clips become angles
1, 2, 3… of a group (`multicam.group`, `multicam.angle`), and all but angle
1 are muted. A score under 0.2 is refused. With one angle selected, the
inspector's **Multicam** row has a button per angle: it shows that angle
from the playhead on (also while playing) by adding a cut to the group's
list (`multicam.cuts`) and rewriting every angle's opacity as stepped keys
(a key 6 ms before each cut, past the 5 ms keyframe snap), so the angles
stay ordinary clips and render everywhere. **Clear cuts** shows angle 1
throughout. The web editor does the same through WASM.

## Rich text

A text element's inspector has a **Selection** row under the Text field:
select words in the field, then Bold, Italic, Smaller (75%), Bigger (150%),
Biggest (200%), a colour swatch, or Clear styles. The styled ranges are
kept in the `richText` param with the text they were made for
(`richtext` crate); when the text is edited anywhere, the unchanged start
and end keep their styles and letters typed inside a styled run take its
style. The rasterizer builds one cosmic-text run per change of look (rich
style, then caption keyword colour and size, then the spoken word), so
pieces share the line's baseline and a larger piece drops the baseline and
rises above it; the raster grows to hold glyphs taller than their line
(this also stops big caption keywords being clipped). The web editor draws
the same layout (the same text at 640×360 lands within 3 pixels).

## Deflicker

A video clip's inspector has **Deflicker** with Off, Light, Medium and
Strong, for timelapse flicker and LED or fluorescent light. The first
strength other than Off reads the part of the file the clip plays with
FFmpeg (160 px wide, 30 frames a second) and measures each frame's mean
Rec. 709 luma with the shared `deflicker` crate; the levels go on the clip
(`deflicker.levels`, tagged with the media id, so a replaced file is not
corrected with old levels) in one undo step with the strength. When a frame
is drawn, its gain is the average level of its neighbours (5, 11 or 25
frames, leaving out ones more than half again brighter or darker, so cuts
and lights switched on stay) over its own, clamped 0.5 to 2, and the shared
compositor multiplies the source by it before effects. Changing the
strength later needs no new measuring. The web editor measures and applies
it the same way (WASM).

## Motion blur

Every visual clip's inspector has **Motion blur** under Transform: 0 (off)
to 100, how much of a frame the shutter stays open (100 the whole frame, 50
the film look). Not keyframed. Where the clip moves during the shutter
(keyframed position, scale or rotation, or a one-click animation), the
planner works out its transform half a shutter before and after the frame
(`editor::motion_blur`), the renderer turns both into quads, and the shared
compositor draws the layer at evenly spaced points between them in one pass
and averages them with premultiplied alpha: one draw per pixel of the
farthest corner's travel, plus one, at most 32
(`compositor::motion_blur_samples`). A clip that holds still is drawn once,
as before. Effects, masks and the outline and shadow are applied after the
blur. The web editor sends the same two quads to the same compositor (WASM),
so the same clip gives the same pixels in both.

## Curved text

A text element's inspector has **Curve** (with the other text numbers,
keyframable): -100 to 100, 0 flat. Above 0 the line bends up like an arch
(the ends drop), below 0 like a smile; at 100 the text's width wraps 350
degrees. The renderer draws the flat text (glyphs, background box,
highlights, underline) as before and bends the whole raster along the arc
with `textwarp::warp` (bilinear, alpha-weighted), centred where the flat
text was; letters stretch a little on the outside of the bend. The web
editor runs the same crate as WASM, so both draw the same curve (the same
text at 640×360 lands within 2 pixels in both).

## Corner pin

A video, picture or sticker clip's inspector has **Corner pin**: **Pin
on/off**, **Edit on canvas** (video and picture clips: the four corners
appear on the preview as handles; drag one to move that corner, a drag that
would cross or fold the corners in is refused) and **Reset corners** (back
to the clip's own corners, keyframes dropped). With the pin on, each
corner's **X** and **Y** are set in percent of the clip's own box before
its rotation (0, 0 is its top left; 100, 100 its bottom right), so moving,
scaling or turning the clip carries the pin; each can be keyframed, and a
drag on a keyframed corner keys it at the playhead. The params are
`cornerPin.enabled` and `cornerPin.<topLeft|topRight|bottomRight|bottomLeft><X|Y>`
(`editor::read_corner_pin`). The shared compositor draws the clip through
the perspective map from the unit square to the four corners
(`compositor::pin_matrix`, Heckbert's square to quadrilateral, inverted
per pixel in the layer shader, with soft edges); crossed, dented or flat
shapes draw the clip without its pin. The web editor uses the same
compositor (WASM) and reads the same params. Masks are applied in the
clip's unpinned box.

## Follow motion

Select a video and a sticker, text, picture or clip placed over something
in it at the playhead; the multi-selection inspector's **Track and follow**
tracks the spot under the follower's centre from the playhead until either
clip ends, and gives the follower position keys that keep it there. Frames
are decoded at 320 pixels wide, up to 30 a second, and followed by
`crates/tracking` (the web editor runs the same code as WASM): template
matching on half-size then full-size frames with normalised
cross-correlation and sub-pixel peaks; a starting box inside a flat area
grows until it takes in some edges; a weak match counts as lost and holds
the last position until the spot is found again. Keys are simplified to
within a canvas pixel of the path; one undo step. The video's own position
and scale are read as they are, not its keyframes, and masks cannot follow
yet (they have no keyframes).

## Cover

The Project section's **Cover** rows make the project's cover picture: a
title, a style (**Bold**: big white words with a dark outline, low; **Band**:
white words on a dark box, low; **Top**: yellow words with a dark outline,
at the top), a frame (**Use playhead**, or **Suggest a frame**, which scores
16 evenly spaced frames for sharpness, colour, exposure and contrast), and
**Embed in MP4**. **Save PNG** writes it at the canvas shape, 16:9
(1280×720) or 9:16 (1080×1920); another shape is the project re-fitted with
fill. The cover is the project with the title as a text clip on a new top
track (`editor::cover_project`), drawn by the normal renderer
(`render::render_cover`), so the title looks as text does in the editor.
MP4 exports, here and from `opencut-render`, get it as an attached picture
(`media::attach_cover`, a stream-copy remux), which players and file
browsers show as the file's cover. The title layout and frame score are the
web editor's (`cover/layout.ts`), checked against `fixtures/cover/cases.json`.

## Split screen

With 2 to 4 video or picture clips selected, the inspector's **Split
screen** section offers the layouts for that many clips (side by side,
top and bottom, three columns or rows, one big and two small, a grid of
four). One click (`editor::split_screen_commands`, the web editor's
`split-screen/layout.ts`) scales each clip to cover its part of the canvas,
centres it there, clears its transform keyframes and replaces its masks
with a rectangle cropped to the part, in the order the clips were selected;
one undo step.

## Colour match

With 2 or more video or picture clips selected, the inspector's **Colour
match** button makes the others take on the colours of the first one
selected. The reference is measured at the playhead (its middle when the
playhead is outside it) and each other clip over five frames, by the mean
and spread of each Lab channel (`editor::colour_match_commands`,
`crates/editor/src/colour_match.rs`, the web editor's
`colour-match/match.ts`; Reinhard transfer with the spread ratio held to
0.5 to 2). The transfer is baked into a 17-point table in a **LUT** effect
named "Colour match" on each clip, so its intensity turns the match down; a
clip matched again has that effect updated. One undo step. The reference
is measured from its source picture, before its own effects. Video frames
need FFmpeg.

## Auto zoom

The inspector's **Auto zoom** section (video and picture clips) plans
punch-ins from the captions over the clip with `editor::plan_punches`
(`crates/editor/src/auto_zoom.rs`, the web editor's planner): a sentence
starts after . ! ? or a 0.7 s pause, sentences alternate between no zoom
and the normal zoom, and one with a keyword, a number or "!" takes the
emphasis zoom (Subtle 1.08 / 1.15, Medium 1.15 / 1.3, Strong 1.25 / 1.5).
They are stored as `zoom.punches` with `zoom.focusX` / `zoom.focusY` (the
point zoomed into; the web editor can find the face) and `zoom.ramp`, and
the renderer scales the drawn frame about the focus as the clip plays, on
top of its keyframes and auto-reframe.

## Corners, outline and shadow

The inspector's **Style** section rounds a clip's corners (a percent of
half its shorter side; not on text, whose box has its own), draws an
outline around its shape, and puts a drop shadow or glow under it
(**Drop shadow** and **Glow** set one up in a click). They are the
`cornerRadius`, `outline.*` and `shadow.*` params the web editor's Style
tab sets, and the shared compositor draws them after the clip's effects
and mask: the corners in the layer shader, the outline and the shadow as
passes in `crates/compositor/src/shaders/style.wgsl`, on the whole frame so
they are never cut off at the clip's edge.

## Freeze frame

**Freeze** in the timeline toolbar (and **Freeze frame** in a clip's
inspector) holds the frame at the playhead of a video clip for three
seconds: the selected clip if it is under the playhead, else the topmost
one. FFmpeg writes that frame as a full-size PNG under
`~/.opencut/processed/` on a background thread; then, in one undo step,
the picture is added and `Command::FreezeFrame` splits the clip, puts an
image clip with the clip's look (position, scale, effects, masks, and
animated values fixed at that moment) in between, and moves everything
that starts at or after the playhead on unlocked tracks three seconds
later, so separated audio and captions stay in step. Clips that span the
playhead on other tracks (music, say) play on. The web editor's toolbar
button and `timeline.freezeFrame` do the same.

## Audio fades and pan

The inspector's **Audio** section has **Fade in** and **Fade out** (seconds,
up to 10 each, scaled down together on a clip too short for both) and
**Pan** (-1 left to 1 right), the `fadeIn`, `fadeOut` and `pan` params the
web editor's Audio tab sets. Fades are straight ramps from and to silence
on top of the volume and its keys; pan follows Web Audio's two-channel
StereoPannerNode (at 0 nothing changes), so playback here, the export
(`afade` and `pan` filters) and the web preview and export all sound the
same.

## Track mixer

Each audio and video track has a mute (**m**) and a solo (**s**) on its
header. With nothing selected, the inspector's **Mixer** section has a
strip per such track: mute and solo, **dB** (the track's volume, −60
silent to +12) and **Pan** (−1 left to 1 right), applied after each clip's
own volume and pan, and a meter (peak bar and momentary LUFS) while the
preview plays. While any track is soloed only soloed tracks are heard; mute
wins. The shared `trackmix` crate works this out for both editors (the web
through WASM); the live mixer, the export (track volume folded into the
clip's gain and volume keys, a second `pan` filter for the track) and web
bundles (`solo`, `volume`, `pan` on audio and video tracks) all carry it.
The desktop's track meters are taken in the mixer, which runs up to 0.2 s
ahead of the sound. Checked against `fixtures/trackmix/cases.json` and with
real tones (`crates/render/tests/track_mix.rs`).

## Audio crossfade

Select an audio or video clip and, under **Audio**, click **Crossfade into
next** (or type a length in **Crossfade (s)** and press Enter) to join it to
the clip that starts where it ends on the same track. **Equal power** (the
default) keeps the loudness steady through the join; **Linear** is a
straight ramp, about 3 dB quieter in the middle. **Remove crossfade** takes
it off. It is one undo step.

The timeline itself never overlaps: for playback both clips run over the
cut into their spare source audio (the outgoing clip's trimmed end, the
incoming clip's trimmed start), half each side when both have it, so they
play together for the crossfade's length. Where a clip has too little spare
audio (two whole files butted together have none) the command trims the two
to make room and moves the incoming clip and those after it on the track
left to close the gap; nothing is lost, the trimmed audio is what plays under
the crossfade. The shared `crossfade` crate works this out in ticks for both
editors (the web through WASM), stored as the clip's `crossfade` and
`crossfadeCurve` params; clips with a speed change cannot be crossfaded, and
a crossfade is at most 10 s. The live mixer follows the curve and the export
uses FFmpeg's `afade` quarter-sine for equal power. Checked against
`fixtures/crossfade/cases.json` and, with real tones, through the live mixer
and an export (`crates/render/tests/crossfade.rs`).

## Enhance voice

The inspector's **Enhance voice** button (audio and video clips) cleans
up speech in one step, through `voice::enhance_voice`
(`crates/voice/src/enhance.rs`): an 80 Hz high-pass, noise reduction
(the web editor's spectral subtraction, run only when the clip has pauses
well under the speech to learn the noise from), a 2 dB cut at 250 Hz and
a 3 dB presence lift at 3.5 kHz, a de-esser on the 5 kHz-and-up band, a
3:1 compressor with a downward expander for the pauses, and loudness to
-16 LUFS under a -1 dBFS limiter. The result is written as a WAV under
`~/.opencut/processed/` and the clip plays it as with the voice effects;
the status line gives the loudness before and after. The web editor runs
the same crate as WebAssembly.

## Voice effects

The inspector's **Voice** section (audio and video clips) changes the
clip's voice: Deeper and Higher (4 semitones), Monster and Chipmunk (9),
Robot, Echo, Radio and Hall. The audio is decoded at 48 kHz, each channel
goes through the `voice` crate (`crates/voice`: a phase vocoder plus
resampling for pitch, zero-phase resynthesis at about 110 Hz for the
robot, a 300 ms feedback delay, a 300-3400 Hz band with drive, a
Schroeder reverb), the channels are kept under full scale together, and
the result is written as a WAV under `~/.opencut/processed/` the same
length as the original. The clip then plays it as with noise reduction
(a video clip gets its own audio clip and is muted); undo puts the
original back. The web editor runs the same crate as WebAssembly.

## Beat markers

The inspector's **Beats** section (audio and video clips) finds the tempo
of the selected clip from its onsets (log-energy flux, 60–200 BPM with a
preference for 120, written here; `crates/editor/src/beats.rs` is the same
code as the web app's `src/beats/track.ts`) and puts a marker on every
beat, every second beat or every bar on the ruler. Clips snap their start
or end to a marker within 8 px while dragged. Markers are not saved with
the project; **Clear markers** removes them. The web editor places beats
as bookmarks instead, which its timeline already snaps to.

## Loudness

The inspector's **Loudness** section (audio and video clips) measures the
selected clip after EBU R128 / ITU-R BS.1770-4: integrated loudness
(K-weighted, gated), loudness range, true peak (4× oversampled) and the
loudest momentary and short-term windows, on the clip's source after
trims and before its own volume. **Normalise** sets the clip's volume so
the integrated loudness meets the chosen target (streaming −14, podcast
−16, broadcast −23 or loud −11 LUFS) without the true peak passing −1
dBTP, in one undo step. The arithmetic is the shared `crates/loudness`
crate, written from the standard (the web app's `src/loudness/r128.ts` is
the same arithmetic, and its mix mastering and meter run the crate through
WASM); channels are weighted equally, so surround material is measured as
if every channel were a front channel.

**Whole-mix loudness at export.** The export dialog's **Loudness** row is
off by default (the mix is exported as it plays) or one of the delivery
targets (−14, −16, −23, −11 LUFS). With a target, the mix is first rendered
by FFmpeg exactly as the export would mix it, measured, given one gain to
the target and passed through a look-ahead true-peak limiter (the gain
each sample needs is held 5 ms ahead, released over 50 ms and smoothed,
the same on both channels, aiming 0.1 dB under −1 dBTP); limiting takes a
little loudness off, so up to three passes raise the gain until the result
is within 0.2 LU of the target. The mastered mix is then encoded with the
picture, and the finished dialog says what was done ("Mix loudness: −24.3 →
−14.1 LUFS (+14.5 dB, peaks limited), true peak −1.1 dBTP"). The AAC
encoder can move the true peak a few hundredths of a decibel.

**Meter.** While the preview plays, the transport shows a peak bar (green,
yellow from −6 dBFS, red from −1) and the momentary (400 ms) and
short-term (3 s) loudness of what the audio device is playing.

## Exporting captions

**Captions…** in the header writes the timeline's captions (the text elements
on visible text tracks, in order) as a subtitle file. The extension you give
the file picks the format: `.srt` (SubRip), `.vtt` (WebVTT, with an inline
timestamp before each word so players can style the words spoken so far) or
`.ass` (Advanced SubStation Alpha, with one style per distinct look and `\k`
karaoke tags carrying each word's duration). Plain text elements export too,
without word tags. The same writers sit behind `opencut-render --captions
<file>` and the web editor's Captions panel and `captions.export` API method,
and the three produce identical files for the same project. ASS placement is
derived from each caption's position: the lower third of the canvas maps to
the bottom alignments, the upper third to the top ones, the rest to the
middle (an approximation of the editor's free placement).

The desktop draws captions itself when it renders, so burning subtitles in
needs no libass pass; the bundled FFmpeg does include libass if you want to
burn an exported `.ass` onto a finished video yourself
(`ffmpeg -i out.mp4 -vf ass=captions.ass burned.mp4`).

## Rendering from the command line

`apps/render-cli` builds `opencut-render`, the same renderer without the
window: it takes a project directory (`--project ~/.opencut/projects/<id>`)
or a bundle (`--bundle promo.opencut.zip`) and writes a video (`--out
promo.mp4`, `--format`, `--quality`, `--codec h264|hevc|av1|vp9`,
`--bitrate <Mbps>`, `--hardware`, `--no-audio`, `--loudness
streaming|podcast|broadcast|loud|<LUFS>` for whole-mix loudness), applies a platform
preset (`--preset youtube-shorts`, which sets the canvas and frame rate and
encodes H.264 High with a closed GOP and faststart), overrides the canvas
and rate (`--size 1080x1920`, `--fps 30`), writes one still (`--frame-at
2.5 --out frame.png`), samples stills for QA (`--stills every=1`,
`--stills at=0.5,2.3`, `--stills-dir qa/`) and tiles them into a contact
sheet (`--contact-sheet qa/sheet.png`). The export loop lives in
`crates/render/src/export.rs` and is shared with the desktop's export
dialog. Fonts and stickers the desktop fetched under `~/.opencut` are used.

## Not there yet

- The media browser does not play files before they go on the timeline;
  the web editor's media panel does.

- Shapes and other graphics from the web editor are read from project
  files but not rendered. Transcription runs through `whisper-cli` on the
  CPU; there is no GPU path or language picker yet (the language is
  detected). A path point's curve handles are set while placing it (drag)
  and follow the point afterwards; they cannot be dragged on their own (the
  web app has no handle dragging either).
- Project bundles move projects between the two editors: **Save bundle…**
  in the header writes `<name>.opencut.zip` (the project in the web app's
  JSON shape plus copies of its media), and **Import bundle…** on the
  projects page reads one, from here or from the web app's *Export project…*
  menu item. Web-only content (stickers, graphics, effect tracks, library
  sounds, extra scenes) is left out of an import and listed on the page.
  Every bundle also holds `assets.json`, the asset manifest: each media
  file, library sound, sticker and font with where it came from, its
  licence (or that none is recorded), source and credit line. *Copy
  credits* in the Stock tab copies its credit lines.
- Templates are bundles too. The projects page lists five starter
  templates (the same files the web app serves from `/templates/`, built
  into the app; rebuild them with `cargo run -p editor --example
  build_templates`) and your own: **Save as template** in the editor's
  header writes the project and its media to `~/.opencut/templates/`.
  Using one makes a new project from it and fetches its fonts from
  Fontsource.
- "Open in ConzaCut" links (`https://…/open?bundle=<url>`,
  `conzacut://open?bundle=<url>` or a bundle URL) open from **Open copied
  link** on the projects page or as the first command-line argument
  (`opencut-desktop "<link>"`, which also takes a bundle path): the bundle
  is downloaded into `~/.opencut/downloads/`, imported and opened. See
  `apps/web/reference/open-in-conzacut.md`.
- **Share link** in the header uploads the project and its media to the
  API worker (R2, kept 7 days) and copies an Open in ConzaCut link. It
  needs the server's sharing passcode in `shareKey` of
  `~/.opencut/settings.json` or `OPENCUT_SHARE_KEY`. To use a self-hosted
  server (`infra/self-host`), set `apiUrl` (for example
  `https://cut.example.com/api`) and `webUrl` (the site links open in) in
  the same file, or `OPENCUT_API_URL` and `OPENCUT_WEB_URL`.
