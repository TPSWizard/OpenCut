# crates/

The Rust core of OpenCut. These crates hold the editor logic that is shared
between the web app (compiled to WebAssembly) and the desktop app (linked
natively). They are members of the root Cargo workspace next to
`apps/desktop`; shared dependency versions live in `[workspace.dependencies]`
in the root `Cargo.toml`.

| Crate                  | Package name   | What it does                                                                 |
| ---------------------- | -------------- | ---------------------------------------------------------------------------- |
| `crates/bridge`        | `bridge`       | Proc-macro crate providing `#[export]` (see below).                          |
| `crates/time`          | `time`         | `MediaTime` (120 000 ticks/second), `FrameRate`, timecode format/parse.      |
| `crates/editor`        | `editor`       | Project document, timeline commands, undo history, frame and audio planning, project files on disk. |
| `crates/media`         | `media`        | Probe, decode (video to BGRA frames, audio to f32) and encode by driving the FFmpeg programs over pipes; images via `image`. |
| `crates/gpu`           | `gpu`          | wgpu device/queue context, blit and fullscreen shaders, texture upload/readback. |
| `crates/effects`       | `effects`      | GPU effect pipeline (gaussian blur).                                         |
| `crates/masks`         | `masks`        | Signed-distance-field masks with feathering (jump flooding on the GPU).      |
| `crates/compositor`    | `compositor`   | Layer compositor: texture pool/store, blend modes, mask and layer shaders.   |
| `crates/render`        | `render`       | `Renderer` (frame plan → pixels through `media`, cosmic-text and `compositor`; builds blur passes and rasterises mask shapes) and `Mixer` (audio clips → samples) for native front ends. |
| `crates/wasm`          | `opencut-wasm` | `cdylib` that re-exports the crates above for the web app via wasm-bindgen.  |

Dependency graph (arrows point at dependencies):

```
opencut-wasm ──► compositor ──► effects ──► gpu
      │              │            ▲
      │              └──► masks ──┘
      └──► time ──► bridge (proc-macro)

render ──► compositor, gpu, media, editor, cosmic-text
editor ──► time
media  ──► time, image           (and the ffmpeg/ffprobe programs at runtime)
apps/desktop ──► render, editor, media, time, gpui, cpal
```

## The `#[export]` bridge macro

Core functions are plain Rust. Annotating one with `#[export]` from `bridge`
makes it reachable from JavaScript when the `wasm` feature is on and is a
no-op otherwise:

```rust
use bridge::export;

#[export]
pub fn round_to_frame(RoundToFrameOptions { time, rate }: RoundToFrameOptions) -> Option<MediaTime> { ... }
```

With `--features wasm` this expands to
`#[wasm_bindgen(js_name = "roundToFrame")]`. Exported functions must take a
single options struct (not positional arguments) so the JS side gets a
named-argument API; the option structs derive `tsify_next::Tsify` so
`wasm-pack` emits matching TypeScript types. `#[export]` on a `pub const`
additionally emits a JS getter of the same name.

## The editor document (`crates/editor`)

`editor::Project` is the document: settings (frame rate, canvas, background),
media items (referenced by path), tracks stored top to bottom (overlays, the
main video track, audio) and their elements. Element parameters are a flat
map with the same keys as the web Editor API (`transform.positionX`,
`opacity`, `content`, `fontSize`, …), and times are `MediaTime` ticks. An
element also carries `animations` (keyframe channels by parameter path, with
times relative to the element's start; `editor::animation`), `effects` (GPU
effects such as `blur` with their own parameters) and `masks` (shapes that
limit where a video or image shows), all in the web app's JSON shape.
`Element::params_at` resolves keyframes at a time; effect parameters can be
keyed under `effects.<id>.params.<key>`.

Every edit is an `editor::Command` applied with `editor::apply`; `editor::Editor`
wraps a project with undo and redo. `plan_frame` says which layers are on
screen at a time, how they are transformed, which effects apply and which
mask shape clips them, all with keyframes resolved, plus the main-track
backdrops of a blurred background; `plan_audio` says which
parts of which files are audible and at what gain (`AudioClipPlan::gain_at`
follows `volume` keyframes). `ProjectStore` reads and writes
`<root>/<id>/project.json` (format version 1).

## Media I/O (`crates/media`)

