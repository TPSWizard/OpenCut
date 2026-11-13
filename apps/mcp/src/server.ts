import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { HeadlessRunner, parseHeadlessJob } from "@opencut/headless";
import { dirname, resolve } from "node:path";
import { z } from "zod";

/**
 * MCP server exposing OpenCut to AI agents. It drives the editor through the
 * headless runner (a browser running the web app), so an agent can open a
 * project, import media, lay out a timeline, run scripts and render files.
 *
 * Environment:
 *   OPENCUT_URL      origin of a running web app (default: start a local preview)
 *   OPENCUT_PROFILE  browser profile directory that holds the projects
 *   OPENCUT_HEADED   set to "1" to show the browser window
 */
export function createOpenCutMcpServer({
	runner = new HeadlessRunner({
		url: process.env.OPENCUT_URL,
		profileDir: process.env.OPENCUT_PROFILE,
		headed: process.env.OPENCUT_HEADED === "1",
		log: (line) => console.error(line),
	}),
}: { runner?: HeadlessRunner } = {}): { server: McpServer; runner: HeadlessRunner } {
	const server = new McpServer({
		name: "opencut",
		version: "0.1.0",
	});

	let openProjectId: string | null = null;
	const mediaIds = new Map<string, string>();

	const ensureProject = async () => {
		await runner.start();
		if (!openProjectId) {
			throw new Error("No project is open. Call opencut_open_project first.");
		}
	};

	const json = (value: unknown) => ({
		content: [{ type: "text" as const, text: JSON.stringify(value, null, 2) }],
	});

	server.registerTool(
		"opencut_list_projects",
		{
			title: "List OpenCut projects",
			description: "List the projects stored in this OpenCut profile (id, name, duration, last update).",
			inputSchema: {},
		},
		async () => {
			await runner.start();
			return json(await runner.listProjects());
		},
	);

	server.registerTool(
		"opencut_open_project",
		{
			title: "Open or create a project",
			description:
				"Open an existing project by id, or create a new one when no id is given. Subsequent tools act on this project. Returns the project info and its timeline.",
			inputSchema: {
				projectId: z.string().optional().describe("Existing project id"),
				name: z.string().optional().describe("Name for a new project"),
				fps: z.number().positive().optional(),
				canvas: z
					.object({ width: z.number().int().positive(), height: z.number().int().positive() })
					.optional(),
			},
		},
		async ({ projectId, name, fps, canvas }) => {
			await runner.start();
			openProjectId = await runner.openProject({ projectId, name });
			mediaIds.clear();
			if (fps !== undefined || canvas) {
				await runner.call("project.updateSettings", { fps, canvas });
			}
			const project = await runner.call("project.get");
			const timeline = await runner.getTimeline();
			return json({ project, timeline });
		},
	);

	server.registerTool(
		"opencut_import_media",
		{
			title: "Import media files",
			description:
				"Import video, audio or image files from disk (absolute paths) or http(s) URLs into the open project. Returns alias → media info.",
			inputSchema: {
				files: z
					.array(
						z.object({
							alias: z.string().describe("Short name to refer to this media in later calls"),
							path: z.string().describe("Absolute file path or URL"),
						}),
					)
					.min(1),
			},
		},
		async ({ files }) => {
			await ensureProject();
			const media = Object.fromEntries(files.map((file) => [file.alias, file.path]));
			const ids = await runner.importMedia({ media, baseDir: process.cwd() });
			for (const [alias, id] of Object.entries(ids)) mediaIds.set(alias, id);
			const assets = await runner.call<Array<{ id: string }>>("media.list");
			return json(
				Object.fromEntries(
					Object.entries(ids).map(([alias, id]) => [alias, assets.find((a) => a.id === id) ?? { id }]),
				),
			);
		},
	);

	server.registerTool(
		"opencut_add_clip",
		{
			title: "Add a clip to the timeline",
			description:
				"Place imported media on the timeline. `media` is an alias from opencut_import_media or a media id. Times are seconds.",
			inputSchema: {
				media: z.string(),
				at: z.number().min(0).optional().describe("Start time in seconds (default: playhead)"),
				duration: z.number().positive().optional().describe("Seconds (default: full media length)"),
				trackType: z.enum(["video", "audio"]).optional(),
				trackId: z.string().optional(),
				name: z.string().optional(),
			},
		},
		async ({ media, ...rest }) => {
			await ensureProject();
			const mediaId = mediaIds.get(media) ?? media;
			return json(await runner.call("timeline.addClip", { mediaId, ...rest }));
		},
	);

	server.registerTool(
		"opencut_add_text",
		{
			title: "Add a text element",
			description: "Add a text/title element. `params` accepts text params such as fontSize, fontFamily, color, transform.positionX/Y, opacity.",
			inputSchema: {
				text: z.string(),
				at: z.number().min(0).optional(),
				duration: z.number().positive().optional(),
				name: z.string().optional(),
				params: z.record(z.string(), z.union([z.number(), z.string(), z.boolean()])).optional(),
			},
		},
		async (params) => {
			await ensureProject();
			return json(await runner.call("timeline.addText", params));
		},
	);

	server.registerTool(
		"opencut_get_timeline",
		{
			title: "Get the timeline",
			description: "Return every track and element of the open project's active scene, with times in seconds.",
			inputSchema: {},
		},
		async () => {
			await ensureProject();
			return json(await runner.getTimeline());
		},
	);

	server.registerTool(
		"opencut_call",
		{
			title: "Call any Editor API method",
			description:
				"Low-level access to the OpenCut Editor API (36+ methods such as timeline.updateElement, timeline.split, timeline.delete, playback.seek, history.undo, project.updateSettings). Read the opencut://api/methods resource for the list.",
			inputSchema: {
				method: z.string().describe("Dotted method name, e.g. timeline.get"),
				params: z.record(z.string(), z.unknown()).optional(),
			},
		},
		async ({ method, params }) => {
			await runner.start();
			if (method !== "project.list" && method !== "project.create" && method !== "app.info") {
				await ensureProject();
			}
			return json(await runner.call(method, params ?? {}));
		},
	);

	server.registerTool(
		"opencut_run_script",
		{
			title: "Run a script in the editor",
			description:
				"Run JavaScript inside the editor with `opencut` bound to the Editor API (same as the editor's Scripting tab). The code is the body of an async function; `return` a value to get it back.",
			inputSchema: { code: z.string() },
		},
		async ({ code }) => {
			await ensureProject();
			const result = await runner.runScript({ code });
			return json({ result: result ?? null });
		},
	);

	server.registerTool(
		"opencut_export",
		{
			title: "Export the project",
			description: "Render the open project to an MP4 or WebM file at the given absolute output path.",
			inputSchema: {
				output: z.string().describe("Absolute output path, e.g. C:/renders/out.mp4"),
				format: z.enum(["mp4", "webm"]).optional(),
				quality: z.enum(["low", "medium", "high", "very_high"]).optional(),
				includeAudio: z.boolean().optional(),
				fps: z.number().positive().optional(),
				preset: z
					.string()
					.optional()
					.describe(
						"Platform preset id (youtube, youtube-shorts, tiktok, instagram-reels, linkedin, x): MP4 with the platform's H.264 settings; run opencut_call export.check first",
					),
			},
		},
		async (params) => {
			await ensureProject();
			await runner.call("project.save");
			const written = await runner.exportTo(params);
			return json({ output: written });
		},
	);

	server.registerTool(
		"opencut_render_job",
		{
			title: "Render a job in one call",
			description:
				"Create a project, import media, build the timeline, optionally run a script and export, all from one job object (same format as apps/headless job files). Paths resolve against `baseDir`.",
			inputSchema: {
				job: z.record(z.string(), z.unknown()),
				baseDir: z.string().optional().describe("Directory relative paths resolve against (default: cwd)"),
			},
		},
		async ({ job, baseDir }) => {
			await runner.start();
			const parsed = parseHeadlessJob(job);
			const base = baseDir ? resolve(baseDir) : process.cwd();
			const output = await runner.runJob({ job: parsed, baseDir: base });
			openProjectId = await runner.call<{ projectId: string | null }>("app.info").then(
				(info) => info.projectId,
			);
			return json({ output, projectId: openProjectId, outputDir: output ? dirname(output) : null });
		},
	);

	server.registerResource(
		"editor-api-methods",
		"opencut://api/methods",
		{
			title: "OpenCut Editor API methods",
			description: "Dotted method names accepted by opencut_call, with a one-line summary each.",
			mimeType: "text/markdown",
		},
		async (uri) => ({
			contents: [{ uri: uri.href, mimeType: "text/markdown", text: EDITOR_API_SUMMARY }],
		}),
	);

	return { server, runner };
}

