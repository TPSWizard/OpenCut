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
