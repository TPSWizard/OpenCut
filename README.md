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
| Editor | `apps/web` – projects, media import, multi-track timeline, GPU-composited preview, text, stickers, effects, masks, keyframes, captions, sounds, MP4/WebM export |
| Editor API | `apps/web/src/api` – one typed, JSON surface (`window.opencut`) over the editor; [reference](apps/web/reference/editor-api.md) |
| Plugins | `apps/web/src/plugins` – manifest-based plugins with sandboxed panels and background workers, installed from Settings → Plugins; [guide](apps/web/reference/plugins.md) |
| Scripting tab | `apps/web/src/scripting` – CodeMirror editor in the assets panel; scripts run in a Worker against the Editor API |
| Headless mode | `apps/headless` – CLI and library that drive the editor in headless Chromium: job files, scripts, batch rendering |
| MCP server | `apps/mcp` – Model Context Protocol server so AI agents can open projects, import media, edit and render |
| Rust core | `crates/` – time, editor (document, commands, undo, frame/audio planning), media (FFmpeg over pipes), render, compositor, effects, masks, gpu, bridge and the `opencut-wasm` package built from this repo; consumed by the web app (wasm) and the desktop app (native) |
| Desktop | `apps/desktop` – native GPUI editor on the Rust core: projects, media import, multi-track timeline, GPU-composited preview with audio, inspector, undo/redo, MP4/WebM export; [README](apps/desktop/README.md) |
| API worker | `apps/api` – Cloudflare Worker (Elysia) with `/health`, `/sounds/search`, `/feedback`; integrations are optional bindings |

Not there yet: a mobile app (the core crates compile for mobile targets, but
no mobile UI exists), and effects, masks, keyframes, stickers and captions on
the desktop (they live in the web editor).

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

## Contributing

We're not set up to take outside contributions yet while the architecture is being designed. If you want to follow along, ask questions, or just hang out, [join the Discord](https://discord.gg/zmR9N35cjK) or [open an issue](https://github.com/opencut-app/opencut/issues).

## Sponsors

OpenCut is supported by companies that believe in open source creator tools.

- [**fal.ai**](https://fal.ai?utm_source=github-opencut&utm_campaign=oss): Generative image, video, and audio models all in one place.

Want your logo here? Reach out at [sponsor@opencut.app](mailto:sponsor@opencut.app).

## License

[MIT](LICENSE)
