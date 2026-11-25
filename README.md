<table width="100%">
  <tr>
    <td align="left" width="120">
      <img src="https://assets.opencut.app/branding/symbol.svg" alt="OpenCut Logo" width="100" />
    </td>
    <td align="right">
      <h1>OpenCut</h1>
      <h3 style="margin-top: -10px;">A free and open source video editor for web, desktop, and mobile.</h3>
    </td>
  </tr>
</table>

> The web editor in this fork is hosted as **ConzaCut** at https://conzacut.conzahub.com. It is OpenCut under the MIT licence with its own name and marks; see `ATTRIBUTION.md`.

[![Discord](https://img.shields.io/discord/1386309140057690133?label=Discord&logo=discord&logoColor=fff&color=5865F2&style=flat)](https://discord.gg/zmR9N35cjK)
[![X](https://img.shields.io/badge/follow-%40opencutapp-000?logo=x&logoColor=fff&style=flat)](https://x.com/opencutapp)
[![License: MIT](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

## Status

The rewrite ships the complete editor in the browser, plus the pieces the
rewrite set out to add. The editor from
[opencut-app/opencut-classic](https://github.com/opencut-app/opencut-classic)
(the previous codebase, MIT) was ported into `apps/web` on top of the new
stack (Vite, TanStack Start, Cloudflare Workers). See `changelog/` and
`ATTRIBUTION.md`.

| Goal | Where it lives |
| --- | --- |
| Editor | `apps/web` – installable PWA that works offline, projects, media import, multi-track timeline (with freeze frame), GPU-composited preview, media panel previews (hover a video to scrub it, play video and audio before adding them), text (open-licence fonts from Fontsource; curved into an arch or a smile, keyframable, bent by the shared `textwarp` crate in both editors; rich text: a colour, weight, slant or size for just some words, kept in step with edits by the shared `richtext` crate), motion blur on moving clips and text (a shutter setting; the shared compositor draws the clip along its path within the frame and averages), deflicker for timelapses and LED or fluorescent flicker (each frame measured once, then evened out against its neighbours by the shared `deflicker` crate), multicam (camera angles lined up by their sound and switched while playing, by the shared `multicam` crate), a Retouch effect that smooths, brightens and evens skin (found by colour, edge-preserving, the same shader in both editors) on all skin or on faces only (faces found once and kept on the clip as a face track), face blur for privacy (blur, pixelate or black over each face as it moves, from the same face track), stickers (flags, shapes, licence-filtered icon sets from Iconify, and animated stickers from your own Lottie or dotLottie files, drawn by the shared Rust renderer in `crates/lottie` on both editors), effects (colour correction, lift/gamma/gain, curves, levels, shadows/highlights, chroma key (green or blue screen, any key colour, with spill removal), LUTs from .cube/.3dl/HALD files, colour match (other clips take on a reference clip's colours, baked into a LUT both editors draw), vignette, grain, RGB split, pixelate, sharpen, mirror, glitch, kaleidoscope, blur), masks, rounded corners, outlines, drop shadows and glows on any clip (drawn by the shared compositor in both editors), corner pin (drag a video, picture or sticker's four corners to fit it into a screen, sign or frame in perspective; keyframable, drawn by the shared compositor in both editors), keyframes, one-click animations (in, out and loop: fade, pop, slides, drop and bounce, spin, typewriter and word by word for text, pulse, shake, Ken Burns, worked out as the clip plays in both editors), captions (with keyword highlight: the words that matter in a larger size and their own colour, picked locally or by Claude, or clicked), auto zoom punch-ins on the speaker at sentence starts and on emphasis (planned from the captions, centred on the face, worked out as the clip plays in both editors), motion tracking (a sticker, text or clip placed over something in a video follows it, through the same Rust tracker in both editors), cover maker (a frame and a title in one of three styles, saved as a PNG at the canvas shape, 16:9 or 9:16 and embedded as the MP4 cover picture, with a suggested frame), resize project (a new canvas size or shape with every clip, text and caption re-fitted: fit, fill or keep sizes), one-click split-screen layouts for 2 to 4 clips (side by side, stacked, three-up, one big and two small, a grid), audio fades and stereo pan, a track mixer (mute, solo, volume and pan per track with a meter each, worked out by the shared `trackmix` crate in both editors), audio crossfades between neighbouring clips in one step (equal power or linear, played over the cut from the clips' spare audio, worked out by the shared `crossfade` crate in both editors), audio effects on a clip (three-band equaliser, compressor, limiter and presets, heard live and in exports, the same Rust code in both editors), auto-ducking, silence auto-cut, text-based editing (strike words in the transcript to cut them, with the captions retimed; Find retakes strikes earlier takes of a line and false starts so only the last take stays), long video to shorts (Claude, or a local pick, chooses the moments that stand on their own; each becomes a 9:16 project with its captions), EBU R128 loudness measurement and normalisation, whole-mix loudness at export (−14, −16, −23 or −11 LUFS with a true-peak limiter) and a live loudness meter while the preview plays (the shared `loudness` crate in both editors), shot detection to bookmarks or splits, beat markers that cuts snap to, noise reduction, one-click enhance voice (rumble cut, noise reduction, presence, de-esser, compressor, -16 LUFS), voice effects (deeper, higher, chipmunk, monster, robot, echo, radio, hall; the same Rust code in both editors), filler-word removal, sounds (663 bundled CC0 effects plus Freesound), B-roll from a script (stock pictures laid out line by line, optionally with Claude choosing the search words), face swap on video and picture clips through a hosted model, or on the desktop with your own FaceFusion install (with a consent check, and exports that say they contain a swapped face), keyboard shortcut presets (Premiere, Final Cut, Resolve) with in/out marks, ripple delete and edit-point navigation, MP4/WebM export with platform presets (YouTube, Shorts, TikTok, Reels, LinkedIn, X) and safe-area guides |
| Render CLI | `apps/render-cli` – `opencut-render`: renders a project directory or bundle to a video, a still or QA stills with a contact sheet, with platform presets; shares its export loop with the desktop; also writes captions as SRT, WebVTT or ASS (`--captions`) |
| Phone editor | `apps/web/src/mobile` – the same web editor laid out for phones and touch tablets: preview on top, a touch timeline (swipe to scrub under a fixed playhead, pinch to zoom, tap to select, drag to move, handles to trim) and a bottom toolbar, with the assets and properties panels in bottom sheets; installed to the home screen it takes videos and photos from the phone's share sheet into a new project (`apps/web/src/pwa`) |
| Android app | `apps/android` – the web editor as an installable Android app (a Trusted Web Activity, `com.conzahub.conzacut`) that takes shares from other apps; GitHub Actions builds a signed APK and a Play Store bundle; [README](apps/android/README.md) |
| Editor API | `apps/web/src/api` – one typed, JSON surface (`window.opencut`) over the editor; [reference](apps/web/reference/editor-api.md) |
| Plugins | `apps/web/src/plugins` – manifest-based plugins with sandboxed panels and background workers, installed from Settings → Plugins; [guide](apps/web/reference/plugins.md) |
| Scripting tab | `apps/web/src/scripting` – CodeMirror editor in the assets panel; scripts run in a Worker against the Editor API |
| Headless mode | `apps/headless` – CLI and library that drive the editor in headless Chromium: job files, scripts, batch rendering |
| MCP server | `apps/mcp` – Model Context Protocol server so AI agents can open projects, import media, edit and render |
| Rust core | `crates/` – time, editor (document, commands, undo, frame/audio planning), media (FFmpeg over pipes), render, compositor, effects, masks, gpu, bridge and the `opencut-wasm` package built from this repo; consumed by the web app (wasm) and the desktop app (native) |
| Desktop | `apps/desktop` – native GPUI editor on the Rust core: projects, media import, multi-track timeline, GPU-composited preview with audio, inspector, undo/redo, MP4/WebM export, stabilising (vid.stab), smooth slow motion (in-between frames) and AI upscaling (Real-ESRGAN, fetched by `scripts/get-realesrgan.sh`); [README](apps/desktop/README.md) |
| API worker | `apps/api` – Cloudflare Worker (Elysia) with `/health`, `/sounds/search`, `/feedback`; integrations are optional bindings |
| Self-hosting | `infra/self-host` – container image and Compose file running the web app and API worker on one port; [guide](infra/self-host/README.md) |

Not there yet: a Play Store listing or an iPhone app (on iOS, add the site to
the home screen), a native mobile UI on the Rust core (GPUI has none), most
effects, stickers and speech transcription on the
desktop (they live in the web editor; captions made there render on both).

[opencut.app](https://opencut.app) still runs the classic version. The rewrite
lives at [new.opencut.app](https://new.opencut.app) until it takes over.

## Development

Install [proto](https://moonrepo.dev/proto) if you haven't already:

**Linux, macOS, WSL:**

```sh
bash <(curl -fsSL https://moonrepo.dev/install/proto.sh)
```

**Windows (PowerShell):**

```powershell
irm https://moonrepo.dev/install/proto.ps1 | iex
```

If shims fail to run, allow local scripts for your user:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

From the repo root:

```sh
proto use    # installs the tools pinned in .prototools
```

```sh
moon run web:dev       # localhost:5173
moon run api:dev       # localhost:8787
moon run desktop:dev   # see apps/desktop/README.md
```

Each app installs its own dependencies (`bun install` inside `apps/web` and
`apps/api`; `proto use` gives you the pinned Bun). Checks:

```sh
moon run web:typecheck web:test web:build
moon run api:typecheck api:build
moon run headless:typecheck headless:test
moon run mcp:typecheck
moon run wasm:check wasm:test        # Rust core (needs the pinned Rust toolchain)
moon run desktop:check desktop:test
```

The desktop app and the `media` crate drive FFmpeg's `ffmpeg`/`ffprobe`
programs: install FFmpeg (or `moon run media:setup` for the pinned build) and
their tests run for real; without it they skip.

Headless rendering and the MCP server are documented in
[`apps/headless/README.md`](apps/headless/README.md) and
[`apps/mcp/README.md`](apps/mcp/README.md); the Rust crates in
[`crates/README.md`](crates/README.md).

The web app talks to the API worker for sound search and feedback. Without the
worker, or without its `FREESOUND_API_KEY` secret and `FEEDBACK` KV binding,
those two features report that they are unavailable and everything else works.
See `apps/web/README.md` and `apps/api/wrangler.jsonc`.

## Self-hosting

`infra/self-host` builds one container that serves the web editor and its
API worker on a single port, with shares stored in a Docker volume:

```sh
cd infra/self-host
cp .env.example .env
docker compose up -d --build   # http://localhost:8080
```

It covers HTTPS with Caddy, the settings, where data is stored, and pointing
the desktop app at your server: see
[`infra/self-host/README.md`](infra/self-host/README.md).

## Contributing

We're not set up to take outside contributions yet while the architecture is being designed. If you want to follow along, ask questions, or just hang out, [join the Discord](https://discord.gg/zmR9N35cjK) or [open an issue](https://github.com/opencut-app/opencut/issues).

## Sponsors

OpenCut is supported by companies that believe in open source creator tools.

- [**fal.ai**](https://fal.ai?utm_source=github-opencut&utm_campaign=oss): Generative image, video, and audio models all in one place.

Want your logo here? Reach out at [sponsor@opencut.app](mailto:sponsor@opencut.app).

## License

[MIT](LICENSE)
