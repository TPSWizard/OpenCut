# OpenCut Headless

Run the editor without a UI. The runner starts a headless Chromium, loads the
OpenCut web app and drives it through the [Editor API](../web/reference/editor-api.md)
(`window.opencut`), so everything the editor can do — import media, build a
timeline, run a script, render — works from the command line or from code.
Projects live in a persistent browser profile (`~/.opencut/headless-profile`
by default), so a project created here shows up in later runs.

```sh
cd apps/headless
bun install
bun run browsers          # downloads the Chromium build Playwright expects (once)

bun run render job.json                     # build + export a job
bun run render a.json b.json c.json         # batch
bun run src/cli.ts list                     # projects in the profile
bun run src/cli.ts inspect --project <id>   # timeline as JSON
bun run src/cli.ts run script.js --new "Promo" --output promo.mp4
bun run src/cli.ts import promo.opencut.zip                     # project bundle → project id
bun run src/cli.ts export --project <id> --output promo.opencut.zip
```

Project bundles (`.opencut.zip`: the project JSON plus its media) are what the
web app's *Export project…* and the desktop editor's *Save bundle…* write, so
`import` and `export` move projects between the two editors without a UI.

Without `--url`, the CLI serves the production build of `apps/web` with
`vite preview` on port 4173 (building it first when `dist/` is missing).
Browser storage is per origin, so the port stays fixed between runs for the
profile's projects to be found again; `--port` changes it, and a busy port
falls back to a free one with a warning. Point the CLI at a running app with
`--url http://localhost:5173` during development.

## Job files

```json
{
  "project": { "name": "Promo", "fps": 30, "canvas": { "width": 1920, "height": 1080 } },
  "media": { "intro": "./intro.mp4", "music": "./music.mp3" },
  "timeline": [
    { "type": "clip", "media": "intro", "at": 0 },
    { "type": "text", "text": "Hello", "at": 1, "duration": 3, "params": { "fontSize": 72 } },
    { "type": "clip", "media": "music", "at": 0, "trackType": "audio" }
  ],
  "script": "./finish.js",
  "export": { "format": "mp4", "quality": "high", "output": "./promo.mp4" }
}
```

| Field | Meaning |
| --- | --- |
| `projectId` | Open an existing project instead of creating one |
| `project` | Name, fps, canvas size and background for a new (or the opened) project |
| `media` | Alias → file path (relative to the job file) or `http(s)` URL |
| `timeline` | Ordered steps: `clip` (media alias, `at`, optional `duration`, `trackType`) or `text` (`text`, `at`, `duration`, `params`) |
| `script` | Path to a JavaScript file (or inline code) run in the editor with `opencut` bound to the Editor API, after the steps and before the export |
| `export` | `format` (`mp4`/`webm`), `quality` (`low`…`very_high`), `includeAudio`, `fps`, `preset` (a platform id such as `youtube-shorts`; forces MP4 with that platform's H.264 settings), `output` path |

Times are seconds. Every step goes through the editor's own undoable
commands, so the resulting project opens normally in the UI.

## Library use

```ts
import { HeadlessRunner } from "@opencut/headless";

const runner = new HeadlessRunner({ url: "http://localhost:5173" });
await runner.start();
await runner.openProject({ name: "From code" });
const ids = await runner.importMedia({ media: { a: "/abs/clip.mp4" }, baseDir: process.cwd() });
await runner.call("timeline.addClip", { mediaId: ids.a, at: 0 });
await runner.exportTo({ output: "/abs/out.mp4" });
await runner.stop();
```

`runner.call(method, params)` reaches any Editor API method. The MCP server
in `apps/mcp` is built on this class.

## Notes

- Rendering uses the editor's own exporter (WebCodecs, mediabunny). Headless
  Chromium has no WebGPU adapter, so the compositor falls back to the canvas
  renderer; output is identical in content, slower on effects-heavy scenes.
- Local media is served to the page through a request route
  (`https://media.opencut.local/<alias>/<file>`), never copied into the
  profile; the project keeps its own copy in OPFS like a normal import.
- `--headed` shows the browser, which is the quickest way to debug a job.
