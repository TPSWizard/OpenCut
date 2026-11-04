# ConzaCut feature research prompt

Paste everything below the line into a fresh agent session that has web access
and this repository checked out. It produces a ranked backlog, not code.

---

You are researching what to build next for **ConzaCut**, a video editor that is a
fork of OpenCut (MIT). You will read widely, then recommend a broad set of
features and additions with evidence, effort and licensing for each. You write
nothing but the report. You do not change code, install software, make paid
calls or deploy anything.

## What ConzaCut is today

- Repository: this checkout (fork of https://github.com/OpenCut-app/OpenCut).
  Read `README.md`, `ATTRIBUTION.md`, `crates/README.md` and
  `apps/desktop/README.md` first.
- `apps/web`: the browser editor, hosted at https://conzacut.conzahub.com on
  Cloudflare Workers. Media and projects stay in the visitor's browser
  (IndexedDB). It has a multi-track timeline, text, transforms, keyframes,
  masks, effects, stickers (Iconify), sounds (Freesound through `apps/api`),
  captions (depends on upstream's hosted transcription, which we do not run),
  a plugin SDK and an Editor API (`apps/web/src/api`).
- `apps/api`: a small Elysia worker for sound search and feedback. The
  Freesound key is not configured yet.
- `apps/desktop`: a native GPUI editor on Rust crates in `crates/` (document,
  commands, animation, masks, render, compositor). It opens the same project
  JSON and `.opencut.zip` bundles as the web app. It does not yet render
  stickers or captions.
- `apps/headless` and `apps/mcp`: Playwright-driven CLI/library and an MCP
  server over the Editor API.
- A sibling project, **Overtime AI Studio**
  (`C:\Users\Conza\Desktop\overtime-ai-studio`), has a planned "Shortform
  Studio" pipeline: narration-synced explainer videos built from an OpenCut
  project JSON timeline, rendered by a Rust CLI, with speech from ElevenLabs
  and images from Higgsfield's REST API. Read
  `prompts/13-shortform-studio-master.md`,
  `prompts/13-shortform-studio-followups.md` and
  `docs/SHORTFORM_STUDIO_BLUEPRINT.md` there. Part of your job is to find
  features that let ConzaCut and that studio share work.

## Hard constraints on what you may recommend

- No CapCut source, icons, fonts, LUTs, effect presets or other assets, and
  nothing scraped from any proprietary editor. Every asset source you
  recommend must have a licence you cite that allows redistribution in an MIT
  project or runtime fetching under its terms.
- Rendering cannot run on Cloudflare Workers. Anything server-side that
  encodes video needs a separate host or the user's machine, and object
  storage (R2) is a prerequisite for anything that keeps media server-side.
- Captions and AI features that need a model must say where the model runs
  (browser via WebGPU/WASM, the desktop app, a paid API, or a self-hosted
  service) and what it costs.
- Label every factual claim **CONFIRMED** (you read it at a URL you cite),
  **INFERRED** (you reasoned from cited evidence) or **UNKNOWN**. "I could
  not determine X" is a valid answer. Do not invent demand, licences or APIs.

## Where to dive

Read the threads, not just the titles. For each source, note what users ask
for most, what they complain about, and what maintainers have declined.

1. OpenCut upstream: open and closed issues, discussions, pull requests and
   the roadmap, plus `OpenCut-app/opencut-classic`. Count recurring requests.
2. Other open-source editors' trackers for the same requests and how they
   solved them: Kdenlive, Shotcut, Olive, Flowblade, Pitivi, LosslessCut,
   Motion Canvas, Remotion (licence check: Remotion is not free for companies),
   Omniclip, Clipchamp alternatives on GitHub.
3. Asset ecosystems and their licences: Iconify collections, Google Fonts and
   Fontsource, LottieFiles and the Lottie player, Pexels/Pixabay/Unsplash
   APIs, Freesound, Openverse, Mixkit, Uppbeat terms, CC0 LUT and SFX packs.
4. Captions and speech: whisper.cpp, Transformers.js and WebGPU Whisper
   demos, Deepgram/AssemblyAI pricing, ElevenLabs, Kokoro and other
   open TTS, word-timing accuracy reports.
5. AI features people actually use in editors: auto-cut silence, filler-word
   removal, scene detection, auto-reframe to 9:16, background removal,
   text-to-video shot lists, B-roll search, style-locked image sets. Find
   working open implementations (MediaPipe, ONNX Runtime Web, rembg, PySceneDetect, ffmpeg silencedetect) and note browser feasibility.
6. Effects and transitions: GLSL/WGSL shader collections with permissive
   licences (gl-transitions is MIT), colour grading approaches, LUT formats.
7. Distribution and growth: PWA install, project sharing, template galleries,
   export presets per platform, keyboard-shortcut parity with CapCut and
   Premiere as a usability signal (behaviour only, never their assets).

## What to deliver

A single Markdown report, `docs/FEATURE-RESEARCH.md`, with:

1. **Summary**: the ten highest-value additions in one paragraph each.
2. **Backlog table**, at least forty rows, one per feature, columns:
   feature; area (assets, sounds, effects, captions, AI, studio coupling,
   editor UX, desktop parity, platform); demand evidence (links and counts);
   where it runs (web, desktop, both, studio); effort (S/M/L/XL with a
   sentence on why); dependencies and prerequisites; licence or provenance of
   anything brought in; claim label.
3. **Studio coupling section**: which backlog items also serve the Shortform
   Studio pipeline, and the minimal shared interfaces (project JSON fields,
   render CLI flags, asset manifests) that make one implementation serve both.
4. **Declined and deferred**: things users ask for that you recommend against,
   with the reason (licence, platform limit, cost, scope).
5. **Open questions** for the owner, each with the decision it unblocks.
6. **Sources**: every URL you relied on, grouped by section.

Order the backlog by value divided by effort. Keep each cell short. Write in
plain prose, sentence case, no emoji.
