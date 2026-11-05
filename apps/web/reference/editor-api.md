# Editor API (v1)

One programmable surface over the editor, used by the scripting tab, plugins,
the headless runner and the MCP server. The implementation lives in
`src/api/`:

- `types.ts` – the method map (`EditorApiMethods`), DTOs and the wire protocol.
- `editor-api.ts` – `createEditorApi({ editor })`: validates params, converts
  seconds to ticks, runs the editor's undoable commands, returns JSON.
- `bridge.ts` – `installEditorApiBridge`: attaches `window.opencut`, serves
  `postMessage` requests from allowed windows and workers, broadcasts events.
- `client.ts` – `createRemoteEditorApi`: the client used in Workers and
  iframes; `createEditorApiFacade` turns it into `opencut.<ns>.<method>()`.

Everything is JSON-serialisable; times are **seconds** (the editor converts
to its integer tick `MediaTime`). Elements are addressed by
`{ elementId, trackId? }`; the track is looked up when omitted.

## In the page

```js
const info = await window.opencut.call("app.info");
const timeline = await window.opencut.timeline.get();   // namespaced sugar
window.opencut.on("timeline", (event) => console.log(event.sequence));
```

`window.opencut` is installed once per page load (`src/api/api-root.tsx`).
`app.info` reports `ready: true` when a project is open; project-bound
methods answer with the error code `no_project` before that.

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `app.info` | – | `{ apiVersion, ready, projectId }` |
| `project.get` | – | `ApiProjectInfo` |
| `project.create` | `{ name? }` | `ApiProjectSummary` (navigate to `/editor/<id>` to open it) |
| `project.list` | – | `ApiProjectSummary[]` |
| `project.rename` | `{ name }` | `ApiProjectInfo` |
| `project.updateSettings` | `{ fps?, canvas?, background? }` | `ApiProjectInfo` |
| `project.save` | – | `{ saved: true }` |
| `project.exportBundle` | `{ fileName? }` | `{ fileName, bytes }` — downloads a project bundle (`.opencut.zip`: the project JSON plus its media; the desktop editor reads and writes the same file) |
| `project.importBundle` | `{ file: File }` | `ApiProjectSummary` (in page / structured clone only) |
| `scenes.list` / `scenes.switch` | – / `{ sceneId }` | `ApiSceneInfo[]` |
| `media.list` | – | `ApiMediaInfo[]` |
| `media.importUrl` | `{ url, name? }` | `ApiMediaInfo` |
| `media.importFiles` | `{ files: File[] }` | `ApiMediaInfo[]` (in page / structured clone only) |
| `media.remove` | `{ mediaId }` | `{ removed: true }` |
| `timeline.get` | – | `ApiTimelineInfo` |
| `timeline.addClip` | `{ mediaId, at?, duration?, trackId?, trackType?, name? }` | `ApiElementInfo` |
| `timeline.addText` | `{ text, at?, duration?, name?, params?, trackId? }` | `ApiElementInfo` |
| `timeline.updateElement` | `{ elementId, trackId?, patch }` | `ApiElementInfo` |
| `timeline.moveElement` | `{ elementId, trackId?, startTime }` | `ApiElementInfo` |
| `timeline.setTransition` | `{ elementId, trackId?, transition: { type, duration?, params? } \| null }` | `ApiElementInfo` |
| `transitions.list` | – | `ApiTransitionInfo[]` |
| `captions.insert` | `{ captions: ApiCaption[], style?: ApiCaptionStyle }` | `{ trackId, elements: ApiElementRef[] }` |
| `captions.transcribe` | `{ language?, modelId?, wordsPerCaption?, insert?, style? }` | `{ text, words, captions, trackId }` (in page: runs Whisper) |
| `timeline.split` | `{ elements, time }` | `ApiElementRef[]` |
| `timeline.delete` | `{ elements }` | `{ deleted }` |
| `timeline.duplicate` | `{ elements }` | `ApiElementRef[]` |
| `timeline.addTrack` / `timeline.removeTrack` | `{ type, index? }` / `{ trackId }` | `ApiTrackInfo` / `{ removed: true }` |
| `timeline.setTrackMuted` / `timeline.setTrackHidden` | `{ trackId, muted }` / `{ trackId, hidden }` | `ApiTrackInfo` |
| `playback.getState` / `play` / `pause` / `seek` | – / – / – / `{ time }` | `ApiPlaybackState` |
| `selection.get` / `set` / `clear` | – / `{ elements }` / – | `ApiElementRef[]` |
| `history.undo` / `redo` / `state` | – | `{ canUndo, canRedo }` |
| `export.render` | `ApiExportOptions` | `{ fileName, mimeType, bytes: ArrayBuffer }` |
| `export.download` | `ApiExportOptions` | `{ fileName, mimeType, bytes }` and a browser download |
| `export.presets` | – | `ApiPlatformPreset[]` — platform presets (YouTube, Shorts, TikTok, Reels, LinkedIn, X) with canvas, fps, limits, safe area and the encoder settings for the open project |
| `export.check` | `{ preset, includeAudio? }` | `{ preset, issues, estimatedBytes }` — what the platform would object to (canvas, fps, length, size), each with a `fix` where a project setting clears it |