export const EDITOR_API_SUMMARY = `# OpenCut Editor API v1 — methods

Times are seconds. Elements are addressed by { elementId, trackId? }.

- app.info → { apiVersion, ready, projectId }
- project.get / project.create { name } / project.list / project.rename { name }
- project.updateSettings { fps?, canvas?: {width,height}, background? } / project.save
- scenes.list / scenes.switch { sceneId }
- media.list / media.importUrl { url, name? } / media.remove { mediaId }
- media.makeProxy { mediaId } → ApiMediaInfo (960 px preview proxy; export reads the original) / media.removeProxy { mediaId }
- media.missing → [{ id, name, type, size }] (files gone from browser storage) / media.relink { mediaId, file } → ApiMediaInfo (in page only)
- timeline.setTrackLocked { trackId, locked } → ApiTrackInfo / selection.selectRange { from?, to? } → ElementRef[] (clips on unlocked tracks overlapping the range; default: in/out marks)
- timeline.group { elements } / timeline.ungroup { elements } → ApiElementInfo[] (groupId; grouped clips select and move together) / timeline.createCompound { elements } → ApiElementInfo (compound: { clips, tracks }) / timeline.breakApart { elementId } → ApiElementInfo[]
- stickers.search { query, pack?: all|emoji|brands|icons|flags, limit? } → [{ id, name, pack, license?, requiresCredit?, credit? }] / timeline.addSticker { stickerId, at?, duration?, name? } → ApiElementInfo (brand logos are trademarks)
- markers.list / markers.add { time?, note?, color?, duration? } / markers.update { time, note?, color?, duration? } / markers.remove { time } → ApiMarker[] (seconds) / markers.chapters → { text }
- storage.status / storage.persist → { level, persisted, browser, standalone, sevenDayRule, reasons, advice, usageBytes, quotaBytes }
- storage.usage → [{ projectId, name, mediaBytes, proxyBytes, mediaCount, missing, lastBackupAt, needsBackup }] / storage.clearProxies { projectId? } → { freedBytes }
- preview.getSettings / preview.setSettings { quality?: auto|full|half|quarter, useProxies?, autoProxies? } / preview.capture { time? } → { width, height, centre, decoders }
- timeline.get → { sceneId, duration, tracks[{ id, name, type, lane, muted?, hidden?, elements[] }] }
- timeline.addClip { mediaId, at?, duration?, trackId?, trackType?, name? }
- timeline.addText { text, at?, duration?, name?, params?, trackId? }
- timeline.updateElement { elementId, trackId?, patch: { name?, startTime?, duration?, trimStart?, trimEnd?, hidden?, params? } }
- timeline.moveElement { elementId, trackId?, startTime }
- timeline.setTransition { elementId, trackId?, transition: { type, duration?, params? } | null } (a gl-transitions name; plays centred on the cut to the next clip)
- transitions.list → [{ name, description, tags, params[{ name, kind, default, min, max }] }]
- captions.insert { captions[{ text, startTime, duration, words?[{ text, start, end }], style? }], style?: { highlight?: none|word|box|fill, activeColor?, boxColor? } } → { trackId, elements[] } (timed words make the spoken word highlight)
- captions.export { format: "srt" | "vtt" | "ass", trackId? } → { format, fileName, cueCount, content }
- captions.transcribe { language?, modelId?, wordsPerCaption?, insert?, style? } → { text, words[], captions[], trackId } (Whisper in the page; the model downloads on first use)
- timeline.split { elements, time } / timeline.delete { elements } / timeline.duplicate { elements }
- timeline.addTrack { type, index? } / timeline.removeTrack { trackId }
- timeline.setTrackMuted { trackId, muted } / timeline.setTrackHidden { trackId, hidden }
- playback.getState / playback.play / playback.pause / playback.seek { time }
- selection.get / selection.set { elements } / selection.clear
- history.undo / history.redo / history.state
- export.render { format?, quality?, includeAudio?, fps?, fileName?, preset? } → bytes (in-page only)
- export.download { same } → triggers a file download (used by the headless runner)
- fonts.list { query?, limit? } → [{ id, family, category, license, weights, styles, variable }] (Fontsource, open licences)
- fonts.load { family } → { family, license, faces }
- timeline.getKeyframes { elementId, trackId? } → { [path]: [{ id, time, value, interpolation? }] }
- timeline.setKeyframe { elementId, trackId?, path, time, value, interpolation? } → ApiElementInfo
- timeline.removeKeyframe { elementId, trackId?, path, time? | keyframeId? } / timeline.clearKeyframes { elementId, trackId?, path } → ApiElementInfo
- masks.list {} → [{ type, name, params }]
- timeline.setMask { elementId, trackId?, type, params? } / timeline.updateMask { elementId, trackId?, params?, inverted? } / timeline.removeMask { elementId, trackId? } → ApiElementInfo (masks)
- timeline.setRetime { elementId, trackId?, rate?, maintainPitch?, reverse?, ramp?: [{ time, rate }], clear? } → ApiElementInfo (speed, ramp or reverse; duration follows)
- luts.parse { file } → { name, size, lut } (then timeline.addEffect { type: "lut", params: { lut } })
- effects.list {} → [{ type, name, keywords, params: [{ key, label, type, default, min?, max?, step?, options? }] }]
- timeline.addEffect { elementId, trackId?, type, params? } → ApiElementInfo (effects: [{ id, type, params, enabled }])
- timeline.updateEffect { elementId, trackId?, effectId, params?, enabled? } → ApiElementInfo
- timeline.removeEffect { elementId, trackId?, effectId } → ApiElementInfo
- shots.detect { elementId, trackId?, threshold?, minGapSeconds?, sampleFps?, apply?: "none" | "bookmarks" | "split" } → { cuts[], framesScored, seconds, applied } (scene changes in a video clip)
- reframe.apply { elementId, trackId?, camera?: "steady" | "smooth" | "follow", resetAtShots?, faces?, sampleFps? } → { scale, positionX, positionY, keysX[], keysY[], shots, cuts, samples, faceSamples, fallbackShots, keyframes } (covers the canvas and pans to keep faces in view; change the canvas to 9:16 first) / reframe.clear { elementId, trackId? } → ApiElementInfo
- picture.cleanup { elementId, trackId?, removeWhite?, decontaminate?, feather?, tint?, crop?, trace? } → { mediaId, width, height, path } (drawing on white → clean cut-out PNG + traced path) / picture.restore { elementId } → ApiElementInfo
- audio.textToSpeech { script, voice?, speed?, captions?, at? } → { mediaId, elementId, captionTrackId, lines, seconds } (Kokoro voice-over on the device, captions per line)
- captions.translate { trackId?, from, to, engine?: "auto" | "browser" | "opus" } → { trackId, captions, engine, models, translations } (translated copy of a caption track on a new track above it)
- cutout.removeBackground { elementId, trackId?, sampleFps? } → { matteId, matteName, frames, seconds, keptShare } (person cut-out from an on-device matte) / cutout.setInverted { elementId, inverted } / cutout.clear { elementId } → ApiElementInfo
- audio.removeFillers { elementId, trackId?, language?, source?, ripple?, apply?, modelId? } → { fillers[], intervals[], seconds, applied, pieces[] } (cuts "um"/"uh" from caption word timings or a transcription)
- audio.reduceNoise { elementId, trackId?, reductionDb? } → { assetId, assetName, elementId, trackId, seconds, channels } (denoised copy the clip plays)
- audio.detectBeats { elementId, trackId?, every?, apply?: "none" | "bookmarks" } → { bpm, confidence, beats[], applied } (beat bookmarks snap)
- audio.measureLoudness { elementId, trackId? } → { integratedLufs, loudnessRangeLu, truePeakDbtp, maxMomentaryLufs, maxShortTermLufs, seconds } (EBU R128)
- audio.normalizeLoudness { elementId, trackId?, targetLufs?, truePeakDbtp?, target? } → { element, report, gainDb, volumeDb, peakLimited } / audio.loudnessTargets {} → presets
- audio.duck { element, voice?, amountDb?, thresholdDb?, attackSeconds?, releaseSeconds?, holdSeconds? } → { element, keyframes, intervals, speechSeconds, voiceElementIds } (music dips under speech; replaces the clip's volume keys)
- audio.clearDucking { element } → ApiElementInfo
- audio.detectSilence { element, thresholdDb?, minSilenceSeconds?, paddingSeconds? } → { intervals, removedSeconds, clipStart, clipDuration }
- audio.cutSilence { element, ripple? ("all" | "track" | "none"), thresholdDb?, minSilenceSeconds?, paddingSeconds? } → { intervals, removedSeconds, clipStart, clipDuration, pieces } (one undo step; closes the gaps)
- export.presets {} → [{ id, label, canvas, fps, maxDurationSeconds, maxFileBytes, safeArea, verified, source, encode }]
- export.check { preset, includeAudio? } → { preset, issues: [{ kind, severity, message, fix? }], estimatedBytes }

Element params of interest: transform.positionX, transform.positionY (pixels from centre),
transform.scaleX, transform.scaleY, transform.rotate (degrees), opacity (0..1);
text: content, fontSize, fontFamily, color, textAlign, letterSpacing, lineHeight;
captions (text elements with words): caption.highlight, caption.activeColor, caption.boxColor.
`;
