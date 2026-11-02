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

The rewrite now ships the complete editor in the browser. The editor from
[opencut-app/opencut-classic](https://github.com/opencut-app/opencut-classic)
(the previous codebase, MIT) was ported into `apps/web` on top of the new
stack (Vite, TanStack Start, Cloudflare Workers): projects, media import,
multi-track timeline, GPU-composited preview, text, stickers, effects, masks,
keyframes, captions, sounds and MP4/WebM export. See `changelog/0.4.0.md` and
`ATTRIBUTION.md`.

What this repository contains today:

| App | State |
| --- | --- |
| `apps/web` | Full editor, type-checked, 193 unit tests, production build |
| `apps/api` | Cloudflare Worker (Elysia) with `/health`, `/sounds/search`, `/feedback`; integrations are optional bindings |
| `apps/desktop` | GPUI shell with the four panels laid out; no editing yet |
| `crates/media` | FFmpeg setup script for the future Rust core |

Still planned, from the original rewrite goals:

- An Editor API and first-class third party plugins (plugin-first architecture)
- Desktop, mobile, and browser from one codebase (Rust core)
- MCP server (for AI agents), headless mode (automation, batch rendering)
- A scripting tab directly in the editor

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
```

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
