# ConzaCut feature research

Research date: 2026-11-04. This report answers `prompts/feature-research.md`. It
recommends what to build next for ConzaCut (the MIT fork of OpenCut hosted at
https://conzacut.conzahub.com) with evidence, effort and licensing for each
item, and it identifies the features that can also serve the Shortform Studio
pipeline planned in the sibling Overtime AI Studio repository.

How to read the labels. **CONFIRMED** means the fact was read at the cited URL
or in this repository's files on the research date. **INFERRED** means it was
reasoned from cited evidence. **UNKNOWN** means it could not be determined.
Reaction counts are GitHub thumbs-up on the opening post unless stated;
Kdenlive counts are Bugzilla votes. Nothing in this report copies CapCut or any
proprietary editor's assets; product pages of commercial editors are cited only
as evidence of what users expect, by feature name.

What the repository actually has today, as read in this checkout (all
CONFIRMED from the files named):

- Captions already run in the browser. `apps/web/src/services/transcription/worker.ts`
  loads `onnx-community/whisper-*` through `@huggingface/transformers` with
  `dtype: "q4"`, `device: "auto"` and `return_timestamps: true`, so the editor
  gets segment-level timestamps, not word-level ones. The task background that
  said captions depend on upstream's hosted transcription was wrong for this
  branch.
- Stickers are flags and shapes only. `apps/web/src/stickers/providers/index.ts`
  registers `logos` (empty), `flags` and `shapes`; there is no Iconify code in
  `apps/web/src`. The background that said "stickers (Iconify)" was also wrong.
- Effects are a single Gaussian blur (`apps/web/src/effects/definitions/blur.ts`,
  `crates/effects`). There is no transition element type in
  `apps/web/src/timeline/types.ts` (tracks: video, text, audio, graphic, effect).
- The Editor API v1 (`apps/web/reference/editor-api.md`) has no keyframe,
  effect, mask, caption or transition methods; the headless runner and MCP
  server inherit that gap.
- Default keybindings (`apps/web/src/actions/definitions.ts`) are Space/K play,
  J/L seek, arrows step, S split, Q/W split-left/right, N snapping, Ctrl+D
  duplicate, Ctrl+Z/Ctrl+Shift+Z undo/redo; no I/O marks, no marker key, no
  ripple-delete key. Keybindings are already user-editable and exportable
  (`apps/web/src/actions/keybindings-store.ts`).
- Platform layout guides exist on the web (`apps/web/src/guides/definitions/platforms.tsx`:
  tiktok, ig-reels, yt-shorts, spotlight) but not on the desktop.
- The web manifest exists (`apps/web/public/manifest.json`), with no service
  worker registered in `apps/web/src/routes/__root.tsx`.
- The pinned desktop FFmpeg is 8.1.3 from BtbN's **LGPL** shared builds
  (`crates/media/setup/ffmpeg.json`), which matters for every GPL-gated filter
  below.
- `@hugeicons/core-free-icons` resolves to 4.4.0 in `apps/web/bun.lock`, the
  version whose licence file is MIT.
- The API worker (`apps/api/src/index.ts`) proxies Freesound with a commercial
  licence filter and has no key configured; `CORS_ORIGIN` is set to the hosted
  site.

## 1. Summary: the ten highest-value additions

**1. Transitions, from gl-transitions on the web and `xfade` on the desktop.**
Transitions are the one editing feature upstream promised and never shipped:
the maintainer wrote "Filters and transitions are expected to be released by
v0.4.0" (2026-03-28), v0.4.0 never shipped, the tracking issue #811 still says
"Transitions and filters are coming", and the community PR #750 with 24 effects
and animated presets has sat unreviewed since 2026-03-23 (all CONFIRMED). The
gl-transitions collection is MIT (two BSD), holds 181 GLSL files, and upstream
already ships WGSL, SkSL and Metal translations plus a Rust crate that targets
wgpu, which is exactly what `crates/compositor` uses on both targets
(CONFIRMED). FFmpeg's built-in `xfade` filter offers 58 named transitions for an
FFmpeg-only path (CONFIRMED). The work is a two-input pass in the compositor, a
`transition` element in the project JSON, and a catalogue that maps each entry
to a GLSL/WGSL source and, where one exists, an `xfade` name. Effort L.

**2. Word-level captions with karaoke highlighting.** The editor's Whisper
worker already runs locally, but it asks for segment timestamps only, and the
plain `onnx-community/whisper-*` exports it loads were not exported with the
attentions that word timestamps need; Transformers.js documents
`return_timestamps: "word"` and ships `onnx-community/whisper-base_timestamped`
and `whisper-large-v3-turbo_timestamped` for exactly this (CONFIRMED). Word
times unlock the TikTok-style active-word captions that Remotion's MIT
`@remotion/captions` package demonstrates, ASS `\k` karaoke burn-in through
libass (ISC) on the desktop, filler-word removal, and the word-anchored overlays
the Shortform Studio blueprint is built on. Upstream's own tracking issue lists
"Auto captions are getting some fixes", and users asked for caption animation
(#807), bulk styling (#901) and a karaoke data model (#898) (CONFIRMED). Effort
S to M.

**3. Platform export presets and safe areas on both editors.** The most
upvoted browser-editor asks across the trackers are portrait output and
platform presets: OpenCut #737 (2 thumbs), #911, Omniclip #22 (6) and #5 (3)
(CONFIRMED). The official specs are public: YouTube wants MP4 with the moov atom
at the front, H.264 High profile, closed GOP of half the frame rate, 4:2:0 and a
bitrate table; Shorts allows up to 3 minutes at a maximum of 1080p; Reels
accepts 1.91:1 to 9:16 at a minimum of 720 pixels and 30 fps; LinkedIn caps
files at 5 GB and 15 minutes (all CONFIRMED). The web app already has canvas
presets and layout guides; the desktop has neither guides nor presets. Effort S.

**4. The `opencut-render` CLI plus Editor API methods for keyframes, effects,
masks and captions.** This is the backbone of studio coupling. The blueprint
chose a native Rust CLI over `editor`, `render` and `media` as the preferred
renderer, and noted that the headless web path "lacks keyframe and caption
methods in Editor API v1" (CONFIRMED in `docs/SHORTFORM_STUDIO_BLUEPRINT.md` and
`apps/web/reference/editor-api.md`). Upstream users asked for a headless SDK
(#778) and a Python API (#214) (CONFIRMED). Factoring the export path out of
`apps/desktop/src/render_thread.rs` is bounded work, and adding
`timeline.setKeyframes`, `timeline.addEffect`, `timeline.setMask` and
`captions.insert` to `apps/web/src/api/editor-api.ts` follows the documented
three-step recipe. Effort M for each.

**5. Silence auto-cut and filler-word removal.** Silence removal is asked for
in OpenCut (#791), Olive (#2348), a 39-post Shotcut forum thread and an Adobe
idea thread; Kdenlive only offers it through its VOSK path (CONFIRMED). In the
browser it is windowed RMS over decoded PCM, optionally with Silero VAD (MIT
model, 2.3 MB, ISC wrapper); on the desktop it is FFmpeg's `silencedetect`
parsed into an edit list, the way LosslessCut does it (CONFIRMED). Filler-word
removal, the Descript feature, needs item 2's word timestamps and a per-language
list (CONFIRMED). Effort M together.

**6. A real effects and colour library: LUTs, colour correction, stylise.**
Upstream's tracking issue promises "expanding the effects library
significantly"; Kdenlive's top LUT wish is an intensity slider (20 votes); every
surveyed desktop editor applies LUTs through FFmpeg's `lut3d`, and FreeCut shows
a `.cube` loader working in the browser (CONFIRMED). The portable shader pool is
three.js (MIT: film grain, vignette, RGB shift, glitch, kaleidoscope, mirror,
halftone, bloom, LUT pass and cube/3dl loaders), pmndrs/postprocessing (Zlib)
and Babylon.js post-processes (Apache-2.0); Shadertoy, lygia, frei0r, MLT,
Kdenlive, Olive and Shotcut code is off limits for licence reasons (CONFIRMED).
Effort M, spread over several small effects.

**7. Asset sources with clean licences: fonts, icons, sounds.** Fontsource
exposes a keyless API that returns each family's licence and jsDelivr file
URLs, so the desktop can download OFL and Apache font files for deterministic
rendering instead of relying on installed fonts (CONFIRMED). Iconify's API is
MIT and its collection metadata carries an SPDX licence per set, so a sticker
provider can allow-list MIT, Apache, ISC, CC0 and CC-BY sets (CONFIRMED).
Kenney's interface and UI audio packs and Juhani Junkala's 512-sound retro pack
are CC0 and can ship in the repo; Freesound search already exists and only
needs its key, with the caveat that commercial API use is negotiated case by
case (CONFIRMED). Effort S each.

**8. Audio automation: ducking, loudness, noise reduction.** "Mix video
narration with audio music track (auto-duck)" is Kdenlive's single
most-voted wish (30 votes); Shotcut and OpenReel ship it (CONFIRMED). Both
ConzaCut editors already keyframe volume, so ducking is an RMS envelope turned
into keyframes, which also gives the studio its ducked music bed. Loudness
normalisation (Olive #262, 38 comments) is EBU R128 with MIT libraries on both
targets; RNNoise (BSD-3) has Apache-2.0 WASM builds (CONFIRMED). Effort S to M.

**9. Desktop parity: stickers, captions, native Whisper, guides, waveforms.**
The desktop README lists stickers and captions as "not there yet" and bundle
import drops them (CONFIRMED). Captions are text elements, so that half is
small; SVG stickers need a rasteriser. Native speech-to-text can follow
Shotcut's pattern: whisper.cpp (MIT) as a subprocess or whisper-rs (Unlicense)
in process, ggml models from Hugging Face with sha256 manifests, and GPU with an
automatic CPU retry (CONFIRMED). The FFmpeg 8 `whisper` filter is segment-only,
so word timings need the library, not the filter (CONFIRMED). Effort M.

**10. Browser-feasible AI: auto-reframe and background removal.** No surveyed
open-source editor has automatic subject-tracking reframe; Shotcut's "Reframe"
is a manual keyframed crop (CONFIRMED). MediaPipe Tasks Vision (Apache-2.0,
14.6 MB wasm) runs face and object detection in the browser with models of
0.23 MB to 7 MB; Google's AutoFlip documents the saliency-and-camera design
(CONFIRMED). Background removal has a cheap live path (Selfie Segmenter, 249 KB,
Apache-2.0 model card) and an offline high-quality path (ISNet or BiRefNet via
ONNX Runtime Web with WebGPU) whose weight licences still need confirming; RVM,
`@imgly/background-removal` and Ultralytics YOLO are GPL or AGPL and excluded
(CONFIRMED). Both features also feed the studio's cut-out pipeline. Effort L
each, gated on the hosting decisions in section 5.

## 2. Backlog

Effort scale: S is up to a few days, M one to two weeks, L several weeks, XL
months. Rows are ordered by value divided by effort as judged from the demand
evidence, the reuse across web, desktop and studio, and the licence risk.
"Runs" says where the code executes. Demand links are to the issue or page
that was read; counts are thumbs-up unless stated.

Rows 59 to 79 were added on 2026-11-21 after rows 1 to 58 (except 7 and 48)
were built. Their evidence is the list of 287 draft model classes read from
CapCut 9.2.0.3931's `videoeditor.dll` in the sibling `capcut-recon` repository
(`docs/evidence/E0019_generated_draft_model_inventory.txt`): a class name
confirms CapCut stores that feature in its projects, not how it behaves. Each
gap was checked by reading this checkout on the same date. Nothing is copied
from CapCut; the designs are own code.

Rows 80 to 113 were added on 2026-11-24 after rows 59 to 79 were built. Their
sources are the E0019 classes not yet covered (named in each row); the open
upstream OpenCut issues that are not done yet; a survey of current competitor
pages (CapCut, Descript, Premiere and Premiere on iPhone, DaVinci Resolve 20,
Clipchamp, Kapwing, Filmora; Adobe help pages and every canva.com page refused
automated fetches, so nothing here rests on them); and a check of this
checkout for each candidate on the same date (ABSENT means no code was found
in `apps/web/src`, `apps/desktop/src` or `crates`; PARTIAL names what exists).
Licences were read on the linked model or repository page; a licence that was
not stated is UNKNOWN, not assumed.

| # | Feature | Area | Demand evidence | Runs | Effort | Dependencies and prerequisites | Licence or provenance | Label |
|---|---|---|---|---|---|---|---|---|
| 1 | Platform export presets (Shorts, TikTok, Reels, LinkedIn, X) with faststart, High profile, closed GOP, bitrate table | platform | OpenCut #737 (2), #911; Omniclip #22 (6), #5 (3); YouTube, Shorts, Reels, LinkedIn spec pages | both | S: export options exist; add a preset table and canvas defaults | `apps/web/src/export`, desktop export dialog; X specs are UNKNOWN (help pages 404) | Public spec pages; no assets | CONFIRMED |
| 2 | Word-level timestamps in the caption worker (switch to `onnx-community/whisper-*_timestamped`, `return_timestamps: "word"`) | captions | Upstream #811 "Auto captions are getting some fixes"; #898 karaoke model; Transformers.js docs and issue #820 | web | S: model id and one option; base model is about 207 MB with the WebGPU recipe | WebGPU for speed (Chrome/Edge 113+, Safari 26, Firefox 141 Windows); WASM fallback | Transformers.js Apache-2.0; Whisper MIT | CONFIRMED |
| 3 | Active-word (karaoke) caption style and caption style presets, bulk restyle | captions | #807 caption animation, #901 bulk styling, #826 Hebrew RTL; Remotion `createTikTokStyleCaptions` as a design reference | both | M: a caption chunk gets `words[]`; text renderer highlights the active word | Row 2; `apps/web/src/subtitles`; desktop text rasteriser | Own code; `@remotion/captions` is MIT but not a dependency | CONFIRMED |
| 4 | Auto-ducking of music under voice (envelope to volume keyframes) | sounds | Kdenlive bug 392623, 30 votes, top wishlist item; Shotcut ships "Automatic audio ducking"; studio blueprint needs ducked music | both | S: RMS envelope in a worker; volume keyframes exist on both | Audio decode to PCM (web: WebCodecs or `decodeAudioData`; desktop: `media` crate) | None | CONFIRMED |
| 5 | Keyboard presets and missing actions: I/O marks, M marker, Shift+Delete ripple delete, A/B/T tools, Ctrl+B split alias, Up/Down next edit | editor UX | #627, #789, #811 "Keyboard-first mode"; Premiere, FCP, Resolve shortcut pages agree on J/K/L, I/O, M, Space, Home/End, Up/Down | web, then desktop | S: keybindings store already supports custom maps and import/export | `apps/web/src/actions/definitions.ts`; new actions for marks and ripple delete | Behaviour only; no vendor assets | CONFIRMED |
| 6 | CC0 sound-effect starter pack bundled (Kenney interface and UI audio; Junkala 512 retro sounds) | sounds | Freesound key unconfigured, so sounds tab is empty today; both packs state CC0 | both | S: files under `apps/web/public/sounds` or R2, plus a desktop provider | Pack size budget; Workers static assets allow 25 MiB per file | CC0 (Kenney pages; Junkala INFO.txt) | CONFIRMED |
| 7 | Configure Freesound (token for search and previews; OAuth2 only for downloads) | sounds | Feature exists in `apps/api`; Freesound limits 60/min, 2000/day | web via API worker | S: `wrangler secret put FREESOUND_API_KEY` | Commercial API use "negotiated on a case by case basis with UPF"; previews in renders UNKNOWN | Per-sound CC0, CC-BY, CC-BY-NC | CONFIRMED |
| 8 | Fontsource font catalogue (keyless API, licence in response) and downloadable font files for the desktop | assets | Fonts load from Google at runtime on web only; desktop uses installed fonts (desktop README); Google API ToS forbids embedding keys in open source | both | M: catalogue fetch, cache, cosmic-text font loading from file | Fontsource limit 2500 req/10 s; pin versions; keep each font's licence file | Fontsource MIT; fonts OFL 1.1, Apache 2.0, UFL | CONFIRMED |
| 9 | Iconify sticker provider filtered by SPDX licence (MIT, Apache, ISC, BSD, CC0, Unlicense; CC-BY with credit) | assets | Stickers are flags and shapes only; #713 "parity with CapCut: filters, transitions, effects" | web first; desktop after row 23 | S: `/collections` then `/search?prefixes=`; self-host option | `license.spdx` is optional, keep a title fallback; exclude GPL and CC-BY-SA sets | Iconify API MIT; icons per set | CONFIRMED |
| 10 | `opencut-render` CLI (`--project`, `--out`, `--fps`, `--size`, `--frame-at`, `--stills`) | studio coupling | Blueprint names it the preferred renderer; #778 headless SDK (2), #214 Python API (2, 9 reactions) | desktop, studio | M: factor `apps/desktop/src/render_thread.rs` over `editor::ProjectStore`, `render::Renderer`, `render::Mixer`, `media::Encoder` | FFmpeg on PATH or `OPENCUT_FFMPEG_DIR`; separate render host (not Workers) | Own code, MIT | CONFIRMED |
| 11 | Editor API v2 methods: `timeline.setKeyframes`, `timeline.addEffect`, `timeline.setMask`, `captions.insert`, `timeline.addTransition` | studio coupling | Blueprint: headless path "lacks keyframe and caption methods"; MCP tool `opencut_call` inherits the gap | web, headless, MCP | M: follow the three-step recipe in `editor-api.md`; mirror in `apps/mcp/src/server.ts` | Row 1 of the transitions work for `addTransition` | Own code | CONFIRMED |
| 12 | Silence auto-cut (web RMS or Silero VAD; desktop `silencedetect`) with margin and ripple | AI | OpenCut #791; Olive #2348; Shotcut forum thread, 39 posts; Adobe idea thread; CapCut "Speech Pause Detection" | both | M: PCM analysis, segment list, ripple-apply through existing commands | Ripple editing exists; Silero VAD 2.3 MB optional | auto-editor is Unlicense (reference); Silero VAD MIT; vad-web ISC; LosslessCut GPL-2.0 (reference only) | CONFIRMED |
| 13 | EBU R128 loudness normalisation and a loudness meter | sounds | Olive #262 (38 comments); LosslessCut #1804, #2934; Shotcut ships two-pass loudness | both | S: `ebur128` crate on desktop; `needles` or a WASM build on web | FFmpeg `loudnorm` defaults I -24, LRA 7, TP -2 | ebur128 (Rust) MIT; needles MIT; ebur128_wasm Apache-2.0 | CONFIRMED |
| 14 | SRT, WebVTT and ASS export of captions (ASS with `\k` for karaoke; desktop burn-in via libass) | captions | SRT import shipped upstream (PR #756); studio deliverables include SRT; WebVTT inline timestamps for highlighting | both | S: `apps/web/src/subtitles/{srt,ass}.ts` already parse; add writers | Desktop burn-in needs an FFmpeg build with libass; check the pinned LGPL build | libass ISC; WebVTT W3C | CONFIRMED |
| 15 | LUT support (.cube, .3dl, HALD CLUT image) with an intensity slider, tetrahedral option | effects | Kdenlive bug 442792 "Adjust Intensity of LUTs", 20 votes; Kdenlive and Shotcut use `lut3d`; FreeCut loads `.cube` in-browser | both | M: 3D texture in wgpu with half-texel correction; `.cube` parser (format from pmndrs loader; Adobe spec PDF is 404) | `crates/effects` pass; LUT files as media items | three.js `LUTCubeLoader`, `LUT3dlLoader` MIT as reference; free LUTs: a6000-LUTs and D-Cinelike packs CC0, slaycamera and Q-DDL CC-BY 4.0 | CONFIRMED |
| 16 | Colour correction effects: brightness/contrast, hue/saturation, exposure, lift/gamma/gain, white balance, curves | effects | #811 "expanding the effects library"; PR #750 unreviewed; Kdenlive lists 57 colour effects | both | M: one WGSL pass each; MLT's published LGG formula is re-implementable | `crates/effects` plus web `effects/definitions` registry; keyframeable params | three.js `BrightnessContrastShader`, `HueSaturationShader`, `ColorCorrectionShader` MIT | CONFIRMED |
| 17 | Stylise effect pack: film grain, vignette, RGB split, glitch, pixelate, kaleidoscope, mirror, sharpen, bloom, tilt-shift | effects | CapCut and Premiere effect names (Glitch, RGB Split, Film Grain, Vignette, Mosaic, Mirror, Sharpen); PR #750 | both | M: port one shader at a time | Same pipeline as row 16 | three.js MIT; pmndrs/postprocessing Zlib (keep notice); Babylon.js Apache-2.0 (keep NOTICE) | CONFIRMED |
| 18 | Transitions catalogue (gl-transitions GLSL/WGSL in the compositor; `xfade` mapping for FFmpeg-only paths) | effects | #713, #670, #907; #811 "Transitions and filters are coming"; v0.4.0 promise unmet; Olive #1842, Pitivi #2563 | both | L: two-input compositor pass, a `transition` element, trimming semantics, UI | Project JSON v1 extension; desktop reads the same catalogue | gl-transitions MIT (two BSD); FFmpeg `xfade` is a tool, not copied code | CONFIRMED |
| 19 | Safe-area guides on the desktop (TikTok, Reels, Shorts, Spotlight) and a studio safe-area preset file | desktop parity | Web guides exist; studio blueprint validates top 8 % and bottom 18 % | desktop, studio | S: port `guides/definitions/platforms.tsx` | None | Own artwork (`platform-guides/` drawn by OpenCut) | CONFIRMED |
| 20 | QA still sampling in the render CLI (every N ms and at listed times) and a contact sheet | studio coupling | Blueprint §5.5b; reference workflow caught real defects this way | desktop, studio | S: `Renderer` already renders a frame at a time | Row 10 | Own code | CONFIRMED |
| 21 | Audio waveforms, clip gain line and mute on the desktop timeline | desktop parity | LosslessCut #260 "Improve waveform", 21; web has RMS waveforms since upstream 0.3.0 | desktop | M: peak extraction through `media` into a cached buffer | Whether the desktop already draws waveforms is UNKNOWN (README silent) | Own code | UNKNOWN |
| 22 | PWA offline install (service worker, update prompt) and `.opencut.zip` file handler | platform | #714 offline PWA; upstream merged PR #105 PWA; manifest exists but no service worker | web | S to M: `@vite-pwa/tanstack`; TanStack #4988 reports vite-plugin-pwa skipping its build with `tanstackStart()` | `file_handlers` and `launch_handler` are Chrome-only; keep Blob download fallback | vite-plugin-pwa MIT | CONFIRMED |
| 23 | Desktop renders stickers (SVG) and captions; bundle import keeps them | desktop parity | Desktop README "not there yet"; bundle import lists them as left out | desktop | M: captions are text elements (small); SVG needs a rasteriser crate | Choice of SVG crate and its licence to verify | UNKNOWN for the SVG crate licence | CONFIRMED (gap), UNKNOWN (crate) |
| 24 | Native speech-to-text on the desktop (whisper.cpp subprocess or whisper-rs, DTW word timestamps, model manifest with sha256, GPU with CPU retry) | desktop parity | Shotcut 24.10 did exactly this; Kdenlive supports VOSK and Whisper; FFmpeg 8 `whisper` filter is segment-only | desktop | M: model download UI, subprocess driver, SRT/word JSON parse | ggml models 78 MB (tiny) to 1.6 GB (turbo); `-ml 1`, `--dtw` | whisper.cpp MIT; whisper-rs Unlicense (now on Codeberg) | CONFIRMED |
| 25 | Filler-word removal ("um", "uh") with per-language lists and soft cuts | AI | Descript "Remove filler words" help page; jvid #345 | both | M after row 2 or 24; Whisper tends to drop disfluencies, so a prompt or VAD heuristic is needed | Word timestamps | Reference: timkulbaev/ai-video-editor MIT | CONFIRMED (demand), INFERRED (dropout caveat) |
| 26 | Scene and shot detection to markers and splits (frame-difference on web; `scdet` or `select='gt(scene,0.3)'` on desktop; optional TransNetV2 ONNX) | AI | LosslessCut #1398 implemented, #1068; Kdenlive "Automatic Scene Split" | both | M: WebCodecs decode at low resolution, histogram diff, hysteresis | TransNetV2 ONNX is 31 MB (exceeds the 25 MiB static-asset limit, host on R2 or CDN) | PySceneDetect BSD-3 (reference); TransNetV2 MIT; edgetools ONNX MIT | CONFIRMED |
| 27 | Beat markers and beat-snap for cuts | AI | CapCut Auto Cut "Beat Sync"; Pitivi shipped beat detection in 2022.06 | both | S to M: onset energy and tempo estimate | None | web-audio-beat-detector MIT; Meyda MIT; aubio GPL and essentia.js AGPL excluded | CONFIRMED |
| 28 | Noise reduction (RNNoise) as an audio effect | sounds | Olive #428 (9 comments) | both | M: AudioWorklet or offline pass on web; `arnndn` needs a model file on desktop | 48 kHz mono PCM | RNNoise BSD-3; `@shiguredo/rnnoise-wasm` Apache-2.0; `@jitsi/rnnoise-wasm` Apache-2.0; DeepFilterNet MIT/Apache has no WASM | CONFIRMED |
| 29 | Speed ramps (keyframed retime) and reverse clips | editor UX | Kdenlive bug 453881 speed ramp, 22 votes; OpenCut #884 slow-mo, #906 reverse; retime module is constant-rate only | both | L: variable-rate time mapping in `retime` and `editor::plan_frame`; audio stretch | soundtouchjs on web; desktop audio stretch UNKNOWN | Own code | CONFIRMED |
| 30 | Proxy media and preview scaling for large files | editor UX | 15 open performance complaints: #656 (20 comments), #628 (14), #584 (3), #776 (3); Kdenlive, Shotcut, Flowblade, FreeCut all have proxies | both | L: WebCodecs transcode to a low-res proxy in IndexedDB or OPFS; desktop FFmpeg proxies | Storage quota (row 31) | Own code | CONFIRMED |
| 31 | Durable browser storage: `navigator.storage.persist()`, OPFS for media, Safari seven-day eviction handling | platform | Same performance threads; MDN quota page: Chromium up to 60 % of disk, Safari deletes script-written data after seven days without interaction | web | M: storage layer change behind `services/storage` | Home-screen install on iOS avoids the seven-day rule | None | CONFIRMED |
| 32 | Localisation framework with Chinese first | platform | #662 Chinese fork (12), #601 "Need Chinese" (11), #194 i18n (6), #865 Turkish, #875 Catalan, #876 Italian; upstream never answered | web, desktop | M: message catalogue and extraction; library choice open | Font coverage for CJK (#817) | Library licence to confirm when chosen | CONFIRMED (demand) |
| 33 | Group elements and compound clips | editor UX | #811 "You'll finally be able to group elements"; Premiere Ctrl+G, FCP compound clips | both | L: document model, selection, nested timing | `crates/editor` commands and web timeline store | Own code | CONFIRMED |
| 34 | Track locking, range selection, markers panel | editor UX | #736 track locking, #801 range select, #800 PR; bookmarks exist | both | S to M | None | Own code | CONFIRMED |
| 35 | Emoji and brand sticker packs (Twemoji, Noto Emoji, Simple Icons) | assets | #713 parity ask; sticker tab has two categories | both | S: bundle SVGs or fetch from the packages | Simple Icons needs its trademark disclaimer shown; OpenMoji excluded (CC-BY-SA) | Twemoji graphics CC-BY 4.0; Noto Emoji OFL/Apache; Simple Icons CC0 | CONFIRMED |
| 36 | Auto-reframe to 9:16 (face and object detection to a smoothed crop path, per shot) | AI | No surveyed open editor has it; Shotcut "Reframe" is manual; CapCut Auto Reframe page; FilmCraft #1564, #4078; Omniclip #22 | web first; desktop evaluates the same crop keyframes | L: MediaPipe in a worker, saliency fusion, Kalman smoothing, shot resets from row 26 | Self-host `@mediapipe/tasks-vision` wasm (14.6 MB) and models; AutoFlip as design reference | MediaPipe Apache-2.0; BlazeFace and EfficientDet model-card licences UNKNOWN; Ultralytics YOLO AGPL excluded | CONFIRMED (code), UNKNOWN (model cards) |
| 37 | Background removal: live Selfie Segmenter plus offline ISNet or BiRefNet cut-outs with temporal smoothing | AI | CapCut background remover page; Kdenlive 25.04 SAM2 object mask; studio needs cut-outs | both | L: ONNX Runtime Web with WebGPU, offline job, mask cache | COOP/COEP for threads; models 4.6 MB (u2netp) to 973 MB (BiRefNet full) need R2 or CDN hosting | Selfie Segmenter model card Apache-2.0; U-2-Net and ISNet repos Apache-2.0 and BiRefNet MIT, but weight licences not separately stated; RVM GPL-3.0, @imgly AGPL-3.0, BRIA paid: excluded | CONFIRMED (licences), UNKNOWN (weights) |
| 38 | Cut-out cleanup, feathering, tint and skeleton path tracing (deterministic) | studio coupling | Blueprint §5.2: halo removal against the white original, feathering, tint, `TracedPath` extraction | both, studio | M: canvas or WebGPU on web; `image` crate on desktop; shared JSON for paths | Row 37 or a flat-white matte | `image` crate MIT or Apache-2.0 | CONFIRMED |
| 39 | In-browser text-to-speech with Kokoro (voice-over from a script) | AI, studio coupling | Upstream PR #525 Kokoro TTS unreviewed; studio Phase 3 voice; ElevenLabs free tier is non-commercial | web (WebGPU) | M: kokoro-js in a worker; 86 MB to 326 MB model | Word timings via row 2 on the generated audio | Kokoro-82M Apache-2.0; kokoro-js on Transformers.js | CONFIRMED |
| 40 | Caption translation (Opus-MT in Transformers.js, or Chrome's Translator API) | captions | #790 Bengali, #601 Chinese, #875, #876 language demand; Kdenlive ships SeamlessM4T translation | web | M: per-language model download | NLLB-200 is CC-BY-NC and excluded; Chrome Translator API is Chrome 138+ desktop only | Opus-MT CC-BY 4.0 | CONFIRMED |
| 41 | Stock video and image search through the API worker (Pixabay, Openverse; Pexels when keys reopen) | assets, studio coupling | Upstream: zero stock-media requests; studio B-roll stage; MoneyPrinterTurbo (138 k stars) proves the pattern | web via worker, studio | M: worker proxy with 24 h cache (Pixabay rule), attribution UI | Keys as worker secrets; Pexels "new API key issuance is currently paused" | Pixabay Content License (no standalone redistribution); Openverse per-item CC; Pexels terms unreadable (403) | CONFIRMED |
| 42 | Script to shot list to B-roll assembly (LLM call plus row 41) | studio coupling | ShortGPT and MoneyPrinter pattern (MIT); CapCut "AI Script Mode" | studio | M: one LLM call returns ordered search terms; paid call needs approval | Overtime text provider layer already exists | Reference implementations MIT | CONFIRMED |
| 43 | Templates gallery built from project bundles (plus studio presets) | platform, studio coupling | Zero explicit upstream requests; latent in #713; FreeCut ships project templates; Remotion's gallery is licence-gated | both | M: a templates index JSON over `.opencut.zip`; "Use template" imports a bundle | Bundle import exists on both editors | Own templates only; no Remotion or CapCut templates | CONFIRMED (evidence), INFERRED (latent demand) |
| 44 | Project sharing by link (direct-to-R2 upload of a bundle, signed URL) | platform | Zero upstream requests for collaboration; studio needs durable storage anyway | web via worker | M: presigned multipart upload, 7-day expiry | R2 at $0.015 per GB-month, free egress; Workers body limit 100 MB means uploads go direct | Cloudflare pricing pages | CONFIRMED (pricing), INFERRED (value) |
| 45 | Lottie animated stickers from user-supplied files (dotLottie player) | assets | No upstream request; LottieFiles has no public search API | web | M: `@lottiefiles/dotlottie-web` in the preview, rasterised per frame for export | Desktop rendering would need a Rust Lottie renderer (UNKNOWN) | Player MIT; LottieFiles free assets under the Lottie Simple License | CONFIRMED |
| 46 | Stabilisation on the desktop (`deshake` now; vidstab if a GPL FFmpeg build is accepted) | AI, desktop parity | Olive #190 (9); Shotcut #417; Flowblade and Kdenlive ship vidstab | desktop | M: two-pass filter graph | Pinned FFmpeg is an LGPL build; `libvidstab` is GPL-gated in FFmpeg's configure | vid.stab LGPL-2.1+; FFmpeg `deshake` built in | CONFIRMED |
| 47 | Upscaling and frame interpolation on the desktop (realesrgan-ncnn-vulkan, rife-ncnn-vulkan, or `minterpolate`) | AI, desktop parity | OpenCut #742 Video2X request (reporter flags licence) | desktop | L: external executables over pipes, progress, caching | Video2X itself is AGPL and excluded | Real-ESRGAN BSD-3; RIFE MIT; rife-ncnn-vulkan MIT | CONFIRMED |
| 48 | Stem separation and vocal isolation | AI | CapCut vocal remover page | desktop (browser proven but about five minutes per track with threads) | L: demucs.cpp sidecar; 84 MB weights | Weight licence not separately stated | Demucs MIT (code); free-music-demixer MIT (code), its weights proprietary | CONFIRMED (code), UNKNOWN (weights) |
| 49 | AV1 and HEVC export, bitrate and profile controls | platform | Flowblade #1237 AV1; Olive #754 FFmpeg options | both | M: mediabunny codec support on web is UNKNOWN; FFmpeg encoders on desktop depend on the build | Pinned LGPL build's encoder set UNKNOWN | FFmpeg | CONFIRMED (demand), UNKNOWN (feasibility) |
| 50 | Self-hosting guide and container image for the web app and API worker | platform | #333 (8), #745 (8), #234 (7): 23 thumbs, never answered upstream | platform | S: Dockerfile running `vite preview` plus `wrangler dev` for the API | Not a render host; rendering stays client-side | Own code | CONFIRMED |
| 51 | "Open in ConzaCut" import route for a studio-generated bundle (deep link plus bundle import) | studio coupling | Blueprint: "Open in OpenCut exports the OpenCut project JSON for manual finishing" | web | S: `project.importBundle` exists; add a route that accepts a URL | Row 44 for hosted bundles | Own code | CONFIRMED |
| 52 | Asset manifest in bundles (`assets.json`: id, path, licence, source, attribution) | studio coupling, assets | Blueprint rights section requires licence fields; Freesound, Pixabay, Openverse and Twemoji all need attribution carried through | both, studio | S: extend `editor::export_bundle` and the web bundle writer | Row 7, 9, 35, 41 | Own schema | CONFIRMED |
| 53 | Native mobile app | platform | #14 (18), the most-upvoted upstream issue; crates compile for mobile but no UI | mobile | XL: a third front end | GPUI on mobile is UNKNOWN | Own code | CONFIRMED (demand) |
| 54 | Clip in and out transitions: pick a transition for the start and the end of each clip (fade in from black after a gap, out to the next clip or to nothing) | editor UX | Requested by the owner 2026-11-12; today a clip has one transition, played where it ends (into the next clip, or out to nothing) and none of its own at its start | both | S to M: add an in-transition to the element model (web `ElementTransition`, `crates/editor/src/transition.rs`), resolve its window over the clip's first moments when no neighbour's out-transition covers the cut, In/Out pickers in the Transition tab and the desktop inspector, API | Shared transition catalogue (`gl-transitions`) | Own code | CONFIRMED (current behaviour read from `apps/web/src/transitions/window.ts`) |
| 55 | Chroma key (green and blue screen) with key colour, strength, edge softness and spill removal | effects | CapCut "Chroma key"; Shotcut, Kdenlive and Premiere ship it; owner asked for the next CapCut features 2026-11-21 | both | S: one WGSL pass, a colour kind of effect parameter | None | Own code | DONE 2026-11-21 (b07fe90a, web ebf2ce24) |
| 56 | Text-based editing: edit the transcript to cut the clip (delete words or sentences, keep the cut list in sync) | editor UX, AI | CapCut "Edit by transcript"; Descript's core workflow | both | M: word timestamps exist (row 2, desktop row 24); a transcript view with selection mapped to ranges, then the ripple cut used by silence/filler removal | Captions with word timings | Own code | DONE 2026-11-21 (Transcript section on web and desktop, `transcript.get` / `transcript.cut`; filler removal now retimes captions too) |
| 57 | Voice effects (pitch shift, robot, echo, radio, deeper or higher voice) | sounds | CapCut "Voice changer" / "Voice effects" | both | M: offline pass like noise reduction (row 28), FFmpeg `rubberband`/`asetrate`/`aecho` on the desktop | None | Own code | DONE 2026-11-21 (crates/voice shared by desktop and web WASM; inspector Voice / properties Voice effects; `audio.voiceEffect`) |
| 58 | Long video to shorts: pick the strongest moments from a transcript (Claude), cut each into its own 9:16 project with captions and auto-reframe | AI, studio coupling | CapCut "Long video to shorts"; Opus Clip | web via API worker | M: transcript (row 2) + one Claude call through the worker (key already set) + reframe (row 36) + templates | Claude API key on the worker (set) | Own code | DONE 2026-11-21 web only (Captions panel "Long video to shorts", `shorts.plan` / `shorts.make`, worker `POST /shorts/plan`; centre cover crop, Auto-reframe stays a manual step in each short) |
| 59 | Freeze frame: hold the frame at the playhead for a set time (default 3 s) and ripple the rest along | editor UX | CapCut 9.2.0.3931 draft model has `freeze_info` (capcut-recon E0019); the web toolbar already shows "Freeze frame (coming soon)" (`timeline/components/timeline-toolbar.tsx`) | both | S: split at the playhead, render that frame to a PNG asset (web: the compositor; desktop: FFmpeg one-frame extract), insert an image clip, ripple; API `timeline.freezeFrame` | None | Own code | DONE 2026-11-21 (toolbar button on web and desktop, inspector button on desktop, `timeline.freezeFrame`, editor `Command::FreezeFrame`; the still is the full-size source frame with the clip's look, later clips on every unlocked track move along) |
| 60 | Audio fade in and out handles on clips, and stereo pan | sounds | CapCut 9.2.0.3931 draft model has `material_audio_fade` (capcut-recon E0019) and `material_audio_panning`; OpenCut #744 asks for one-click fade in/out; no fade or pan field on audio or video elements (`timeline/types.ts`, `crates/editor/src/audio.rs`) | both | S: two durations and a pan value on the element, applied in the web mixer (GainNode ramps, StereoPannerNode) and the desktop mixer and export; drag handles on the clip corners | None | Own code | DONE 2026-11-21 (`fadeIn` / `fadeOut` / `pan` params in the web Audio tab and the desktop inspector; web preview, web export, desktop mixer and desktop export (`afade`, `pan`) all apply them) |
| 61 | Play media in the media panel before adding it (video scrub on hover, audio play button) | editor UX | OpenCut #548 (8 comments); `MediaPreview` in `components/editor/panels/assets/views/assets.tsx` shows only a thumbnail or an icon | web, desktop | S: a `<video>`/`<audio>` element on hover or click with the asset's object URL (or its proxy); desktop browser panel plays through the existing preview decoder | None | Own code | DONE 2026-11-21 web only (hover a video card to scrub it, play button with sound on video and audio cards, one preview at a time, proxies used); the desktop media browser does not play files yet |
| 62 | Animation presets for text, stickers and clips: in, out and loop (fade, pop, slide, zoom, typewriter, word by word, bounce, shake, Ken Burns pan for photos) | editor UX | CapCut 9.2.0.3931 draft model has `material_animations` (capcut-recon E0019), `sticker_animation` and `caption_anim`; keyframes and easing presets exist (`timeline/components/graph-editor/easing-presets.ts`) but no one-click animations | both | S to M: each preset writes ordinary keyframes over the clip's first or last N frames, so both renderers and the API get it with no new render code; typewriter and word-by-word need a text reveal parameter | None | Own code | DONE 2026-11-22 (Animation tab on the web, Animation section in the desktop inspector; 12 ways in, 9 out, 5 loops including Ken Burns; worked out as the clip plays from `animIn` / `animOut` / `animLoop` params rather than written as keyframes, so trims keep them right and they never clash with keys; typewriter and word by word draw unshown letters clear so the layout holds) |
| 63 | Shadow and glow for text; outline, drop shadow and rounded corners for video, image and sticker clips | effects | CapCut 9.2.0.3931 draft model has `material_video_shadow` (capcut-recon E0019), `material_video_stroke`, `material_round_radius`, `shadow_point`; text has stroke only and rounded corners exist only on the text background (`params/registry.ts`, `crates/editor/src/frame.rs`) | both | S to M: one compositor pass each (alpha dilate for outline, blurred offset alpha for shadow, SDF corner mask), the same WGSL on web and desktop | None | Own code | DONE 2026-11-22 (Style tab on the web, Style section in the desktop inspector; rounded corners in the shared layer shader, outline (ring-sampled dilation) and drop shadow / glow (offset, two-pass Gaussian) as compositor passes on the full-frame layer, so both editors draw them alike; Drop shadow and Glow one-click looks; no glow-only blend mode yet) |
| 64 | Enhance voice: one click for clearer speech (high-pass, noise reduction, presence EQ, de-ess, compressor, loudness to -16 LUFS) | sounds | CapCut 9.2.0.3931 draft model has `material_vocal_beautify` (capcut-recon E0019) and the `enhande_voice` flag; parts exist as separate tools (rows 13, 28) | both | S: a chain in `crates/voice` reusing the noise reduction and loudness code, run as WASM on the web like row 57 | None | Own code | DONE 2026-11-22 (`voice::enhance_voice` in crates/voice/src/enhance.rs, shared by the desktop inspector and the web Voice section through WASM `enhanceVoice`; 80 Hz high-pass, spectral noise reduction only when the clip has pauses, 250 Hz cut and 3.5 kHz presence, linear-phase split-band de-esser, 3:1 compressor with a pause expander, -16 LUFS under a -1 dBFS limiter; editor API `audio.enhanceVoice`) |
| 65 | Caption keyword highlight: pick the words that matter in each caption and colour or enlarge them | captions | CapCut 9.2.0.3931 draft model has `caption_keywords` (capcut-recon E0019) and `subtitle_keywords_config`; shorts-style captions (row 58) depend on it | both (local); web via worker (Claude) | S: a local pick (longest non-stop word, numbers, names) plus an optional Claude pick through the API worker like `/shorts/plan`; the style is a per-word colour and scale on caption words | Captions with word timings | Own code | DONE 2026-11-22 (`keyword` flag on caption words in both documents; local pick in `captions/keywords.ts` and `crates/editor/src/caption_keywords.rs`, checked against the shared `fixtures/caption-keywords/cases.json`; Claude pick through the API worker `POST /captions/keywords`, not yet run against the real API; `caption.keywordColor` / `caption.keywordScale` drawn by both renderers; Captions tab Pick / Clear and click-a-word toggle, desktop Pick / Clear; editor API `captions.pickKeywords` / `clearKeywords` / `setKeywords`) |
| 66 | Auto zoom punch-ins: small zooms in and out on the speaker at sentence starts and emphasis, for talking-head shorts | AI, studio coupling | Shorts tools (Opus Clip, CapCut auto zoom) market it; CapCut has `smart_motion` and `zoom_info_param`, whose behaviour is UNKNOWN | both | S to M: scale and position keyframes placed from the transcript (row 2) and centred on the face box from auto-reframe (row 36) | Captions with word timings | Own code | DONE 2026-11-22 (procedural `zoom.punches` / `zoom.focusX/Y` / `zoom.ramp` params worked out as the clip plays, on top of keyframes and auto-reframe, rather than keyframes; planner in `auto-zoom/zoom.ts` and `crates/editor/src/auto_zoom.rs` checked against `fixtures/auto-zoom/cases.json`; web finds the face with the auto-reframe MediaPipe detector on 8 frames; transform tab Auto zoom section, desktop inspector Auto zoom, editor API `zoom.auto` / `zoom.clear`) |
| 67 | Split-screen layouts: put 2, 3 or 4 selected clips side by side or in a grid in one step | editor UX | CapCut 9.2.0.3931 draft model has `split_screen_info` (capcut-recon E0019) | both | S: a command that sets each clip's position, scale and a rectangle mask (masks exist) for the chosen layout and canvas | None | Own code | DONE 2026-11-22 (six layouts for 2 to 4 clips; cover scale, cell-centre position and a cropped rectangle mask per clip, transform keyframes cleared, one undo step; planner in `split-screen/layout.ts` and `crates/editor/src/split_screen.rs` checked against `fixtures/split-screen/cases.json`; web multi-selection panel, desktop multi-selection inspector, editor API `layout.splitScreen` with an optional gap) |
| 68 | Colour match: make one clip's colour match a reference frame from another | effects | CapCut 9.2.0.3931 draft model has `color_match_info` (capcut-recon E0019) | both | S to M: mean and spread per channel in Lab (Reinhard transfer), written into the existing colour-correct effect or a generated LUT (row 15), so both renderers use it unchanged | None | Own code | DONE 2026-11-22 (the other selected clips take on the first one's colours: reference frame at the playhead or its middle, targets over 5 frames, Lab mean and spread with the spread ratio held to 0.5–2, baked into a 17-point "Colour match" LUT effect that a second match updates; one undo step; maths in `colour-match/match.ts` and `crates/editor/src/colour_match.rs` checked against `fixtures/colour-match/cases.json` from an independent Python reference; web multi-selection panel, desktop multi-selection inspector, editor API `color.match`; measures the reference's source picture, not after its effects) |
| 69 | Resize project: change the canvas size or aspect ratio and re-fit every clip, text and caption | platform | CapCut 9.2.0.3931 draft model has `responsive_layout` (capcut-recon E0019); OpenCut #911; the caption re-layout already written for shorts (`shorts/make.ts` `relayoutCaptions`) is the start of it | both | S: the shorts cover-crop and caption re-layout made a general command with fit, fill and keep-size choices | None | Own code | DONE 2026-11-22 (the old picture scaled about the centre by the content factor: fit = whole picture shows, fill = canvas covered, keep = pixel sizes; positions and their keyframes, outline/shadow pixels and mask feather/stroke follow, each clip's own scale allows for its base size following the canvas (pictures fit it, text follows its height), compounds' clips too; captions placed again at the bottom (a quarter up on a tall canvas, 5 % otherwise; re-wrapped on the web); one undo step; maths in `resize/plan.ts` and `crates/editor/src/resize.rs` checked against `fixtures/resize-project/cases.json` from an independent Python reference; web settings panel "Content when resizing" choice (also used by the export platform fixes), desktop Project section Content row and export fixes, editor API `project.resize`) |
| 70 | Cover and thumbnail maker: pick a frame, add a title, export a PNG (16:9 and 9:16) and embed it as the MP4 cover | platform | CapCut 9.2.0.3931 draft model has `cover` (capcut-recon E0019) and `cover_template` | both | S to M: render one frame plus text layers to PNG; desktop export adds it as an attached picture stream (FFmpeg `-disposition:v:1 attached_pic`) | None | Own code | DONE 2026-11-22 (title in three styles — bold, band, top — wrapped onto at most 3 lines, added as a text clip on a new top track of the project re-fitted to the cover size (fill), drawn by each editor's renderer; PNG at the canvas shape, 16:9 or 9:16; suggested frame from 16 samples scored for sharpness, colourfulness, exposure and contrast; embedded in MP4 exports (web: mediabunny cover metadata; desktop and `opencut-render`: a stream-copy remux with an attached picture); kept in project settings and bundles; layout and score in `cover/layout.ts` and `crates/editor/src/cover.rs` checked against `fixtures/cover/cases.json` from an independent Python reference; web Settings → Cover tab, desktop Project section, editor API `cover.get/set/suggest/render`) |
| 71 | Motion tracking: a sticker, text or mask follows a point or box picked in the video | AI, effects | CapCut 9.2.0.3931 draft model has `material_video_tracking` (capcut-recon E0019), `video_tracker` and `surface_tracking`; OpenCut #786 asks for one-click mask tracking | both | M to L: a pyramidal Lucas-Kanade or template tracker in a shared Rust crate (WASM on the web, as `crates/voice`), output as position (and scale) keyframes on the follower | None | Own code | DONE 2026-11-22 in part (position only; stickers, text, pictures and clips follow; masks not yet, as they have no keyframes): new `crates/tracking` (template matching on half then full size, NCC, sub-pixel peaks, box grows over flat areas, lost frames hold) used by the desktop and, through `crates/wasm` `PointTracker` / `followKeys`, by the web; the spot under the follower's centre at the playhead is tracked until either clip ends and the follower gets simplified position keys (1 canvas px); one undo step; web and desktop multi-selection "Track and follow", editor API `tracking.follow`; tested on synthetic frames in the crate, through WASM in vitest, and through FFmpeg-decoded video on the desktop) |
| 72 | Live audio effects: equaliser, compressor and limiter on a clip or track | sounds | CapCut 9.2.0.3931 draft model has `audio_effect_adjust_params_info` (capcut-recon E0019); the effects registry holds video effects only (`effects/definitions`) | both | M: an audio effect list on elements, Web Audio nodes in the preview and the same biquad and compressor maths in Rust for export and the desktop mixer | None | Own code | DONE 2026-11-22 (per clip, not per track: `audio.*` params for a 3-band EQ (120 Hz shelf, 1 kHz bell, 8 kHz shelf, ±12 dB), a compressor (threshold, ratio; linked, soft knee, make-up) and a limiter (ceiling), plus 7 presets; `crates/voice/src/audio_fx.rs` runs in the desktop's processed-audio cache (`render::rendered_clip`, used by the preview mixer and export) and on the web through WASM `applyAudioFx` on the prepared clip buffer (playback) and in the export mix; web Audio tab "Audio effects" (presets + registry params), desktop inspector block, editor API `audio.setFx` / `audio.fxPresets`) |
| 73 | Corner pin: drag four corners to fit a clip into a screen, sign or frame in perspective | effects | CapCut 9.2.0.3931 draft model has `corner_pin` (capcut-recon E0019) and `corner_pin_info` | both | M: a homography in the compositor's placement step, four keyframable points, on-canvas handles | None | Own code | DONE 2026-11-23 (video, picture and sticker clips: `cornerPin.enabled` + `cornerPin.<corner><X|Y>` params in percent of the clip's own box, keyframable; `crates/compositor/src/corner_pin.rs` (square-to-quad homography, inverted in the layer shader with soft edges) draws it for both editors, the web through the WASM compositor; web Corner pin tab with draggable preview handles, desktop inspector block with Edit on canvas handles (video and picture); editor API `transform.cornerPin` (box percent or canvas pixels); checked against `fixtures/corner-pin/cases.json`. Masks stay in the unpinned box) |
| 74 | Curved text and text on a path | editor UX | CapCut 9.2.0.3931 draft model has `text_curve` (capcut-recon E0019) and `text_typesetting_paths` | both | M: per-glyph placement along an arc or a mask path in both text renderers | None | Own code | DONE 2026-11-23 for curved text (an arc), NOT text on a path: the `curve` text param (-100 smile to 100 arch, 350 degrees at 100, keyframable); new crate `crates/textwarp` bends the flat text raster (glyphs, box, highlights, lines) along the arc, used by the desktop text rasterizer and by the web through WASM `bendText` / `bentTextSize` (off-screen flat text at device scale, bent, drawn back); web Text tab Curve field, desktop inspector Curve row, editor API `text.setCurve`; checked against `fixtures/curved-text/cases.json`; the same text lands within 2 px in both editors. Glyphs are warped, not placed one by one, so letters stretch slightly on the outside of the bend |
| 75 | Motion blur from movement (keyframed position, scale and rotation) | effects | CapCut 9.2.0.3931 draft model has `motion_blur_config` (capcut-recon E0019) | both | M: sample the transform at several sub-frame times and average, with a shutter-angle setting; costs render time, so off by default | None | Own code | DONE 2026-11-24: the `motionBlur` param on visual clips and text (0 off to 100, share of a frame the shutter is open; not keyframed); each editor resolves the transform half a shutter either side (keyframes and one-click animations; desktop `editor::motion_blur`, web `@/motion-blur/blur`) and sends both quads as `LayerDescriptor.motionBlur`; the shared compositor draws the layer at evenly spaced points between them in one shader pass and averages (one draw per pixel of travel plus one, at most 32); web text, drawn into a canvas-sized layer, sends the canvas quad moved about the text's anchor. Web Transform tab field, desktop inspector row, editor API `transform.motionBlur`; checked against `fixtures/motion-blur/cases.json` (real GPU), and the same clip gives the same pixels in both editors. Zoom punch-ins use the frame's zoom at both ends; masks do not move with the blur |
| 76 | Deflicker for timelapse and LED or fluorescent light flicker | effects | CapCut 9.2.0.3931 draft model has `deflicker` (capcut-recon E0019) | desktop first, web later | S on the desktop: FFmpeg `deflicker` in a processed copy, swapped in like noise reduction; M on the web (per-frame brightness smoothing needs frame history) | None | Own code (built without the FFmpeg filter) | DONE 2026-11-24 in BOTH editors, not with FFmpeg's filter: new crate `crates/deflicker` (frame level = mean Rec. 709 luma; per-frame gain = mean of neighbours within a 1.5x ratio over own, clamped 0.5-2; light/medium/strong = 5/11/25 frames; levels stored as `deflicker.levels` 'media;start;rate;v,...'); the first strength measures the played part of the file (desktop FFmpeg VideoReader, web mediabunny CanvasSink, both 160 px at 30 fps) and the shared compositor multiplies the source by the gain before effects (`LayerDescriptor.brightnessGain`). Desktop inspector buttons, web Transform-tab section, editor API `video.deflicker`; checked against `fixtures/deflicker/cases.json`; a synthetic flickering video's frame-to-frame change drops 14.2 to 0.4 (desktop) and 7.8 to 1.1 (web). Browser and FFmpeg decode the same file to slightly different levels, so levels measured in one editor correct a little less in the other |
| 77 | Rich text: different colour, weight or size for words inside one text clip | editor UX | CapCut 9.2.0.3931 draft model has `rich_text_command` (capcut-recon E0019) and `text_range` | both | M: styled ranges in the text model, layout and both renderers; caption word styles (row 65) reuse it | None | Own code | DONE 2026-11-24: the `richText` param ({text, spans:[{start,end,color?,bold?,italic?,scale?}]} in characters) and new crate `crates/richtext` (per-character styles, rebasing onto edited text by common prefix/suffix, style/clear a range, canonical spans), used by the desktop rasterizer and by the web through WASM (`richTextStyles`, `richTextStyleRange`, `richTextClearRange`); both renderers lay each line out piece by piece (rich style, then caption keyword, then spoken word) on one baseline set by the largest piece; desktop inspector Selection row on the Text field's selection, web Text-tab 'Style words' section, editor API `text.styleRange` / `text.clearStyles`; checked against `fixtures/rich-text/cases.json`; the same text lands within 3 px in both editors. Not done: underline or strike per range, per-range fonts, keyframed styles |
| 78 | Multi-camera editing: sync angles by their audio and switch between them while playing | editor UX | CapCut 9.2.0.3931 draft model has `material_multi_camera` (capcut-recon E0019) | both | L: audio cross-correlation sync, a multicam clip type, an angle viewer and cut-on-switch | None | Own code | DONE 2026-11-24 WITHOUT a new clip type or an angle viewer: new crate `crates/multicam` (onset envelope at 200/s, FFT cross-correlation with parabolic peak, cut lists, program segments, stepped opacity keys 6 ms before each cut); Sync by audio (2+ selected videos, each on its own track; desktop FFmpeg 8 kHz mono, web decodeAudioData 8 kHz) moves the angles by their lag, groups them in params (`multicam.group/angle/cuts`) and mutes all but angle 1; angle buttons (desktop inspector, web Transform-tab Multicam section) add a cut at the playhead and rewrite every angle's opacity keys, so only the chosen angle shows; editor API `multicam.sync`, `multicam.switch`, `multicam.clearCuts`. Checked against `fixtures/multicam/cases.json`; FFmpeg-made recordings 0.75 s apart sync within 10 ms on the desktop and 0.07 ms on the web. Not done: a live grid of all angles, keyboard switching, sound that follows the picture, retimed angles |
| 79 | Face retouch: smooth skin, brighten and even the face | AI | CapCut 9.2.0.3931 draft model has `beauty_face_auto_retouch_info` (capcut-recon E0019), `retouch` and `material_manual_beauty` | web first | M to L: MediaPipe Face Landmarker (Apache-2.0) for the face area, then an edge-preserving blur inside it; desktop needs ONNX Runtime | Model download | MediaPipe Apache-2.0 (model card to confirm) | DONE 2026-11-24 in BOTH editors WITHOUT a model: a `retouch` effect (Smooth, Brighten, Even tone, Radius) whose shared shader (`crates/effects/src/shaders/retouch.wgsl`) finds skin by its Cb/Cr chroma, smooths it with an edge-preserving bilateral blur (centre plus two rings of eight whole-pixel taps), evens its colour and lifts it, leaving everything else exactly as it was; desktop catalogue + render passes, web effect definition (same uniforms, checked against the desktop catalogue); checked against `fixtures/retouch/cases.json` on a real GPU (within 1 level) and the same picture gives the same skin mean and spread in both editors (within 0.15). FACE-ONLY added 2026-11-24: Retouch Area "faces only" with a face track kept on the clip (`faces.track`, new crate `crates/facetrack`: samples at 5 a second, interpolation, soft ovals 0.6 x 0.62 of the box raised by 0.08 h, up to three in the shader's spare uniforms); faces are found by MediaPipe BlazeFace on the web (Face retouch section, API `retouch.faces`) and by FaceFusion's YOLO face model in FaceFusion's Python on the desktop (inspector button; needs FaceFusion); the same portrait and track give identical changed-pixel counts in both editors (face 2616, hand 0) |
| 80 | Retake removal: find repeated takes of the same line in the transcript and keep only the last | editor UX, AI | Descript features page lists "Remove Retakes" (https://www.descript.com/features); Kapwing Smart Cut (https://www.kapwing.com/ai/overview); filler-word removal and text-based editing (row 56) exist here, retakes are ABSENT | both | S to M: compare neighbouring transcript sentences (normalised word overlap), mark earlier near-duplicates, review list, cut through the row 56 cut list; shared crate so both editors pick the same takes | Word timestamps (row 2 / row 24) | Own code; no model | DONE 2026-11-24 in BOTH editors: new crate `crates/retakes` (phrases at sentence ends and 0.7 s pauses; an earlier phrase opening like one of the next three within 30 s with at least 60% of its words found again in order, short chatter between and hesitations before the kept take going with it; false starts inside a phrase), used by the desktop through `editor::transcript_retakes` and by the web through WASM `findRetakes`; Find retakes in both transcript sections strikes them for review and Cut struck words removes them with the captions retimed (row 56); API `transcript.retakes`. Checked against `fixtures/retakes/cases.json` (13 cases from an independent Python script) in Rust and through WASM; a synthetic clip with captions gives the same cut in both editors (0.92 to 2.95 s, kept caption at 0.97 s). Not yet tried on a real recording of retakes (INFERRED that the thresholds suit real speech) |
| 81 | Record in the editor: voice-over from the microphone, webcam, and screen or window, with a teleprompter and record markers | studio, platform | Clipchamp screen and camera recorder (https://clipchamp.com/screen-recorder/); Descript screen recording; Filmora 15.6 Teleprompter and Recording Markers, 15.5.2 Window Recording (https://filmora.wondershare.com/whats-new-in-filmora-video-editor.html); no `getUserMedia` or `MediaRecorder` in this checkout (ABSENT) | web first, then desktop | M: web `getUserMedia` / `getDisplayMedia` plus `MediaRecorder` into OPFS, then import as media at the playhead (voice-over records while the timeline plays); desktop through FFmpeg `dshow` / `gdigrab` (Windows) and `avfoundation` (macOS) | Tests use Chrome fake devices (`--use-fake-device-for-media-stream`); the owner records real input themselves | Browser APIs; FFmpeg LGPL | CONFIRMED |
| 82 | Loudness of the whole mix at export (target -14 / -16 / -23 LUFS, true-peak ceiling) and live audio meters | sounds | CapCut 9.2.0.3931 draft model has `material_loudness` and `loudness_param` (capcut-recon E0019); Filmora 15.1 Auto Normalization (LUFS); Resolve 20 loudness metering (https://www.blackmagicdesign.com/media/partial/release/20250404-02); row 13 normalises one clip only (`apps/web/src/loudness`), the mix and meters are ABSENT | both | S to M: move the BS.1770 maths (`apps/web/src/loudness/r128.ts`) into a shared crate, measure the rendered mix, apply one gain plus a limiter in export; peak and short-term meters in the preview | Export mixers on both sides; row 72 limiter | Own code (ITU-R BS.1770 / EBU R128 are public) | DONE 2026-11-25 in BOTH editors: new crate `crates/loudness` (the BS.1770 maths moved out of the editor crate, plus `master`: gain to the target and a look-ahead true-peak limiter at ceiling minus 0.1 dB, up to 3 passes to within 0.2 LU; and `Meter`: streaming K-weighted 100 ms blocks, momentary, short-term, 300 ms peak); fixture `fixtures/loudness/cases.json` (6 mastering + 2 meter cases from an independent Python reference); desktop: export renders the mix (`media::render_audio_mix`), masters it and encodes the float WAV, dialog Loudness row, `opencut-render --loudness`, meter in the audio device callback shown in the transport; web: WASM `masterMix` / `LoudnessMeter`, `createTimelineAudioBuffer({ loudness })`, export dialog Mix loudness select, API `export.render` `loudness` + `mastered`, meter tap in the audio manager shown in the preview toolbar. Real exports: desktop -24.3 to -14.07 LUFS / -1.18 dBTP measured in the MP4; web -24.5 to -14.01 LUFS / -0.99 dBTP |
| 83 | Track mute, solo, volume and pan, and a small mixer panel | sounds, editor UX | CapCut 9.2.0.3931 draft model has `material_audio_track_index` and `material_audio_balance` (capcut-recon E0019); standard in Premiere, Resolve and Shotcut; no solo or track volume in this checkout (ABSENT; clip volume and pan exist, row 60) | both | S to M: track-level gain and pan applied in both mixers, solo set, meters per track from row 82 | Row 82 for meters | Own code | DONE 2026-11-25 in BOTH editors: new crate `crates/trackmix` (audible = sounding and not muted and (no solo or soloed), mute wins; volume -60 (silent) to +12 dB; pan after the clip's own, StereoPanner maths); fixture `fixtures/trackmix/cases.json` from an independent Python reference; desktop: Track `solo`/`volume`/`pan`, `Command::SetTrackMix`, plan_audio skips silenced tracks and carries track gain/pan, mixer pans clip then track and keeps a `loudness::Meter` per track, export folds gain into clip gain/keys and adds a second pan filter, bundles carry the fields, s button on track headers, Mixer section in the inspector (nothing selected); web: WASM `trackMix`, all three collectors, per-track buses (gain, StereoPanner, meter tap) in the player, `SetTrackMixCommand`, Mixer panel in the properties empty view, S on track headers, API `timeline.setTrackMix`. Real tones: desktop live and export -12.02 dB left / silent right at -6 dB pan -1; web export -15.03 dB left (expected -15.03) / silent right |
| 84 | Audio crossfade between neighbouring clips in one step (equal power or linear) | sounds | Standard in every surveyed editor; row 60 has fade handles per clip but nothing joins two clips (ABSENT) | both | S: overlap the two clips by the fade length and set matched fade-out / fade-in curves through the existing fade params | Row 60 | Own code | DONE 2026-11-24 in BOTH editors: new crate `crates/crossfade` (ticks; a clip's `crossfade` seconds and `crossfadeCurve` join it to the next audio or video clip starting within 10 ms of its end; for playback both run over the cut into their handles, centred when both have enough, so the timeline never overlaps; equal power sin/cos or linear; the one-step command trims the two for handles where short and moves the incoming clip and those after it on the track left; at most 10 s; no speed-changed clips); fixture `fixtures/crossfade/cases.json` (21 cases from an independent Python reference); desktop: `editor::crossfade_commands`, `plan_audio` plays `crossfaded_elements`, fade curves in `AudioClipPlan` and FFmpeg `afade` qsin in exports, inspector Audio rows; web: WASM `audioCrossfades` / `planCrossfade`, `withAudioCrossfades` in every audio path, Crossfade section, API `audio.crossfade`. Real tones: equal power flat (desktop live and export within 0.06 dB, web export flat), linear dips 2.96 dB desktop / 2.95 dB web mid-join |
| 85 | Face blur and pixelate for privacy, following each face | effects, AI | Filmora 14 "AI Face Mosaic"; ABSENT here, but the face track from row 79 (`crates/facetrack`, `faces.track`) already follows faces in both editors | both | S: a "face blur" effect (blur, pixelate or solid) drawn inside the face ovals the retouch shader already receives; optional larger oval; pick faces to leave clear | Row 79 face track; desktop needs FaceFusion for finding faces, or YuNet | BlazeFace from MediaPipe (Apache-2.0, as shipped for row 79); YuNet MIT (https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) | DONE 2026-11-24 in BOTH editors: a `face-blur` effect (Style blur / pixelate / black, Strength, Size, Soft edge) whose shared shader (`crates/effects/src/shaders/face_blur.wgsl`) hides each face oval of the clip's face track (row 79's, grown by Size, up to three) with a two-pass Gaussian, 4 x 4-tap blocks or black, at a reach of Strength x half the oval's smaller radius so far and near faces are hidden alike; frames without faces are untouched. Faces button pair in both editors (Retouch faces / Blur faces), API `faces.blur`; checked against `fixtures/face-blur/cases.json` on a real GPU (within 1 level) and the same portrait and track give the same changed pixels in both editors (blur 19256, black 19076; pixelate within 2) |
| 86 | Background blur behind a person (portrait mode) using the cutout matte | effects, AI | CapCut 9.2.0.3931 draft model has `ai_background` and `editing_background` (capcut-recon E0019); the web cutout matte exists, a blur behind it is ABSENT; desktop matte making is PARTIAL | web first | S to M: blur the clip and composite the sharp matte area over it in one effect pass; desktop once its matte making lands | Cutout matte (web), desktop matte model | Same models as the existing cutout | DONE 2026-11-25 in BOTH editors (rendering; the matte is still made in the web editor): the cut-out gains `backgroundBlur` (the shared Blur effect's 0-100 intensity; default 60). Both renderers draw the clip twice: whole, without its cut-out, through the Blur effect after its own effects, then the sharp cut-out over it (`editor::background_blur_layer`, web `backgroundBlurElement` in the scene builder), so the blur is the shared effects crate on both sides; with `inverted` the person is blurred instead. Desktop inspector Hidden / Blurred + Blur amount; web Remove background section switch + slider; API `cutout.setBackgroundBlur`. Striped 640x360 clip with a left-half matte at 60: desktop and web identical, kept half 129.5 +- 127.5, blurred half 127.5 +- 1.1; off hides it (0); inverted swaps them |
| 87 | Video scopes: waveform, RGB parade, vectorscope and histogram | effects, editor UX | Standard in Resolve and Premiere colour tools; colour correction (row 16) and colour match (row 68) have no measuring view; ABSENT | both | M: compute from the preview frame on the GPU (compute or a reduction pass) and draw in a panel; same bins on both sides | Rows 16 and 68 | Own code | CONFIRMED | DONE 2026-11-25 in BOTH editors: new shared crate `crates/scopes` (point-samples the frame at most 384 columns wide, whole-number Rec. 709 luma, draws waveform, RGB parade, vectorscope with 75% bar targets and a skin tone line, and histogram as an RGBA picture, plus luma 1%/99% and the share with a channel at 0 or 255). Desktop: the render thread draws the open scope from each preview frame (bottom bar Scopes, column beside the frame); web: WASM `drawVideoScope` reads the preview canvas after each frame (at most every 100 ms while playing), toolbar button + panel beside the preview; API `preview.scope` and `preview.setSettings {scope}`. Computed on the CPU from the sampled frame rather than the GPU (under 2 ms). Python reference `fixtures/scopes/cases.json` (6 frames, picture hashes) passes in Rust and through WASM; 75% bars light every vectorscope target on both; desktop renderer and web read the same 640x360 bars frame identically (57600 samples, luma 1% 7, 99% 247, 57.2% with a channel at 0) |
| 88 | HSL secondary and hue curves (hue vs saturation, hue vs hue, hue vs luma) and colour wheels | effects | CapCut 9.2.0.3931 draft model has `material_hsl`, `material_hsl_curves`, `material_primary_color_wheels` and `material_log_color_wheels` (capcut-recon E0019); row 16 has lift / gamma / gain and curves but no hue-selective control (ABSENT) | both | M: one shared shader with a per-hue lookup (eight hue bands as CapCut-style sliders, plus curves); wheels as a UI over the existing lift / gamma / gain | Row 87 helps tune it | Own code | CONFIRMED | DONE 2026-11-25 in BOTH editors: new shared shader `hsl` (crates/effects/src/shaders/hsl.wgsl): eight bands (red 0, orange 30, yellow 60, green 120, aqua 180, blue 240, purple 270, magenta 300 degrees), each hue shift (+-30 degrees), saturation and lightness (-100..100), blended between neighbouring bands with a smoothstep, so the 24 values are also the hue vs hue / saturation / lightness curves; greys untouched, lightness scaled by chroma. The effect uniform block grew from 16 to 24 numbers (s4, s5). Catalogue effect `hsl` (params redHue..magentaLightness); desktop inspector band swatches + three curves drawn + the picked band's rows; web HslHeader (swatches + SVG curves). Colour wheels over lift / gamma / gain on both (offsets sum to zero, red/green/blue at 105/225/345 degrees, rim = 0.5; `editor::colour_wheel_*`, web `effects/colour-tools.ts`), written on release. Python reference `fixtures/hsl/cases.json`: GPU exact (0 difference) on 32 colours x 5 cases; desktop renderer and web read the same hue-bar frame identically with blue saturation -100 (58.3% with a channel at 0, from 66.7%). Also fixed: web standalone effect layers never drew (`sceneEffect` sent `effectPassGroups`, the compositor wanted `effect_pass_groups`), and effect elements are now named after the effect ("HSL", not "Hsl"). Log / log-wheel grading (`material_log_color_wheels`) not done |
| 89 | Bloom, glow and light-leak effects | effects | CapCut 9.2.0.3931 draft model has `bloom_params` and `material_light_source` (capcut-recon E0019); text and layer glow exist (row 63), a frame bloom and light leaks are ABSENT | both | S: threshold, blur pyramid and add in the shared effects crate; light leaks as procedural gradients (no bundled footage) | None | Own code; no third-party leak footage | CONFIRMED | DONE 2026-11-25 in BOTH editors: Bloom = shared passes bloom-extract (threshold, smoothstep knee, premultiplied), the shared Gaussian blur (radius = 2 sigma at 1080 high) and bloom-combine (intensity x tint added in premultiplied terms; alpha grows to hold the glow), which reads the effect's untouched input through the new `@source` table key (`effects::SOURCE_TABLE`, re-exported as `compositor::EFFECT_SOURCE_TABLE`; web passes `lut: {key: "@source"}`, never uploaded). Light leak = one procedural shader `light-leak` (two gaussian glows drifting from the edge at `angle`, flicker, screen blend), colours Warm/Golden/Rose/Cool from `editor::LIGHT_LEAK_COLOURS`. Generic catalogue rows on both. Python reference `fixtures/glow/cases.json`: extract/combine/leak within 1 level on the GPU; bloom chain glows round a lit square and leaves darker frames byte-identical. Desktop renderer and web read the same frames identically (bloom centre 11 -> 18, 91.67% -> 87.92% with a channel at 0; leak 2.5 s in 40.53% / 5.94%). Also fixed on the web: effect elements on an effect track got no time (grain, glitch and leaks stood still there). |
| 90 | Freehand drawing on the video (pen, marker, highlighter) that can draw on over time | editor UX | CapCut 9.2.0.3931 draft model has `material_handwrite`, `segment_handwrite` and `keyframe_handwrite` (capcut-recon E0019); ABSENT (the picture clean-up brush is not a layer) | both | M: a stroke element (points with pressure and time), rendered as a shared tessellated path; a "reveal" param draws it on | None | Own code | CONFIRMED | DONE 2026-11-25 in BOTH editors: new shared crate `crates/strokes` (Tool pen/marker/highlighter, points [x, y, pressure, time] in canvas units, JSON in the element's `strokes` param, push_stroke times strokes one after another, CPU rasteriser of anti-aliased round-capped segments with pressure radius and per-stroke max coverage, reveal by recorded time). A drawing is the web graphic element `definitionId: drawing` (hidden from the shapes list; source size = canvas via `GraphicDefinition.sourceSize`; `timedParams` gives the draw-on reveal from `drawOn` seconds) and the desktop's new `ElementKind::Graphic { definition_id }` (draws drawings; other web shapes kept, not drawn). Draw mode on both previews (web toolbar pen button + brush bar over the preview, desktop bottom-bar Draw + tools/colours/S M L), strokes join the selected drawing under the playhead or start one; Remove last stroke; desktop inspector Draw on over + Remove last stroke; API drawing.addStroke / drawing.removeLastStroke; WASM rasteriseDrawing etc. Python reference `fixtures/strokes/cases.json` (5 cases, within 1 level in Rust and through WASM). A real web drag made a drawing; desktop renderer and web read the same strokes identically (luma 1% 32, 99% 109, 0.7135% at 255); drawn on over 2 s, 0.5 s in: 81 web, 80 desktop. Desktop drawing not tried by hand (mouse only, pressure 0.5); web pen pressure untested on a real stylus |
| 91 | Text on a path (the remainder of row 74) | editor UX | CapCut 9.2.0.3931 draft model has `text_typesetting_paths` and `text_typesetting_path_cmd` (capcut-recon E0019); row 74 shipped arcs only | both | M: a path (bezier points) on the text element and per-glyph placement along it in `crates/textwarp`, shared through WASM | Row 74 | Own code | CONFIRMED | DONE 2026-11-25 in BOTH editors: `textwarp::warp_path` lays the flat text raster along a path of cubic Bézier segments (sampled 32 pieces each, scaled so its length is the text width, origin = the text's anchor; each output pixel takes its nearest piece through a grid, flat x = distance along, flat y = side distance; band h/2 + 1; alpha-weighted bilinear). Presets wave / rise / swoosh / circle (an arc over the top, Path amount = share of a turn) and custom points (`textPathPoints`, 3k + 1). Text params `textPath`, `textPathAmount`, `textPathPoints` (take the place of `curve`); desktop TextStyle.path + inspector Path buttons + Path amount row; web params registry select + number in the text panel, `textPathAt`, `bentRect`/`drawCurvedText` with a path, WASM textPathPoints / textPathSize / bendTextAlongPath; API `text.setPath`. Python reference `fixtures/text-path/cases.json` (6 shapes; points exact, pixels within 1 in Rust and through WASM). Web real clicks: Path = Wave in the text panel; circle via API. Deviation from the plan: the raster is warped along the path rather than each glyph placed and rotated (letters bend slightly with tight curves); no on-canvas path handle editing yet (custom points through the API) |
| 92 | Masks and corner pin that follow motion tracking, and surface (planar) tracking | effects | CapCut 9.2.0.3931 draft model has `surface_tracking`, `surface_tracking_config` and `material_video_tracking` (capcut-recon E0019); Premiere 2026 forward and backward mask tracking and 3D perspective mask tracking (https://blog.adobe.com/en/publish/2026/01/20/new-ai-powered-video-editing-tools-premiere-major-motion-design-upgrades-after-effects); row 71 tracks position only, no masks; ABSENT | both | M to L: first apply a row 71 track to mask position; then a four-point planar track (homography from tracked features) driving row 73 corner pin keys | Rows 71 and 73 | Own code | CONFIRMED | DONE 2026-11-25 in BOTH editors: tracking runs several points in one decode pass (web `trackPoints`, desktop `track_points`). Tracked corner pin: the pinned clip's four corner pin corners at the playhead are tracked as four points and keyed (`tracking::corner_keys`, joint RDP within 1 px; corners converted to the clip's box units at each key time); web Follow motion panel button + API `tracking.cornerPin`, desktop multi-selection button, `editor::tracked_corner_pin_command`. Tracked mask: masks have no keyframes, so the mask's centre motion is stored on it (`track` param, `tracking::MaskTrack` sampled 30/s) and both renderers add `mask_offset` to centerX/centerY at the clip's local time (web frame-descriptor `withMaskTrack`, desktop frame plan); web masks tab Track mask / Stop following + API `tracking.mask`, desktop inspector Mask row. Checked: crate tests, desktop FFmpeg box video tracking two points, web real click on a synthetic moving-box video (90 frames, 0 lost, offsets 0.0042 of the frame per frame = 80 px/s), web tracked corner pin keys 22.10 %/s vs 22.22 expected. Not done: a true planar track (homography fitted to many features); the four corners are four point tracks. Mask handles in the web preview show the untracked place |
| 93 | Slip, slide and roll edit tools | editor UX | Standard in Premiere, Resolve, FCP and Kdenlive; ripple and trim exist, slip / slide / roll are ABSENT | both | S to M: three timeline drag modes over the existing trim commands, one undo step each | None | Behaviour only | CONFIRMED | DONE 2026-11-25 in BOTH editors: new shared crate `crates/trimedit` (ticks; slip, slide, roll with limits from the media (video/audio `limited`, trims >= 0), a one-frame minimum, neighbours touching within 1200 ticks or the gap beside, speed-changed clips refused; `plan` finds the neighbours). Desktop: `editor::trim_tool_command` (changes applied so no two clips overlap on the way), timeline toolbar Move / Slip / Slide / Roll, clip drags in a tool mode apply on release (no live ghost), roll at the grabbed half. Web: WASM `trimEditPlan`, `timeline/trim-tools.ts` (tool store, preview while dragging, commit as one snapshot undo step), toolbar picker, clip press wrapper in the timeline, API `timeline.trimEdit`. Python reference `fixtures/trimedit/cases.json` (17 cases) equal in Rust and through WASM. Web real drags: Slip (trim 1.5 -> 1.0 s, clip in place) and Slide (+0.5 s: neighbours 3.5 and 2.5 s); Roll -1 s via API. Desktop drags not tried by hand |
| 94 | Caption find and replace, batch restyle by search, and auto emoji in captions | captions | CapCut 9.2.0.3931 draft model has `subtitle_batch_editing_info`, `ai_emoji_config` and `emoji_sticker_info` (capcut-recon E0019); caption find / replace and emoji are ABSENT | both | S: find / replace across caption elements in one command; emoji from a small keyword list (own data) or the Claude caption pass (paid, approval first) | Emoji font (Twemoji, row 35) | Own word list; Twemoji CC-BY 4.0 graphics | CONFIRMED | DONE 2026-11-25 in BOTH editors: new shared crate `crates/captionedit` (find: whole words in order, case and edge punctuation ignored, no overlaps; replace keeps the edge punctuation and shares the matched span's time by token length + 1, empty replacement removes, keywords kept; highlight sets keywords; auto emoji from our own ~90-word list (very common words left out) adds one zero-length emoji word after the first fitting word per caption, skipping captions that have one; remove emoji). Desktop: ElementPatch.words, `editor::caption_edit_command` / `caption_match_count`, caption inspector Find / Replace with + Replace all, Highlight matches, Add emoji, Remove emoji. Web: WASM captionEdit / captionFindCount, `captions/find-replace.ts`, Captions tab block, API `captions.findReplace`. Batch restyle by search = Highlight matches (keyword colour and size); no Claude emoji pass (would be paid). Python reference `fixtures/captionedit/cases.json` (14 cases) equal in Rust and through WASM; the desktop renderer draws emoji (Segoe UI Emoji); web real typing + clicks: "the plan" -> "our new roadmap" with the time split, then Add emoji put a rocket after "launch" |
| 95 | Subtitle import: WebVTT on the web, SRT and VTT on the desktop | captions | Web imports SRT only; desktop has no subtitle import; both export SRT / VTT (row 14); PARTIAL | both | S: one parser in a shared crate (SRT, VTT cues and timing) used by both through WASM | None | Own code | CONFIRMED |
| 96 | More export kinds: GIF, audio only (WAV, AAC, Opus), still PNG on the desktop, image sequence | platform | GIF and audio-only export are ABSENT; the web exports a still PNG and the desktop does not (PARTIAL) | both | S to M: GIF through a palette quantiser (own code, or FFmpeg `palettegen` on the desktop); audio through the existing mixers into WAV or WebCodecs AAC / Opus; PNG and sequence from the frame renderer | None; MP3 left out (encoder licence not checked) | Own code; FFmpeg LGPL | CONFIRMED |
| 97 | Transparent video export (WebM VP9 with alpha; ProRes 4444 and DNxHR on the desktop) | platform | Needed for overlays and lower thirds made here and used elsewhere; ABSENT | desktop first | M: desktop FFmpeg `prores_ks` / `dnxhd` with an alpha-preserving render target; web VP9 alpha through WebCodecs is UNKNOWN (browser support for encoding alpha not checked) | Render with a transparent background on both sides | FFmpeg LGPL | CONFIRMED desktop; web UNKNOWN |
| 98 | Credit roll (end credits) and a text template library (titles, lower thirds, subscribe cards) | editor UX, assets | CapCut 9.2.0.3931 draft model has `material_tail_leader`, `segment_tail_leader`, `material_text_template` and `material_combo_text_template` (capcut-recon E0019); templates exist as starter projects only (PARTIAL), credit roll ABSENT | both | S to M: a scrolling text element (speed or fit to duration); a template is a saved group of text, shape and animation presets (rows 62 and 63), own designs only | Rows 62, 63, 77 | Own designs; Fontsource fonts with their licences | CONFIRMED |
| 99 | Word-art styles: gradient fill, several outlines, inner shadow, a style gallery | editor UX | CapCut 9.2.0.3931 draft model has `material_flower` with `material_flower_fill`, `material_flower_stroke`, `material_flower_shadow` and `material_flower_single_char_bg` (capcut-recon E0019); one outline and no gradient fill today (PARTIAL) | both | S to M: gradient fill and up to three strokes in both text rasterisers; presets are own JSON | Row 77 rich text renderers | Own designs | CONFIRMED |
| 100 | Audiogram: an animated waveform or bars layer driven by the audio, for podcasts | editor UX | Kapwing audiogram (https://kapwing.com/ai/clip-maker/podcast/audiogram); Filmora 14 Audio to Video; ABSENT | both | S to M: a generated element that reads an audio clip's level or spectrum per frame (shared FFT from `crates/multicam` / `crates/voice`) and draws bars, line or circle | None | Own code | CONFIRMED |
| 101 | Fit music to the video's length: shorten or extend a track at beat-matched joins | sounds | Resolve 20 Music Extender (https://www.blackmagicdesign.com/media/partial/release/20250404-02); ABSENT; beat detection exists (`apps/web/src/beats`) | both | M: find repeatable bars from the beats and similar-sounding sections, cut or loop between them with short crossfades; no model | Beat detection, row 84 crossfade | Own code; ACE-Step (Apache-2.0, https://huggingface.co/ACE-Step/ACE-Step-v1-3.5B) only if a generative extension is wanted later | CONFIRMED demand; approach INFERRED |
| 102 | Multicam grid view with number keys to switch angles (row 78 follow-up) | editor UX | Row 78 switches from a list; the grid and keys are ABSENT; standard in Premiere and Resolve | both | S to M: a preview mode drawing each angle small, keys 1-9 cut to that angle at the playhead while playing | Row 78 | Own code | CONFIRMED |
| 103 | Speaker labels in captions and switching multicam to whoever is talking | AI, captions | Resolve 20 SmartSwitch and IntelliCut split by speaker (release page above); Descript Automatic Multicam; ABSENT | desktop first | L: pyannote speaker-diarization-3.1 in a desktop Python helper (as FaceFusion is used for faces), labels kept on caption chunks, then row 78 cuts per speaker; web through a smaller embedding model is UNKNOWN | Hugging Face terms acceptance by the owner for the model; Python environment | pyannote MIT, gated (https://huggingface.co/pyannote/speaker-diarization-3.1); its precision-2 pipeline is a paid API and out | CONFIRMED |
| 104 | Click-to-select object mask that follows the object (segment anything) | AI, effects | Premiere 2026 Object Mask (blog above); CapCut Auto Cutout; Resolve Magic Mask (Studio only); ABSENT | web and desktop | L to XL: SAM 2 image encoder and mask decoder through ONNX (web: onnxruntime-web WebGPU; desktop: ONNX Runtime), point prompts, mask carried frame to frame; then use it as a clip mask or with row 86 | Model files over 25 MiB need R2 (section 4 note); download needs the owner's yes | SAM 2 Apache-2.0, code and checkpoints (https://github.com/facebookresearch/sam2) | CONFIRMED |
| 105 | Search media by what is in it ("dog on a beach") and by what is said | AI, editor UX | Premiere Media Intelligence (2025 blog, https://blog.adobe.com/en/publish/2025/04/02/introducing-new-ai-powered-features-workflow-enhancements-premiere-pro-after-effects); CapCut Smart Search (https://www.capcut.com/tools/desktop-ai-power); ABSENT | web first | M to L: SigLIP image embeddings for sampled frames kept per media item, text embedding of the query, plus transcript search; desktop through ONNX Runtime | Model download (owner's yes); row 24 transcripts | SigLIP Apache-2.0 (https://huggingface.co/google/siglip-base-patch16-224) | CONFIRMED |
| 106 | Smooth jump cuts: blend across a cut in a talking-head clip with frame interpolation | AI, effects | Descript "Smoother jump cuts" (https://feedback.descript.com/changelog/release-round-upaugust-5th-2026); ABSENT; the desktop has frame interpolation (row 47) | desktop first | M to L: synthesise a few frames between the last frame before and the first after the cut with the existing interpolation; web UNKNOWN until an interpolation model runs there | Desktop frame interpolation | Same as the existing interpolation | CONFIRMED demand; quality UNKNOWN |
| 107 | Stabilisation on the web (the desktop has `deshake` / vid.stab, row 46) | effects | Row 46 is desktop only; web stabilisation is ABSENT | web | M to L: global motion per frame from the `crates/tracking` feature tracker, smoothed path, compensating transform; shared through WASM so the desktop can use the same path | Row 71 tracker | Own code | CONFIRMED |
| 108 | Large files on the web: stream audio and keep memory flat for long or 4K media | platform | OpenCut #593, #605, #656 and #594 (open); the web decodes each clip's whole audio into an `AudioBuffer` (PARTIAL; proxies exist, row 30) | web | M to L: decode audio in chunks on demand for playback and export, cache waveform peaks only; measure memory with a two-hour synthetic file | Row 30 | Own code | CONFIRMED |
| 109 | Desktop installers and release builds (Windows MSI, macOS DMG, Linux AppImage or Flatpak) from CI | platform | OpenCut #516 and #174 (open); no release workflow for the desktop in this checkout (ABSENT) | desktop | M: a tagged-release workflow building each target, bundling FFmpeg (LGPL build) and the licence file; signing is a later step needing the owner's certificates | GitHub Actions minutes on the fork; code-signing certificates (owner) | FFmpeg LGPL build notice in the package | CONFIRMED |
| 110 | Python client for the editor API (pip package over the headless runner and MCP) | studio coupling | OpenCut #214 "Python API" (open); the headless runner and MCP server exist (PARTIAL) | any | S to M: a thin typed client generated from `apps/web/reference/editor-api.md` / `types.ts`, examples and tests against the headless runner | Headless runner | Own code, MIT | CONFIRMED |
| 111 | Project interchange: export and import EDL, FCPXML and OpenTimelineIO | platform | Asked for by people finishing in Resolve or Premiere; ABSENT | both | M: a shared crate mapping tracks, clips, trims and simple transitions to OTIO JSON, with CMX 3600 EDL and FCPXML writers; effects not carried (stated in the export) | None | OpenTimelineIO Apache-2.0 (format reference; no dependency needed) | CONFIRMED gap; demand INFERRED |
| 112 | Caption translation on the desktop (the web has it, row 40) | captions, AI | CapCut 9.2.0.3931 draft model has `material_ai_translate` and `multi_language_material_refs` (capcut-recon E0019); Premiere auto-translated captions in 27 languages (2025 blog); desktop translation is ABSENT (PARTIAL overall) | desktop | M: the web's Opus-MT models through ONNX Runtime on the desktop, or MADLAD-400-3B for more languages (large) | Model download (owner's yes) | Opus-MT as on the web; MADLAD-400 Apache-2.0 (https://huggingface.co/google/madlad400-3b-mt) | CONFIRMED |
| 113 | Brand kit: saved colours, fonts and logos applied to text, captions and templates | assets, studio coupling | Clipchamp brand kits (https://clipchamp.com/en/whats-new); Descript Brand Studio (features page); ABSENT | both | S to M: a kit stored with the user's settings (and in shared bundles), pickers that show kit colours first, one-step apply to a selection | Rows 98 and 99 make it more useful | Own code | CONFIRMED |

## 3. Studio coupling

The Shortform Studio pipeline (`prompts/13-shortform-studio-master.md`,
`docs/SHORTFORM_STUDIO_BLUEPRINT.md` in the sibling repository, both read)
already fixes three contracts that ConzaCut should treat as its own: the
OpenCut project JSON (format version 1, 120,000 ticks per second, flat `params`
with `animations`, `effects`, `masks`), a native render CLI over the `editor`,
`render` and `media` crates, and the editor's caption chunking. The backlog
rows below serve both products through one implementation, and the shared
interfaces are deliberately small.

Backlog rows that serve both (CONFIRMED against the blueprint's phases):

- Row 10 `opencut-render` and row 20 QA stills are the blueprint's Phase 1
  renderer and its QA gate.
- Row 11 Editor API methods make the headless fallback renderer usable for
  keyframed, captioned projects, which the blueprint says it is not today.
- Rows 2, 3, 14 word timestamps, karaoke style and SRT export are the
  blueprint's captions and `SRT generated from the caption chunks`.
- Rows 4 and 13 ducking and loudness are the blueprint's "Music is ducked under
  the voice" and "Voice at 0 dB reference".
- Rows 15, 16, 38 LUT, colour and tint are the "tint to the style palette"
  post-process and the depth-layer look.
- Rows 37 and 38 cut-outs and path tracing are the picture-list stage.
- Rows 39, 24 and 2 give the voice stage a free in-browser TTS plus local word
  timings, matching the blueprint's "browser Whisper plus a deterministic script
  aligner".
- Rows 1 and 19 export presets and safe areas are the blueprint's platform
  presets and the top 8 % and bottom 18 % safe-area validator.
- Rows 18 and 16 transitions and colour give the studio's later presets
  (Cinematic, Fast Facts) something to use without a second renderer.
- Rows 41, 42, 43, 51, 52 stock search, shot lists, templates, "Open in
  ConzaCut" and the asset manifest are the hand-off and rights plumbing.

Minimal shared interfaces (INFERRED from the two codebases and the blueprint;
field names are proposals):

1. Project JSON, format version 1, kept as is. Add only optional fields that
   old readers ignore: `element.anchors` as a map of word id to time in ticks
   (so overlays can be re-timed when the voice changes); a `transition`
   element on video tracks with `{ catalogueId, duration, params }`; new
   `effects[]` ids (`lut`, `colorCorrect`, `grain`, …) with flat `params`;
   ducking expressed as ordinary `volume` keyframes, which both editors already
   evaluate (CONFIRMED in `apps/desktop/README.md` and `crates/README.md`).
   Pin a schema copy to a commit hash in the studio, as the blueprint demands.
2. Render CLI flags: `opencut-render --project <dir or .opencut.zip> --out
   <file.mp4> [--fps N] [--size WxH] [--range start:end] [--frame-at t ...]
   [--stills every=0.5s,at=t1,t2]` with JSON progress lines of frames encoded
   over total frames (the blueprint forbids timer-driven progress) and an
   ffprobe-style validation summary at the end.
3. Asset manifest (`assets.json` inside the bundle): `{ id, path, kind,
   license, source, attribution, origin: "mock" | "demo" | "live" }`. The
   studio writes it; both editors show it; exporters append attribution text
   for CC-BY material.
4. Caption chunk JSON: `{ text, startMs, endMs, words: [{ word, startMs,
   endMs, id }] }` produced by the editor's worker (row 2) and consumed by
   `buildCaptionChunks`, the SRT and ASS writers and the studio's aligner.
5. Fonts by file, not by name: the studio and the desktop render the same
   text only if the font file is the same, so both resolve families through
   the Fontsource catalogue (row 8) and bundle the font file.

## 4. Declined and deferred

Declined, with the reason:

- **CapCut features left out on 2026-11-21:** digital humans and lip sync
  (`material_digital_human`, `mouth_shape_driver`), relighting
  (`material_smart_relight`), AI expand and in-painting (`ai_in_painting`,
  `outpainting_ratio`), body reshaping (`figure_slim`, `figure_stretch`) and
  lyric videos (`lyric_*`). The first three need paid generation or model
  weights whose licences were not checked; body reshaping is low value and
  open to misuse; lyric videos are a template product. They can be revisited
  on request.

- **Any Remotion runtime dependency** (`@remotion/transitions`,
  `@remotion/captions`, `@remotion/webcodecs`, templates). Remotion's licence is
  free only for individuals, non-profits and for-profits with up to three
  employees; companies of four or more need a Company License, and the
  Remotion-based "react-video-editor" adopted the same two-tier terms
  (CONFIRMED). Use the designs as references only.
- **Shader code from Shadertoy, lygia, frei0r, MLT, Kdenlive, Olive, Shotcut.**
  Shadertoy's terms page is behind a bot wall (UNKNOWN) and third parties
  describe the default as CC BY-NC-SA; lygia is the Prosperity non-commercial
  licence; the rest are GPL or LGPL (CONFIRMED). Feature names are fair input.
- **RobustVideoMatting, `@imgly/background-removal`, Ultralytics YOLO,
  essentia.js, aubio, Video2X.** GPL-3.0, AGPL-3.0, AGPL-3.0, AGPL-3.0, GPL-3.0
  and AGPL-3.0 respectively (CONFIRMED); Ultralytics states an enterprise
  licence is required unless the whole project is open-sourced under AGPL.
- **BRIA RMBG weights** (rembg's default): paid agreement for commercial use
  (CONFIRMED).
- **Piper's GPL-3.0 successor, Fish Audio, F5-TTS weights, Coqui XTTS,
  NLLB-200.** GPL, revenue-capped community licence, CC-BY-NC weights, CPML
  (terms UNKNOWN, page 404) and CC-BY-NC respectively (CONFIRMED where read).
- **Edge TTS and Web Speech export.** Edge TTS is an unofficial Microsoft
  endpoint with recurring 403 breakage and unknown terms; `speechSynthesis`
  cannot capture audio to a file (CONFIRMED).
- **A Google Fonts API key in the repository.** Google's API terms say
  "Developer credentials may not be embedded in open source projects"
  (CONFIRMED). Fontsource replaces it.
- **Bundling Pixabay, Pexels, Unsplash, Mixkit, Uppbeat or Sonniss material.**
  Pixabay forbids standalone redistribution; Unsplash and Pexels forbid
  compiling a competing collection; Mixkit and Uppbeat have no API and
  prohibit making items available to third parties; Sonniss GDC bundles may
  not be supplied "as part of a sound effects library ... software development
  kit" (CONFIRMED; Uppbeat and Pexels terms pages were unreadable).
- **Scraping LottieFiles' private search endpoints.** No public search API is
  documented; the free-asset licence forbids compiling files for a competing
  service (CONFIRMED).
- **Server-side rendering on Cloudflare Workers.** Workers allow 128 MB per
  isolate, 100 MB request bodies and at most 5 minutes of CPU on the paid plan,
  and cannot spawn processes (CONFIRMED). Rendering stays in the browser or in
  the user's machine or a separate render host.
- **CapCut assets of any kind**, per the task's hard constraint.
- **Left out of round 3 (2026-11-24):**
  - Eye-contact correction (Descript, Kapwing, Filmora): no usable open model
    was found. LivePortrait's code is MIT but its InsightFace models are
    non-commercial and its README shows no gaze control; NVIDIA Maxine Eye
    Contact is proprietary (CONFIRMED where read).
  - CrisperWhisper for filler words: CC-BY-NC-4.0 (CONFIRMED,
    https://huggingface.co/nyrahealth/CrisperWhisper). Filler removal already
    runs on the existing Whisper models.
  - Generative sound effects: Stable Audio Open's community licence keeps its
    commercial terms on a separate page that was not read (UNKNOWN); MMAudio's
    weights are CC-BY-NC (CONFIRMED).
  - Chatterbox voice cloning: MIT but retired as of August 2026 and every
    output carries a watermark (CONFIRMED, https://github.com/resemble-ai/chatterbox).
  - Agent editing by prompt (CapCut EditPilot, Descript Underlord): the MCP
    server and editor API already cover agent use; a hosted agent would be a
    paid API.

Deferred, with the reason:

- **Native mobile app** (row 53). Highest upstream demand but XL effort and no
  mobile GPUI evidence.
- **Real-time collaboration.** Zero upstream requests; y-webrtc is MIT but
  needs a signalling endpoint, and the hosted PartyKit platform shuts down in
  October 2026 (CONFIRMED). Bundle sharing (row 44) covers the need.
- **Stem separation in the browser** (row 48): proven but about five minutes a
  track with threads; desktop first.
- **vidstab stabilisation** until the FFmpeg build question (section 5) is
  settled; `deshake` is the interim.
- **TransNetV2 and large matting models as static assets**: the 25 MiB
  per-file static-asset limit forces R2 or a CDN with CORS headers first.
- **In-browser diffusion for style-locked image sets**: multi-gigabyte
  weights; stays a paid-API or desktop-GPU feature (INFERRED).
- **Video object removal (round 3).** ProPainter is non-commercial (NTU
  S-Lab licence, CONFIRMED, https://github.com/sczhou/ProPainter); DiffuEraser
  builds on ProPainter and states no weights licence; LaMa is Apache-2.0 but
  works on single frames, so results would flicker (INFERRED). Revisit when an
  open video in-painting model has clear terms.
- **Dubbing with lip sync (round 3).** MuseTalk's page allows commercial use of
  the trained model but its bundled parts carry their own licences, and
  LatentSync's weights licence is UNKNOWN; translated captions (row 112) and
  Kokoro voice-over (row 39) come first, and any voice of a real person needs
  that person's consent.
- **Generative extend (round 3).** Wan2.1's first-and-last-frame model is
  Apache-2.0 but 14B parameters (CONFIRMED, https://github.com/Wan-Video/Wan2.1);
  otherwise a paid API. Freeze frames and in-between frames cover short gaps.
- **HDR, colour management, 3D text and real-time collaboration (round 3)**:
  ABSENT and large (XL) with little demand in the sources read; CapCut's
  `hdr_settings` shows the class exists, not that people use it.

## 5. Open questions for the owner

1. **Cross-origin isolation (COOP and COEP headers) on conzacut.conzahub.com?**
   ONNX Runtime Web threads, MediaPipe and in-browser Demucs need
   `crossOriginIsolated`; with `Cross-Origin-Embedder-Policy: require-corp`
   every cross-origin asset (Google Fonts, Freesound previews, Pixabay
   thumbnails, Hugging Face model files) must carry CORS or CORP headers or it
   is blocked, and the `_headers` file does not apply to Worker-generated
   responses (CONFIRMED). Decides rows 12, 26, 36, 37, 48 and the font and
   sound sources.
2. **Where do model files live?** Workers static assets cap each file at 25
   MiB; Whisper base for WebGPU is about 207 MB, TransNetV2 31 MB, ISNet 179
   MB (CONFIRMED). Provisioning R2 (10 GB-month free, free egress) is the
   obvious answer but needs approval. Decides rows 2, 26, 36, 37, 39.
3. **GPL or LGPL FFmpeg for the desktop?** The pinned build is LGPL, which
   excludes `libvidstab` and anything `--enable-gpl` gates (CONFIRMED). Which
   H.264 encoder the pinned build provides is UNKNOWN from this checkout.
   Decides rows 46 and 49 and the libass burn-in in row 14.
4. **Freesound terms.** Commercial use of the API is negotiated case by case
   with UPF, and whether preview MP3s may appear in rendered videos is UNKNOWN
   (CONFIRMED on the terms page). Decides row 7 and whether sounds must be
   downloaded through OAuth2.
5. **Download size tolerance for captions.** Word timestamps need the
   `_timestamped` exports; base is about 207 MB with the WebGPU recipe and
   turbo about 2.9 GB (CONFIRMED). Default to base, or keep small as default?
   Decides row 2's default.
6. **Remotion position**, for the studio's third renderer option and for the
   `whisper-webgpu` package it is moving to: is the owner's company within
   three employees? Decides whether Remotion is even an evaluation candidate.
7. **TTS vendor and consent policy.** Kokoro in the browser is free and
   Apache-2.0; ElevenLabs is paid, returns character timestamps and its free
   tier is non-commercial with mandatory credit (CONFIRMED). Decides row 39
   versus the studio's Phase 3 fal or ElevenLabs route.
8. **Does ConzaCut track upstream or diverge?** Upstream's last merge was
   2026-05-17, the last maintainer comment 2026-07-17, the last commit
   2026-09-24, and status questions go unanswered (CONFIRMED). Diverging frees
   the project JSON to gain the fields in section 3; tracking keeps bundle
   interchange with a rewrite that may not arrive. Decides the schema policy
   for rows 11, 18, 52.
9. **Language priority for row 32.** Chinese has the clearest upstream demand;
   the owner's audience is UNKNOWN.
10. **Stock media at all?** Upstream users never asked; the studio wants it.
    Decides rows 41 and 42 and whether a Pixabay key is worth the 24-hour
    caching obligation.
11. **Hosting a render worker for the studio** (Docker host, Cloudflare
    Containers or a VM). The blueprint documents but does not provision it.
    Decides when row 10 becomes reachable from Overtime.

## 6. Sources

Grouped by the section of the research prompt they informed. Every URL was
read on 2026-11-04 by this session or one of its research sub-tasks unless
marked otherwise.

Repository files (this checkout and the sibling repository):
`README.md`, `ATTRIBUTION.md`, `crates/README.md`, `apps/desktop/README.md`,
`apps/headless/README.md`, `apps/mcp/README.md`, `apps/web/reference/editor-api.md`,
`apps/web/package.json`, `apps/web/bun.lock`, `apps/web/public/manifest.json`,
`apps/web/src/routes/__root.tsx`, `apps/web/src/services/transcription/worker.ts`,
`apps/web/src/transcription/models.ts`, `apps/web/src/stickers/providers/*.ts`,
`apps/web/src/stickers/categories.ts`, `apps/web/src/effects/definitions/blur.ts`,
`apps/web/src/timeline/types.ts`, `apps/web/src/actions/definitions.ts`,
`apps/web/src/actions/keybindings-store.ts`, `apps/web/src/guides/definitions/platforms.tsx`,
`apps/web/src/subtitles/*.ts`, `apps/web/src/retime/presets.ts`,
`apps/api/src/index.ts`, `apps/api/wrangler.jsonc`, `crates/media/setup/ffmpeg.json`,
`changelog/0.5.0.md`, `changelog/0.6.0.md`;
`C:\Users\Conza\Desktop\overtime-ai-studio\prompts\13-shortform-studio-master.md`,
`...\prompts\13-shortform-studio-followups.md`, `...\docs\SHORTFORM_STUDIO_BLUEPRINT.md`.

Section 1, OpenCut upstream (GitHub API via `gh` and pages):
- https://github.com/OpenCut-app/OpenCut (repo stats, README, PR template)
- https://github.com/OpenCut-app/OpenCut/blob/main/.github/pull_request_template.md
- https://github.com/OpenCut-app/OpenCut/blob/main/apps/desktop/README.md
- https://github.com/OpenCut-app/OpenCut/issues/811 (tracking: rewrite)
- https://github.com/OpenCut-app/OpenCut/issues/14, /480, /652 (mobile)
- https://github.com/OpenCut-app/OpenCut/issues/516, /174, /713 (desktop, packaging)
- https://github.com/OpenCut-app/OpenCut/issues/333, /745, /234 (Docker)
- https://github.com/OpenCut-app/OpenCut/issues/662, /601, /194, /865, /875, /876, /817 (localisation)
- https://github.com/OpenCut-app/OpenCut/issues/778, /214, /719, /273 (API, headless)
- https://github.com/OpenCut-app/OpenCut/issues/742, /573, /868 (AI)
- https://github.com/OpenCut-app/OpenCut/issues/487, /749, /807, /826, /898, /901, /790 (captions)
- https://github.com/OpenCut-app/OpenCut/issues/670, /907, /569, /799, /824, /786 (effects, keyframes)
- https://github.com/OpenCut-app/OpenCut/issues/737, /911, /858, /751, /771 (export, portability)
- https://github.com/OpenCut-app/OpenCut/issues/548, /791, /622, /838 (audio)
- https://github.com/OpenCut-app/OpenCut/issues/473, /906, /884, /736, /801, /730, /627, /789, /732 (timeline, shortcuts)
- https://github.com/OpenCut-app/OpenCut/issues/656, /628, /584, /776, /594 (performance)
- https://github.com/OpenCut-app/OpenCut/issues/916, /920, /890, /863 and https://github.com/OpenCut-app/OpenCut/discussions/861 (status)
- https://github.com/OpenCut-app/OpenCut/issues/714, /192, /847, /197
- https://github.com/OpenCut-app/OpenCut/pull/750, /582, /526, /525, /484, /756, /788, /765, /805, /139, /178, /482, /342, /54, /118, /217
- https://github.com/OpenCut-app/OpenCut/issues/713#issuecomment-4148572699
- https://github.com/OpenCut-app/OpenCut/issues/569#issuecomment-3216656281
- https://github.com/OpenCut-app/OpenCut/pull/139#issuecomment-3011558760
- https://github.com/OpenCut-app/OpenCut/pull/178#issuecomment-3094354446
- https://github.com/OpenCut-app/OpenCut/issues/273#issuecomment-3073176349
- https://github.com/OpenCut-app/OpenCut/issues/628#issuecomment-3955823216
- https://github.com/OpenCut-app/OpenCut/issues/846#issuecomment-5007677154
- https://github.com/OpenCut-app/opencut-classic (archived, 0 issues)
- https://github.com/OpenCut-app/opencut-classic/blob/main/apps/web/src/app/roadmap/page.tsx
- https://github.com/OpenCut-app/opencut-classic/blob/main/apps/web/src/changelog/entries/0.4.0.md
- https://github.com/orgs/OpenCut-app/projects
- https://opencut.app, https://new.opencut.app
- https://api.npmjs.org (opencut-wasm download counts)

Section 2, other open-source editors:
- https://invent.kde.org/multimedia/kdenlive/-/raw/master/LICENSES/GPL-3.0-or-later.txt
- https://docs.kdenlive.org/en/effects_and_filters/speech_to_text.html
- https://docs.kdenlive.org/en/effects_and_filters/video_effects/color_image_correction/applylut.html
- https://docs.kdenlive.org/en/effects_and_filters/video_effects/color_image_correction/lift_gamma_gain.html
- https://docs.kdenlive.org/en/effects_and_filters/video_effects/color_image_correction.html
- https://docs.kdenlive.org/effects_and_filters/video_effects/alpha_mask_keying/object_mask.html
- https://docs.kdenlive.org/en/compositing/transitions/transitions_list.html
- https://docs.kdenlive.org/en/titles_and_graphics/titles/title_template_titles.html
- https://bugs.kde.org/show_bug.cgi?id=392623, https://bugs.kde.org/show_bug.cgi?id=442792, https://bugs.kde.org/show_bug.cgi?id=453881
- https://bugs.kde.org/buglist.cgi?product=kdenlive&bug_severity=wishlist&order=votes%20DESC
- https://invent.kde.org/multimedia/kdenlive/-/issues/1563
- https://raw.githubusercontent.com/mltframework/shotcut/master/COPYING
- https://raw.githubusercontent.com/mltframework/shotcut/master/src/jobs/whisperjob.cpp
- https://github.com/mltframework/shotcut/blob/master/src/qml/extensions/whispermodel.qml
- https://github.com/mltframework/shotcut/blob/master/scripts/build-shotcut.sh
- https://shotcut.org/blog/new-release-241029/, https://shotcut.org/features/, https://shotcut.org/blog/new-release-1608/
- https://raw.githubusercontent.com/mltframework/shotcut/master/src/widgets/lumamixtransition.cpp
- https://raw.githubusercontent.com/mltframework/shotcut/master/src/qml/filters/{lut3d,reframe,stabilize}/meta.qml
- https://github.com/mltframework/shotcut/issues/1827, /417
- https://forum.shotcut.org/t/automatic-silence-removing/32775
- https://mltframework.org/plugins/FilterLift_gamma_gain/
- https://github.com/olive-editor/olive/blob/master/LICENSE, https://olivevideoeditor.org/
- https://github.com/olive-editor/olive/issues/190, /262, /428, /754, /1842, /1231, /2348, /634
- https://raw.githubusercontent.com/olive-editor/olive/master/CMakeLists.txt
- https://raw.githubusercontent.com/jliljebl/flowblade/master/LICENSE, https://github.com/jliljebl/flowblade
- https://raw.githubusercontent.com/jliljebl/flowblade/master/flowblade-trunk/Flowblade/res/help/{proxy,stabilizing}.html
- https://github.com/jliljebl/flowblade/issues/625, /1237
- https://gitlab.gnome.org/GNOME/pitivi/-/raw/master/COPYING, https://www.pitivi.org/, https://www.pitivi.org/manual/transitions.html
- https://gitlab.gnome.org/GNOME/pitivi/-/work_items/2563
- https://raw.githubusercontent.com/mifi/lossless-cut/master/LICENSE, https://github.com/mifi/lossless-cut
- https://github.com/mifi/lossless-cut/blob/master/src/main/ffmpeg.ts
- https://github.com/mifi/lossless-cut/blob/master/src/renderer/src/ffmpegParameters.ts
- https://github.com/mifi/lossless-cut/issues/126, /372, /260, /1398, /1068, /1804, /2934
- https://raw.githubusercontent.com/motion-canvas/motion-canvas/main/LICENSE
- https://raw.githubusercontent.com/motion-canvas/motion-canvas/main/packages/ffmpeg/package.json
- https://github.com/motion-canvas/exporters, https://motioncanvas.io/docs/rendering, https://motioncanvas.io/docs/rendering/video
- https://github.com/motion-canvas/motion-canvas/issues/415, /1218
- https://github.com/remotion-dev/remotion/blob/main/LICENSE.md, https://www.remotion.pro/license
- https://www.remotion.dev/docs/transitions/, https://www.remotion.dev/docs/captions/create-tiktok-style-captions
- https://www.remotion.dev/docs/install-whisper-cpp/, https://www.remotion.dev/docs/whisper-web/, https://www.remotion.dev/docs/webcodecs/, https://www.remotion.dev/templates
- https://github.com/remotion-dev/remotion/issues/5913
- https://raw.githubusercontent.com/omni-media/omniclip/main/LICENSE, https://github.com/omni-media/omniclip
- https://github.com/omni-media/omniclip/issues/22, /5, /3, /8, /24, /7
- https://github.com/Augani/openreel-video, https://github.com/walterlow/freecut
- https://raw.githubusercontent.com/openvideodev/react-video-editor/main/LICENSE
- https://raw.githubusercontent.com/tejaswigowda/ffmpeg-webCLI/main/LICENSE
- https://raw.githubusercontent.com/diffusionstudio/core/main/LICENSE, https://raw.githubusercontent.com/wassgha/rescript/main/LICENSE
- https://github.com/storytold/filmcraft/issues/4078

Section 3, asset ecosystems and licences:
- https://github.com/iconify/api, https://github.com/iconify/icon-sets
- https://iconify.design/docs/api/, https://iconify.design/docs/api/collections.html, https://iconify.design/docs/api/search.html, https://iconify.design/docs/types/iconify-info.html
- https://github.com/antfu-collective/icones
- https://github.com/google/fonts, https://openfontlicense.org/open-font-license-official-text/, https://openfontlicense.org/ofl-faq/
- https://developers.google.com/fonts/docs/developer_api, https://developers.google.com/terms
- https://github.com/fontsource/fontsource, https://fontsource.org/docs/api/introduction, https://fontsource.org/docs/api/fonts, https://fontsource.org/docs/getting-started/cdn
- https://lottiefiles.com/page/license, https://docs.lottiefiles.com, https://dotlottie.io/spec/
- https://www.pexels.com/api/documentation/, https://www.pexels.com/api/
- https://pixabay.com/api/docs/, https://pixabay.com/service/license-summary/, https://pixabay.com/service/terms/
- https://unsplash.com/documentation, https://unsplash.com/license, https://help.unsplash.com/en/articles/2511245-unsplash-api-guidelines
- https://freesound.org/docs/api/authentication.html, https://freesound.org/docs/api/overview.html, https://freesound.org/docs/api/resources_apiv2.html, https://freesound.org/help/tos_api/, https://freesound.org/help/faq/
- https://api.openverse.org/v1/schema/, https://docs.openverse.org/terms_of_service.html, https://make.wordpress.org/openverse/2022/06/17/mitigating-out-of-terms-api-usage/
- https://mixkit.co/license/, https://mixkit.co/llm-info/
- https://github.com/darkyboys/opensource-luts, https://github.com/IRCGraphic/D-Cinelike-and-Normal-Blockbuster-LUTs, https://github.com/slaycamera/luts, https://github.com/jonmatifa/a6000-LUTs, https://www.cgchannel.com/2020/01/download-800-free-3d-luts/, https://quickddl.net/free_color_luts/
- https://kenney.nl/assets/interface-sounds, https://kenney.nl/assets/ui-audio
- https://git.uni-greifswald.de/GAIM/gaim/src/branch/main/assets/Sounds/collection1/INFO.txt (Junkala pack licence text)
- https://sonniss.com/gdc-bundle-license/
- https://unpkg.com/@hugeicons/core-free-icons@4.4.0/package.json and LICENSE.md
- https://lucide.dev/license, https://github.com/simple-icons/simple-icons (LICENSE.md, DISCLAIMER.md)
- https://github.com/twitter/twemoji, https://github.com/jdecked/twemoji, https://github.com/googlefonts/noto-emoji, https://openmoji.org/faq/
- https://github.com/WordPress/openverse/issues/1795 (Free Music Archive API), https://incompetech.com (licences and FAQ pages), https://ccmixter.org/query-api

Section 4, captions and speech:
- https://github.com/ggml-org/whisper.cpp, https://github.com/ggml-org/whisper.cpp/blob/master/include/whisper.h, https://github.com/ggml-org/whisper.cpp/issues/89
- https://huggingface.co/ggerganov/whisper.cpp/tree/main
- https://ffmpeg.org/index.html (8.0 release note), https://ffmpeg.org/ffmpeg-filters.html (whisper, subtitles, ass filters), https://github.com/FFmpeg/FFmpeg/blob/master/libavfilter/af_whisper.c
- https://codeberg.org/tazz4843/whisper-rs, https://docs.rs/whisper-rs
- https://huggingface.co/docs/transformers.js/guides/webgpu, https://huggingface.co/docs/transformers.js/api/pipelines
- https://github.com/huggingface/transformers.js/issues/820
- https://github.com/huggingface/transformers.js-examples (realtime-whisper-webgpu, whisper-word-timestamps)
- https://huggingface.co/onnx-community/whisper-base_timestamped, https://huggingface.co/onnx-community/whisper-large-v3-turbo_timestamped, and the onnx-community whisper-tiny, base, small, large-v3-turbo file listings
- https://caniuse.com/webgpu, https://web.dev/blog/webgpu-io2025 (Safari 26, Firefox 141/145 status)
- https://github.com/m-bain/whisperX, https://ar5iv.labs.arxiv.org/html/2303.00747 (WhisperX paper tables)
- https://github.com/nyrahealth/CrisperWhisper, https://github.com/jianfch/stable-ts, https://github.com/MontrealCorpusTools/Montreal-Forced-Aligner
- https://deepgram.com/pricing, https://www.assemblyai.com/pricing, https://developers.openai.com/api/docs/pricing, https://console.groq.com/docs/speech-to-text, https://elevenlabs.io/pricing/api
- https://elevenlabs.io/pricing, https://help.elevenlabs.io (commercial licence per tier), https://elevenlabs.io/docs/api-reference/text-to-speech/convert-with-timestamps
- https://huggingface.co/hexgrad/Kokoro-82M, https://github.com/hexgrad/kokoro (kokoro-js README), https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX
- https://github.com/rhasspy/piper, https://github.com/OHF-Voice/piper1-gpl
- https://huggingface.co/coqui/XTTS-v2, https://github.com/fishaudio/fish-speech, https://github.com/resemble-ai/chatterbox, https://github.com/canopyai/Orpheus-TTS, https://github.com/nari-labs/dia, https://github.com/SWivid/F5-TTS
- https://github.com/rany2/edge-tts, https://github.com/rany2/edge-tts/issues/458
- https://developer.mozilla.org/en-US/docs/Web/API/SpeechSynthesis
- https://www.w3.org/TR/webvtt1/, https://aegisub.org/docs/latest/ass_tags/, https://github.com/libass/libass (ISC)
- https://github.com/unconv/captacity, https://github.com/jurczykpawel/captions-cli
- https://huggingface.co/facebook/nllb-200-distilled-600M, https://huggingface.co/Helsinki-NLP/opus-mt-en-de, https://developer.chrome.com/docs/ai/translator-api

Section 5, AI features:
- https://github.com/microsoft/onnxruntime, https://github.com/microsoft/onnxruntime/blob/main/js/web/README.md
- https://onnxruntime.ai/docs/tutorials/web/, https://onnxruntime.ai/docs/tutorials/web/ep-webgpu.html, https://onnxruntime.ai/docs/tutorials/web/deploy.html, https://onnxruntime.ai/docs/tutorials/web/env-flags-and-session-options.html
- https://web.dev/articles/coop-coep
- https://developers.cloudflare.com/pages/configuration/headers/, https://developers.cloudflare.com/workers/static-assets/headers/
- https://github.com/imgly/background-removal-js, https://github.com/imgly/background-removal-js/blob/main/packages/web/README.md
- https://github.com/google-ai-edge/mediapipe, https://developers.google.com/edge/mediapipe/solutions/guide
- https://developers.google.com/edge/mediapipe/solutions/vision/image_segmenter, .../image_segmenter/web_js, .../object_detector, .../object_detector/web_js, .../face_detector
- https://storage.googleapis.com/mediapipe-assets/Model%20Card%20MediaPipe%20Selfie%20Segmentation.pdf
- https://github.com/google-ai-edge/mediapipe/blob/master/docs/solutions/autoflip.md
- https://github.com/WyattBlue/auto-editor, https://github.com/carykh/jumpcutter, https://github.com/WofWca/jumpcutter
- https://github.com/ricky0123/vad, https://github.com/snakers4/silero-vad
- https://community.adobe.com/t5/premiere-pro-ideas/automatic-removal-of-silence/idi-p/14274830
- https://www.capcut.com/help/how-to-use-auto-cut, https://www.capcut.com/tools/auto-reframe, https://www.capcut.com/tools/video-background-remover, https://www.capcut.com/tools/vocal-remover
- https://help.descript.com/hc/en-us/articles/10164806394509-Remove-filler-words
- https://github.com/timkulbaev/ai-video-editor, https://github.com/openai/whisper
- https://github.com/Breakthrough/PySceneDetect, https://github.com/soCzech/TransNetV2, https://huggingface.co/edgetools/transnetv2
- https://github.com/Anil-matcha/AI-Youtube-Shorts-Generator, https://docs.ultralytics.com/integrations/onnx/, https://www.ultralytics.com/license
- https://github.com/PeterL1n/RobustVideoMatting, https://github.com/danielgatis/rembg, https://github.com/xuebinqin/U-2-Net, https://github.com/xuebinqin/DIS, https://github.com/ZhengPeng7/BiRefNet, https://github.com/plemeri/transparent-background
- https://github.com/harry0703/MoneyPrinterTurbo, https://github.com/RayVentura/ShortGPT, https://github.com/FujiwaraChoki/MoneyPrinter
- https://github.com/tencent-ailab/IP-Adapter, https://runware.ai/docs/learn/ip-adapters, https://fal.ai/models/fal-ai/ip-adapter-face-id/api
- https://raw.githubusercontent.com/image-rs/image/main/Cargo.toml
- https://github.com/xiph/rnnoise, https://github.com/GregorR/rnnoise-models, https://github.com/shiguredo/rnnoise-wasm, https://github.com/jitsi/rnnoise-wasm, https://github.com/Rikorose/DeepFilterNet
- https://github.com/domchristie/needles, https://github.com/streamonkey/ebur128_wasm, https://github.com/sdroege/ebur128
- https://github.com/aubio/aubio, https://github.com/MTG/essentia.js, https://github.com/meyda/meyda, https://meyda.js.org/audio-features, https://github.com/chrisguttandin/web-audio-beat-detector
- https://github.com/facebookresearch/demucs, https://github.com/sevagh/free-music-demixer
- https://github.com/georgmartius/vid.stab, https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/configure, https://github.com/gyroflow/gyroflow
- https://github.com/xinntao/Real-ESRGAN, https://github.com/thekevinscott/UpscalerJS, https://github.com/hzwer/ECCV2022-RIFE, https://github.com/nihui/rife-ncnn-vulkan

Section 6, effects and transitions:
- https://github.com/gl-transitions/gl-transitions, https://raw.githubusercontent.com/gl-transitions/gl-transitions/master/LICENSE
- https://github.com/transitive-bullshit/ffmpeg-gl-transition
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/doc/filters.texi (xfade, lut3d, haldclut, silencedetect, silenceremove, scdet, loudnorm, arnndn, sidechaincompress, vidstabdetect, deshake, minterpolate)
- https://github.com/patriciogonzalezvivo/lygia (LICENSE.md), https://discourse.vidvox.net/t/user-bennoh-is-stealing-shaders-and-uploading-them-online-with-mit-license/2682 (Shadertoy default licence, secondary)
- https://github.com/hughsk/glsl-noise, https://github.com/ashima/webgl-noise, https://github.com/stegu/webgl-noise
- https://github.com/mrdoob/three.js (LICENSE, examples/jsm/shaders, examples/jsm/postprocessing)
- https://github.com/pmndrs/postprocessing (LICENSE.md, src/effects, src/loaders/LUTCubeLoader.js)
- https://github.com/BabylonJS/Babylon.js (license.md, PostProcesses)
- https://github.com/dyne/frei0r, https://github.com/KDE/kdenlive, https://github.com/olive-editor/olive, https://github.com/mltframework/shotcut (licence files)
- https://raw.githubusercontent.com/ImageMagick/ImageMagick/main/coders/cube.c
- https://ffmpeg.org/pipermail/ffmpeg-trac/2023-June/066367.html
- http://www.quelsolaar.com/technology/clut.html
- https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-24-using-lookup-tables-accelerate-color
- https://shotgunsoftware.zendesk.com/hc/en-us/community/posts/209497368-tetrahedral-interpolation-for-3D-LUTs
- https://github.com/AcademySoftwareFoundation/OpenColorIO, https://github.com/ampas/aces-dev
- https://helpx.adobe.com/premiere/desktop/add-video-effects/effects-and-transitions-library/list-of-effects-and-transitions.html
- https://www.capcut.com/help/recommended-alternative-effects

Section 7, distribution and growth:
- https://github.com/vite-pwa/vite-plugin-pwa, https://vite-pwa-org.netlify.app/guide/, https://github.com/vite-pwa/tanstack, https://unplugin-pwa.netlify.app/frameworks/tanstack
- https://github.com/TanStack/router/issues/4988
- https://developers.cloudflare.com/workers/static-assets/, https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/
- https://developers.cloudflare.com/workers/platform/limits/, https://developers.cloudflare.com/workers/platform/pricing/
- https://developers.cloudflare.com/r2/pricing/, https://developers.cloudflare.com/durable-objects/platform/pricing/
- https://caniuse.com/native-filesystem-api, https://developer.mozilla.org/en-US/docs/Web/API/File_System_API
- https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system
- https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria
- https://webkit.org/blog/10218/full-third-party-cookie-blocking-and-more/
- https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest/Reference/file_handlers (and launch_handler, Window.launchQueue)
- https://github.com/yjs/yjs, https://github.com/yjs/y-webrtc, https://liveblocks.io/pricing, https://www.partykit.io/
- https://support.google.com/youtube/answer/1722171, https://support.google.com/youtube/answer/10059070
- https://ads.tiktok.com/resources/help/article/tiktok-auction-in-feed-ads
- https://help.instagram.com/1038071743007909, https://help.instagram.com/270963803047681
- https://www.linkedin.com/help/linkedin/answer/a7486279, https://www.linkedin.com/help/linkedin/answer/a548372
- https://support.discord.com/hc/en-us/articles/115000435108, https://discord.com/nitro, https://support.discord.com/hc/en-us/articles/360028038352
- https://helpx.adobe.com/premiere/desktop/get-started/keyboard-shortcuts/default-keyboard-shortcuts.html
- https://support.apple.com/guide/final-cut-pro/keyboard-shortcuts-ver90ba5929/mac
- https://documents.blackmagicdesign.com/UserManuals/DaVinci-Resolve-15-Keyboard-Shortcuts.pdf
- https://capcutguide.com/capcut-keyboard-shortcuts/, https://fastshortcuts.com/shortcuts/capcut/ (third-party, INFERRED only)

Pages that could not be read (claims that depended on them are marked
UNKNOWN): pexels.com/license and terms, uppbeat.io terms, opengameart.org,
npmjs.com package pages, shadertoy.com/terms, the Adobe `.cube` specification
PDF, help.x.com video pages, TikTok organic upload limits, Instagram maximum
file size, docs.olivevideoeditor.org, remotion.dev/docs/license body,
coqui.ai/cpml, the Kokoro npm page, the Hugging Face whisper-webgpu Space,
Google and Azure speech pricing, the MediaPipe JS API reference and most
MediaPipe model cards, x.com posts cited by upstream maintainers.
