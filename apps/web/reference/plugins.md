# Plugins

OpenCut plugins are web pages and ES modules described by a JSON manifest and
installed by URL from **Settings → Plugins** in the editor. They never run in
the editor's own origin: panels load in a sandboxed iframe with an opaque
origin, background code runs in a dedicated Worker, and both talk to the
editor only through the [Editor API](./editor-api.md) message bridge.

## Manifest

```json
{
  "id": "com.example.timeline-stats",
  "name": "Timeline stats",
  "version": "1.0.0",
  "description": "Shows clip counts and lets you add a title card.",
  "author": "Example",
  "apiVersion": "1",
  "panel": { "title": "Stats", "url": "./panel.html" },
  "entry": "./worker.js"
}
```

- `id` – letters, digits, `.`, `-`, `_`; stable across versions.
- `panel` – optional. Adds a tab to the assets panel that shows `url` in an
  iframe (`sandbox="allow-scripts allow-forms allow-downloads"`).
- `entry` – optional. An ES module imported in a Worker while a project is
  open. Export `activate(opencut)` (or a default function); return a function
  to run on deactivate, or export `deactivate`.
- At least one of `panel` or `entry` is required. Relative URLs resolve
  against the manifest URL.

## Panel pages

Use the SDK served by the editor:

```html
<script type="module">
  import { connect } from "https://<editor host>/plugin-sdk/opencut-plugin.js";
  const opencut = await connect();
  const timeline = await opencut.timeline.get();
  opencut.on("timeline", async () => { /* refresh */ });
  await opencut.timeline.addText({ text: "Hi", duration: 3 });
</script>
```

`connect()` resolves once the editor has registered the frame. The object
has every API namespace, `call(method, params)`, `on(event, listener)`,
`apiVersion` and `disconnect()`.

## Background modules

```js
export async function activate(opencut) {
  const stop = opencut.on("timeline", async () => {
    const timeline = await opencut.timeline.get();
    // react to edits
  });
  return () => stop();
}
```

The Worker is terminated when the plugin is disabled, removed, or the project
closes. Status (running / error) shows in Settings → Plugins.

## Sample

`public/plugins/timeline-stats/` is a complete plugin (manifest, panel,
background module). Install it from the Plugins settings with one click or by
pasting `/plugins/timeline-stats/manifest.json`.

## Model

- `src/plugins/types.ts` – manifest parsing and tab keys.
- `src/plugins/plugins-store.ts` – installed plugins, persisted in
  `localStorage` under `opencut-plugins`.
- `src/plugins/plugin-host.tsx` – Worker lifecycle for `entry` modules.
- `src/plugins/components/plugin-panel-view.tsx` – iframe host that registers
  the frame with the bridge.
- `src/plugins/components/plugins-settings.tsx` – install / enable / remove UI.