`ApiExportOptions.preset` names a platform preset: the export is then MP4
with H.264 High profile at the platform's recommended bitrate (YouTube's SDR
table by frame size and rate), a closed GOP of half the frame rate, the moov
atom first and 384 kbps AAC. The canvas and frame rate stay as the project
has them, so call `export.check` and `project.updateSettings` first.

`patch` for `timeline.updateElement` takes `name`, `startTime`, `duration`,
`trimStart`, `trimEnd`, `hidden` and `params` (merged into the element's
params). Common params: `transform.positionX/Y`, `transform.scaleX/Y`,
`transform.rotate`, `opacity`; text elements also take `content`, `fontSize`,
`fontFamily`, `color`, `textAlign`, `letterSpacing`, `lineHeight`.

A transition (`timeline.setTransition`) belongs to the clip it leads out of
and plays centred on the cut to the next clip on the track, half over each
side, so durations never change; a clip with nothing after it fades out over
its own end. `type` is a name from `transitions.list` (the gl-transitions
catalogue, for example `Fade`, `DirectionalWipe`, `CrossZoom`), `duration` is
seconds (default 1) and `params` are the shader's uniforms as `transitions.list`
describes them. `ApiElementInfo.transition` reads it back.

Captions are text elements that carry `words`: the content's
whitespace-separated tokens with the seconds each is spoken, relative to the
element's start (`ApiElementInfo.words`). `captions.insert` takes captions in
timeline seconds (`{ text, startTime, duration, words?, style? }`; the words,
joined by spaces, must spell the text) and puts them on a new text track at
the top of the overlays. `style.highlight` picks how the spoken word stands
out: `word` colours it (`activeColor`), `box` also draws a rounded box behind
it (`boxColor`), `fill` colours every word spoken so far, `none` leaves the
text plain; the same values live in the `caption.*` params of each element.
`captions.transcribe` runs the editor's own Whisper worker on the scene's
audio (`modelId`: `whisper-tiny`, `whisper-base`, `whisper-small` (default),
`whisper-medium`, `whisper-large-v3-turbo`; the model downloads into the
browser cache on first use) and returns the words and captions, inserting them
unless `insert` is false.

Errors carry a `code`: `not_found`, `invalid_params`, `no_project`,
`unknown_method`, `failed`.

## Events

`project`, `timeline`, `media`, `playback`, `selection` (and `history`) fire
after the editor's state changes, coalesced per tick, as
`{ type: "opencut:event", event, sequence }`.

## Wire protocol

Remote consumers post `{ type: "opencut:request", id, method, params }` and
receive `{ type: "opencut:response", id, ok, result | error }`. The bridge
sends `{ type: "opencut:ready", apiVersion }` when it registers a consumer and
forwards events. `ArrayBuffer` results (exports) are transferred, not copied.

Only windows registered with `bridge.allowSource(window)` (plugin iframes) and
workers the editor spawned (plugins, scripts) are answered.

## Adding a method

1. Add the entry to `EditorApiMethods` in `types.ts`.
2. Implement it in the `handlers` map in `editor-api.ts` (validate, convert
   seconds with `ticks()`, go through the managers' command-based methods).
3. Document it here and in `apps/mcp/src/server.ts` (`EDITOR_API_SUMMARY`).

The method list is derived from the handlers, so remote facades pick it up
without changes.
