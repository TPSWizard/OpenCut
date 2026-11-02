# OpenCut Web

The browser editor. Vite 8 + React 19 + [TanStack Start](https://tanstack.com/start), deployed as a Cloudflare Worker.

## Running

```sh
bun install
bun run dev          # http://localhost:5173
bun run typecheck    # tsc
bun run test         # vitest
bun run build        # production build into dist/
```

Sound search and feedback talk to the API worker in `apps/api`. Start it with
`moon run api:dev` (or `bun run dev` in that folder) and, if it is not on
`http://localhost:8787`, point the web app at it with `VITE_API_URL` (see
`.env.example`). Both features degrade gracefully when the API is missing or
unconfigured.

## Routes

| Route | Rendering | What it is |
| --- | --- | --- |
| `/` | SSR | Landing page |
| `/changelog` | SSR | Release notes rendered from `../../changelog/*.md` |
| `/projects` | client only | Project list, stored in the browser (IndexedDB + OPFS) |
| `/editor/$projectId` | client only | The editor |
| `/editor` | redirect | Sends you to `/projects` |

The client-only routes load their page through `*.lazy.ts` wrappers
(`src/project/components/projects-page.lazy.ts`,
`src/components/editor/editor-route.lazy.ts`). Keep dynamic imports out of the
route files themselves: TanStack's route code-splitter rewrites route files and
does not cope with them.

## Layout of `src/`

- `routes/` – file-based routes (`routeTree.gen.ts` is generated, do not edit).
- `compat/` – stand-ins for `next/image`, `next/link` and `next/navigation`
  used by the ported editor code.
- `components/editor/` – editor shell: header, panels, onboarding, export button.
- `core/` – `EditorCore` and its managers (project, playback, renderer, save…).
- `timeline/`, `preview/`, `masks/`, `animation/`, `effects/`, `text/`,
  `stickers/`, `subtitles/`, `sounds/`, `media/`, `export/` – the editor
  subsystems. `reference/` holds the architecture notes for several of them.
- `services/` – storage (IndexedDB / OPFS, with migrations), renderer
  (WebGPU/WASM compositor and canvas fallback), transcription worker,
  waveform and video caches.
- `commands/`, `actions/` – undoable commands and keyboard-bindable actions.
- `wasm/` – typed wrappers around `opencut-wasm` (integer-tick `MediaTime`).
- `lib/api.ts` – base URL of the API worker.

## Editor API, plugins, scripting

- `src/api/` exposes the editor as `window.opencut` and over a message
  bridge. See [reference/editor-api.md](reference/editor-api.md).
- `src/plugins/` loads manifest-based plugins: sandboxed panel iframes and
  background Workers that talk to the API. See [reference/plugins.md](reference/plugins.md).
  A sample lives in `public/plugins/timeline-stats/`; the browser SDK is
  `public/plugin-sdk/opencut-plugin.js`.
- `src/scripting/` is the Scripting tab: a CodeMirror editor whose code runs
  in a Worker with `opencut` bound to the API (examples in `examples.ts`).

## Rust core

The web app consumes the Rust core as the `opencut-wasm` npm package. The same
crates live in `../../crates`; `wasm-pack build crates/wasm --target bundler
--out-dir pkg --release` from the repo root produces an identical package. To
run the app against a local build: `cd crates/wasm/pkg && bun link`, then
`bun link opencut-wasm` here.

## Tests

Vitest runs the unit tests under `src/**/__tests__` in jsdom.
`vitest.setup.ts` stubs the 2D canvas context so text measurement works
without a native canvas.

## Browser requirements

Chromium-based browsers give the full experience (WebGPU compositor,
WebCodecs export). Other browsers fall back to the canvas renderer and show a
banner.