Rather than linking FFmpeg's libraries, `media` runs the `ffmpeg` and
`ffprobe` programs and talks to them over pipes: `ffprobe -print_format json`
for probing, `ffmpeg … -f rawvideo -pix_fmt bgra -` for frames,
`-f f32le` for audio, and raw BGRA on stdin plus an `amix` filter graph for
encoding. That keeps the crate free of unsafe code and bindings, decodes the
same formats on every platform, and needs no headers at build time.
`Ffmpeg::locate()` looks in `OPENCUT_FFMPEG_DIR`, the pinned build under
`.cache/media/*/bin` (from `moon run media:setup`, which downloads the
prebuilt in `crates/media/setup/ffmpeg.json` or builds the pinned source),
next to the executable, then `PATH`. Images are decoded in-process with
`image`. Set `OPENCUT_FFMPEG_TRACE=1` to print every encoder command line
as it is spawned.

## How the web app consumes the core

`apps/web` depends on the `opencut-wasm` npm package (currently `^0.2.10`),
which is the `wasm-pack` output of `crates/wasm`. The `cdylib` re-exports
everything from `time` and, on `wasm32`, the GPU-facing wrappers in
`crates/wasm/src/{gpu,compositor,effects,masks,perf}.rs`. `gpu` is built with
its `wasm` feature (`wgpu/webgl`) so rendering works on WebGL2 as well as
WebGPU.

To build it from this repository:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked

# either
moon run wasm:build
# or, from the repo root
wasm-pack build crates/wasm --target bundler --out-dir pkg --release
```

The output lands in `crates/wasm/pkg/` (git-ignored). To point `apps/web` at
the local build instead of the published package:

```bash
cd crates/wasm/pkg && bun link
cd ../../../apps/web && bun link opencut-wasm
```

`crates/wasm/pkg/opencut_wasm.d.ts` is the contract the web app codes
against; compare it with `apps/web/node_modules/opencut-wasm/opencut_wasm.d.ts`
before publishing a new version.

## How the desktop app consumes the core

`apps/desktop` is a GPUI app and links the crates directly as Cargo path
dependencies declared in `[workspace.dependencies]`, e.g.
`render = { workspace = true }`. No `wasm` feature is enabled, so `#[export]`
does nothing and the functions are ordinary Rust. The app keeps an
`editor::Editor`, renders previews and exports through `render::Renderer` on
a background thread, and plays audio through `render::Mixer`.

## Commands

```bash
# Native type-check of every crate except the (slow, GPUI) desktop app
cargo check --workspace --exclude opencut-desktop

# Unit and integration tests for the core crates (FFmpeg- and GPU-dependent
# tests skip themselves when those are missing)
cargo test --workspace --exclude opencut-desktop     # or: moon run wasm:test

# Type-check the wasm target without producing a package
cargo check -p opencut-wasm --target wasm32-unknown-unknown   # or: moon run wasm:check

# Desktop app (compiles GPUI from source; 15-30 min cold)
cargo check -p opencut-desktop                        # or: moon run desktop:check
cargo test -p opencut-desktop                         # or: moon run desktop:test

# Reproduce an export outside the app: exports <root>/<id>/project.json at
# its canvas size into a temporary directory and fails with ffmpeg's message
OPENCUT_EXPORT_PROJECT=~/.opencut/projects/<id> cargo test -p opencut-desktop exports_the_project_named
# add OPENCUT_EXPORT_OUTPUT=/path/out.mp4 to keep the file
```

## Adding a new crate

1. Create it under `crates/<name>/` and add `'crates/<name>'` to `members`
   in the root `Cargo.toml` (members are listed explicitly because
   `crates/media/setup` is not a crate).
2. Add `<name> = { path = "crates/<name>" }` to `[workspace.dependencies]`.
3. Depend on `bridge` and annotate public entry points with `#[export]` if
   the web app should reach them.
4. Re-export it from `crates/wasm/src/wasm.rs` so it ships in `opencut-wasm`.

## Windows note

`wgpu-hal 29` and `gpu-allocator 0.28` must resolve to the same `windows`
crate version. GPUI pulls in `windows 0.61`, and if Cargo reuses that for
`gpu-allocator` the build of `wgpu-hal` fails with "multiple different
versions of crate `windows`". `Cargo.lock` pins `gpu-allocator` to
`windows 0.62.2`; keep it that way when updating dependencies.
