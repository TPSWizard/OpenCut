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
| `crates/gpu`           | `gpu`          | wgpu device/queue context plus blit and fullscreen shaders.                  |
| `crates/effects`       | `effects`      | GPU effect pipeline (gaussian blur).                                         |
| `crates/masks`         | `masks`        | Signed-distance-field masks with feathering (jump flooding on the GPU).      |
| `crates/compositor`    | `compositor`   | Layer compositor: texture pool/store, blend modes, mask and layer shaders.   |
| `crates/wasm`          | `opencut-wasm` | `cdylib` that re-exports the crates above for the web app via wasm-bindgen.  |
| `crates/media`         | –              | Not a Rust crate: only `setup/` (a TypeScript setup script). Not a member.   |

Dependency graph (arrows point at dependencies):

```
opencut-wasm ──► compositor ──► effects ──► gpu
      │              │            ▲
      │              └──► masks ──┘
      └──► time ──► bridge (proc-macro)
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
`time = { workspace = true }`. No `wasm` feature is enabled, so `#[export]`
does nothing and the functions are ordinary Rust. The Timeline panel
(`apps/desktop/src/panels/timeline.rs`) uses `time::{FrameRate, MediaTime,
format_timecode}` to lay out its ruler.

## Commands

```bash
# Native type-check of every crate except the (slow, GPUI) desktop app
cargo check --workspace --exclude opencut-desktop

# Unit tests for the core crates
cargo test --workspace --exclude opencut-desktop     # or: moon run wasm:test

# Type-check the wasm target without producing a package
cargo check -p opencut-wasm --target wasm32-unknown-unknown   # or: moon run wasm:check

# Desktop app (compiles GPUI from source; 15-30 min cold)
cargo check -p opencut-desktop                        # or: moon run desktop:check
```

## Adding a new crate

1. Create it under `crates/<name>/` and add `'crates/<name>'` to `members`
   in the root `Cargo.toml` (members are listed explicitly because
   `crates/media` has no `Cargo.toml`).
2. Add `<name> = { path = "crates/<name>" }` to `[workspace.dependencies]`.
3. Depend on `bridge` and annotate public entry points with `#[export]`.
4. Re-export it from `crates/wasm/src/wasm.rs` so it ships in `opencut-wasm`.

## Windows note

`wgpu-hal 29` and `gpu-allocator 0.28` must resolve to the same `windows`
crate version. GPUI pulls in `windows 0.61`, and if Cargo reuses that for
`gpu-allocator` the build of `wgpu-hal` fails with "multiple different
versions of crate `windows`". `Cargo.lock` pins `gpu-allocator` to
`windows 0.62.2`; keep it that way when updating dependencies.
