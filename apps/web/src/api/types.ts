/**
 * OpenCut Editor API, version 1.
 *
 * Everything here is JSON-serialisable so the same surface can be driven from
 * the in-editor scripting tab (a Worker), plugins (sandboxed iframes), the
 * headless runner (Playwright) and the MCP server. Times are seconds at this
 * boundary; the editor converts them to its integer tick representation.
 */

import type { AssetManifest } from "@/project/assets";
import type { TemplateInfo } from "@/templates/templates";

export const EDITOR_API_VERSION = "1.0.0";

export type ApiTrackType = "video" | "text" | "audio" | "graphic" | "effect";

export type ApiElementType =
	| "video"
	| "image"
	| "audio"
	| "text"
	| "sticker"
	| "graphic"
	| "effect"
	| "compound";

export type ApiParamValue = number | string | boolean;
export type ApiParamValues = Record<string, ApiParamValue>;

export interface ApiElementRef {
	trackId: string;
	elementId: string;
}

/** An element reference where the track may be omitted and looked up. */
export interface ApiElementLocator {
	elementId: string;
	trackId?: string;
}

export interface ApiProjectInfo {
	id: string;
	name: string;
	fps: number;
	canvas: { width: number; height: number };
	background:
		| { type: "color"; color: string }
		| { type: "blur"; blurIntensity: number };
	/** Total timeline duration of the active scene, in seconds. */
	duration: number;
	currentSceneId: string;
	createdAt: string;
	updatedAt: string;
}

export interface ApiProjectSummary {
	id: string;
	name: string;
	duration: number;
	createdAt: string;
	updatedAt: string;
}

export interface ApiSceneInfo {
	id: string;
	name: string;
	isMain: boolean;
	isActive: boolean;
}

export interface ApiMediaInfo {
	id: string;
	name: string;
	type: "image" | "video" | "audio" | "lottie";
	duration?: number;
	width?: number;
	height?: number;
	fps?: number;
	hasAudio?: boolean;
	/** The proxy the preview decodes instead of the original, when there is one. */
	proxy?: { width: number; height: number };
	/** True while a proxy is being made. */
	proxyPending?: boolean;
}

export type ApiPreviewQuality = "auto" | "full" | "half" | "quarter";

export interface ApiMarker {
	/** Seconds from the start of the timeline. */
	time: number;
	note?: string;
	color?: string;
	/** Seconds the marker spans, when it covers a stretch. */
	duration?: number;
}

export interface ApiStorageStatus {
	/** protected: the browser keeps the data; at-risk: it may delete it (low disk, or Safari's 7-day rule); unknown: it does not say. */
	level: "protected" | "at-risk" | "unknown";
	persisted: boolean | null;
	browser: "safari" | "firefox" | "chromium" | "other";
	standalone: boolean;
	sevenDayRule: boolean;
	reasons: string[];
	advice: string[];
	usageBytes: number | null;
	quotaBytes: number | null;
}

export interface ApiProjectStorage {
	projectId: string;
	name: string;
	mediaBytes: number;
	proxyBytes: number;
	mediaCount: number;
	/** Media ids whose file is gone from the browser's storage. */
	missing: string[];
	lastBackupAt: string | null;
	/** Changed since the last bundle export, or never exported. */
	needsBackup: boolean;
}

export interface ApiMissingMedia {
	id: string;
	name: string;
	type: "image" | "video" | "audio" | "lottie";
	size: number;
}

export interface ApiPreviewSettings {
	/** How finely the preview decodes video: auto fits the canvas, half and quarter go lower, full decodes every pixel. */
	quality: ApiPreviewQuality;
	useProxies: boolean;
	autoProxies: boolean;
	/** The video scope shown beside the preview, or null when hidden. */
	scope: ApiVideoScope | null;
}

export type ApiVideoScope = "waveform" | "parade" | "vectorscope" | "histogram";

export interface ApiScopeReading {
	scope: ApiVideoScope;
	/** Size of the drawn scope picture. */
	width: number;
	height: number;
	/** Pixels sampled from the frame. */
	samples: number;
	/** Share (0 to 1) of samples with a channel at 0. */
	atBlack: number;
	/** Share of samples with a channel at 255. */
	atWhite: number;
	/** Luma (0 to 255) the darkest 1% of samples sit at or below. */
	blackLevel: number;
	/** Luma the brightest 1% sit at or above. */
	whiteLevel: number;
}

export interface ApiRetime {
	rate: number;
	maintainPitch?: boolean;
	reverse?: boolean;
	ramp?: { time: number; rate: number }[];
}

export interface ApiElementInfo {
	id: string;
	trackId: string;
	type: ApiElementType;
	name: string;
	/** Clips sharing a group id are selected and moved together. */
	groupId?: string;
	/** For compound clips: how many clips on how many tracks it holds. */
	compound?: { clips: number; tracks: number };
	startTime: number;
	duration: number;
	/** Speed change of a video or audio clip, when it has one. */
	retime?: ApiRetime;
	trimStart: number;
	trimEnd: number;
	mediaId?: string;
	hidden?: boolean;
	params: ApiParamValues;
	/** The transition the clip leads out of (visual elements only). */
	transition?: ApiTransition;
	/** The transition it plays in with from nothing (none when a clip sits directly before it). */
	transitionIn?: ApiTransition;
	/** A caption's words, seconds from the element's start (text elements). */
	words?: ApiCaptionWord[];
	/** Effects on a visual element, in the order they apply. */
	effects?: ApiEffectInfo[];
	/** The mask on a video or image element, when it has one. */
	masks?: ApiMaskInfo[];
}

export type ApiTransitionParamValue = number | boolean | number[];

/** A clip's transition: a catalogue name, a duration in seconds, parameters. */
/** A stock search result (see `stock.search`). */
export interface ApiStockItem {
	provider: "openverse" | "pixabay";
	id: string;
	kind: "image" | "video";
	title: string;
	thumbnail: string;
	width: number;
	height: number;
	duration?: number;
	license: string;
	licenseUrl: string;
	creator: string;
	creatorUrl?: string;
	sourceUrl: string;
	attribution: string;
	fileUrl?: string;
}

export interface ApiTransition {
	type: string;
	duration: number;
	params?: Record<string, ApiTransitionParamValue>;
}

/** A spoken word; `start` and `end` are seconds on the timeline. */
export interface ApiCaptionWord {
	text: string;
	start: number;
	end: number;
	/** Set on caption keywords (see `captions.pickKeywords`); left out otherwise. */
	keyword?: boolean;
}

/** How a caption's spoken word stands out; see the `caption.*` text params. */
export interface ApiCaptionStyle {
	highlight?: "none" | "word" | "box" | "fill";
	activeColor?: string;
	boxColor?: string;
	/** Colour of the keywords (words flagged by `captions.pickKeywords` or `captions.setKeywords`). */
	keywordColor?: string;
	/** Keyword size relative to the other words, 1 to 2 (default 1.25). */
	keywordScale?: number;
}

/**
 * One caption to insert: seconds on the timeline, optional timed words
 * (which must spell `text` when joined by spaces, in order) and optional
 * text styling (font, colour, placement) as the subtitle importer takes it.
 */
export interface ApiCaption {
	text: string;
	startTime: number;
	duration: number;
	words?: ApiCaptionWord[];
	style?: Record<string, unknown>;
}

export interface ApiTransitionParamInfo {
	name: string;
	kind: string;
	default: ApiTransitionParamValue;
	min?: number | null;
	max?: number | null;
	step?: number | null;
	color: boolean;
	description?: string | null;
}

export interface ApiTransitionInfo {
	name: string;
	author: string;
	license: string;
	description?: string | null;
	tags: string[];
	params: ApiTransitionParamInfo[];
}

export interface ApiTrackInfo {
	id: string;
	name: string;
	type: ApiTrackType;
	/** "main" is the primary video track; overlays sit above it, audio below. */
	lane: "overlay" | "main" | "audio";
	muted?: boolean;
	hidden?: boolean;
	/** Locked: its clips cannot be selected or edited and ripple edits leave it alone. */
	locked?: boolean;
	/** Track mixer (audio and video tracks): solo, volume in dB, pan. */
	solo?: boolean;
	volume?: number;
	pan?: number;
	elements: ApiElementInfo[];
}

export interface ApiTimelineInfo {
	sceneId: string;
	duration: number;
	tracks: ApiTrackInfo[];
}

export interface ApiPlaybackState {
	isPlaying: boolean;
	time: number;
	duration: number;
	volume: number;
	muted: boolean;
}

export type ApiExportFormat = "mp4" | "webm";
export type ApiExportQuality = "low" | "medium" | "high" | "very_high";
/** A B-roll shot: the script line, its search query, and when it plays (seconds from the B-roll's start). */
export interface ApiShot {
	text: string;
	query: string;
	start: number;
	duration: number;
}

export type ApiVideoCodec = "h264" | "hevc" | "av1" | "vp9";

export interface ApiExportCodecs {
	format: ApiExportFormat;
	/** The container's default codec. */
	default: ApiVideoCodec;
	/** Every codec the container takes, and whether this browser can encode it at the canvas size. */
	codecs: Array<{ id: ApiVideoCodec; label: string; encodable: boolean }>;
}

export interface ApiExportOptions {
	/**
	 * What to write (row 96): `video` (default), `gif` (an animated GIF
	 * through the shared `gifenc` crate, the same bytes as the desktop;
	 * see `gif`), `audio` (the sound alone as `audioFormat`, with
	 * `loudness`), `still` (the frame at the playhead as a PNG) or `frames`
	 * (every frame at the project's rate as numbered PNGs in a zip).
	 */
	kind?: "video" | "gif" | "audio" | "still" | "frames";
	/** GIF width in pixels (default 480; the height keeps the canvas shape), frame rate (default 15, at most 50) and dithering (default on). */
	gif?: { width?: number; fps?: number; dither?: boolean };
	/** Audio-only format: `wav` (16-bit), `aac` (.m4a, default) or `opus` (.opus in Ogg). */
	audioFormat?: "wav" | "aac" | "opus";
	/**
	 * Leave the background out and keep the alpha (row 97): a video is then
	 * VP9 with alpha in WebM (`format: "mp4"`, a `platform` or another
	 * `codec` is refused), a still or `frames` PNGs with alpha.
	 */
	transparent?: boolean;
	format?: ApiExportFormat;
	quality?: ApiExportQuality;
	includeAudio?: boolean;
	fps?: number;
	/** File name without extension; defaults to the project name. */
	fileName?: string;
	/**
	 * A platform preset id from `export.presets`. Forces MP4 with H.264
	 * High profile at the platform bitrate, a half-second closed GOP, the
	 * moov atom first and 384 kbps audio; `quality` is then ignored. The
	 * canvas and frame rate are not changed: run `export.check` first.
	 */
	preset?: string;
	/**
	 * Video codec. MP4 takes h264 (default), hevc and av1; WebM takes vp9
	 * (default) and av1. Browsers differ in what they can encode: ask
	 * `export.codecs` first. Ignored with `preset`.
	 */
	codec?: ApiVideoCodec;
	/** Target video bitrate in bits per second (0.2 to 400 Mbps); replaces `quality` for the picture. */
	videoBitrate?: number;
	/** Prefer a hardware encoder (faster; quality depends on the GPU). */
	hardware?: boolean;
	/**
	 * Bring the whole mix to a loudness: `streaming` (-14 LUFS), `podcast`
	 * (-16), `broadcast` (-23), `loud` (-11), or a number of LUFS (-40 to
	 * -5); true peaks are held under -1 dBTP by a limiter. The shared Rust
	 * `loudness` crate, as the desktop export.
	 */
	loudness?: "streaming" | "podcast" | "broadcast" | "loud" | number;
}

export interface ApiDuckOptions {
	/** The music clip (audio or video element). */
	element: ApiElementRef;
	/** Clips carrying the voice; every other audible clip when omitted. */
	voice?: ApiElementRef[];
	/** How far the music drops while speech plays, in dB. Default -12. */
	amountDb?: number;
	/** RMS level in dBFS above which a window is speech. Default -40. */
	thresholdDb?: number;
	/** Default 0.1. */
	attackSeconds?: number;
	/** Default 0.5. */
	releaseSeconds?: number;
	/** Gaps in speech shorter than this stay ducked. Default 0.3. */
	holdSeconds?: number;
}

export interface ApiDuckResult {
	element: ApiElementInfo;
	/** Volume keys written. */
	keyframes: number;
	/** Where speech was found, in timeline seconds. */
	intervals: { start: number; end: number }[];
	speechSeconds: number;
	/** Elements the voice was measured from. */
	voiceElementIds: string[];
}

export interface ApiFontInfo {
	id: string;
	family: string;
	category: string;
	/** SPDX licence id, such as "OFL-1.1" or "Apache-2.0". */
	license: string;
	weights: number[];
	styles: string[];
	variable: boolean;
}

export interface ApiMaskInfo {
	id: string;
	type: string;
	params: Record<string, unknown>;
}

export interface ApiMaskParamInfo {
	key: string;
	label: string;
	type: string;
	default: unknown;
	min?: number;
	max?: number;
	step?: number;
	options?: { value: string; label: string }[];
}

export interface ApiMaskDefinitionInfo {
	type: string;
	name: string;
	params: ApiMaskParamInfo[];
}

export interface ApiKeyframeInfo {
	id: string;
	/** Seconds from the element's start. */
	time: number;
	value: number | string | boolean;
	/** How the value reaches the next key: linear, hold or bezier (number channels only). */
	interpolation?: "linear" | "hold" | "bezier";
}

export interface ApiLoudnessReport {
	/** `null` when nothing passed the gate (silence). */
	integratedLufs: number | null;
	loudnessRangeLu: number;
	truePeakDbtp: number | null;
	maxMomentaryLufs: number | null;
	maxShortTermLufs: number | null;
	seconds: number;
	sampleRate: number;
	channels: number;
}

export interface ApiEffectInfo {
	id: string;
	type: string;
	params: ApiParamValues;
	enabled: boolean;
}

export interface ApiEffectParamInfo {
	key: string;
	label: string;
	/** `color` takes a hex string such as "#00b140". */
	type: "number" | "select" | "color";
	default: number | string;
	min?: number;
	max?: number;
	step?: number;
	options?: { value: string; label: string }[];
}

export interface ApiEffectDefinitionInfo {
	type: string;
	name: string;
	keywords: string[];
	params: ApiEffectParamInfo[];
}

export interface ApiSilenceOptions {
	/** The clip (audio or video element). */
	element: ApiElementRef;
	/** RMS level in dBFS under which a window is silent. Default -40. */
	thresholdDb?: number;
	/** Silence shorter than this stays. Default 0.5. */
	minSilenceSeconds?: number;
	/** Silence kept on each side of the speech around a cut. Default 0.1. */
	paddingSeconds?: number;
}

export interface ApiSilenceReport {
	/** Stretches to remove, in timeline seconds. */
	intervals: { start: number; end: number }[];
	removedSeconds: number;
	clipStart: number;
	clipDuration: number;
}

export interface ApiSilenceCutResult extends ApiSilenceReport {
	/** The pieces of the clip left on the timeline. */
	pieces: ApiElementRef[];
}

export interface ApiSafeArea {
	top: number;
	bottom: number;
	left: number;
	right: number;
}

export interface ApiPlatformPreset {
	id: string;
	label: string;
	canvas: { width: number; height: number };
	fps: number;
	maxDurationSeconds: number | null;
	maxFileBytes: number | null;
	/** Fractions of the frame the platform's controls cover, or null. */
	safeArea: ApiSafeArea | null;
	verified: "official" | "third-party";
	source: string;
	/** Encoder settings for the open project's canvas and frame rate. */
	encode: {
		videoBitrate: number;
		audioBitrate: number;
		keyFrameIntervalSeconds: number;
		codec: string;
	};
}

export interface ApiPlatformIssue {
	kind: "canvas" | "fps" | "duration" | "size";
	severity: "warning" | "error";
	message: string;
	fix?: { canvas?: { width: number; height: number }; fps?: number };
}

export interface ApiPlatformCheck {
	preset: string;
	/** Empty when the project matches the platform. */
	issues: ApiPlatformIssue[];
	estimatedBytes: number;
}

export interface ApiExportResult {
	fileName: string;
	mimeType: string;
	/** Encoded media. Transferred, not copied, across the message bridge. */
	bytes: ArrayBuffer;
	/** What whole-mix loudness did (with `loudness`); levels are `null` for silence. */
	mastered?: {
		inputLufs: number | null;
		inputTruePeakDbtp: number | null;
		gainDb: number;
		outputLufs: number | null;
		outputTruePeakDbtp: number | null;
		limited: boolean;
		passes: number;
	};
}

export interface ApiElementPatch {
	name?: string;
	startTime?: number;
	duration?: number;
	trimStart?: number;
	trimEnd?: number;
	hidden?: boolean;
	params?: ApiParamValues;
}

/**
 * The complete method map. Keys are dotted method names, values describe the
 * single params object and the result. `dispatch(method, params)` on the
 * bridge is typed against this.
 */
export interface EditorApiMethods {
	"app.info": {
		params: Record<string, never>;
		result: { apiVersion: string; ready: boolean; projectId: string | null };
	};

	"project.get": { params: Record<string, never>; result: ApiProjectInfo };
	/** Create a project in browser storage; open it by navigating to `/editor/<id>`. */
	"project.create": { params: { name?: string }; result: ApiProjectSummary };
	"project.list": {
		params: Record<string, never>;
		result: ApiProjectSummary[];
	};
	"project.rename": { params: { name: string }; result: ApiProjectInfo };
	"project.updateSettings": {
		params: {
			fps?: number;
			canvas?: { width: number; height: number };
			background?: ApiProjectInfo["background"];
		};
		result: ApiProjectInfo;
	};
	"project.save": { params: Record<string, never>; result: { saved: true } };
	/**
	 * Download the open project and its media as a project bundle (a zip the
	 * desktop editor reads and writes too). `fileName` is without the
	 * `.opencut.zip` extension, which is added.
	 */
	"project.exportBundle": {
		params: { fileName?: string };
		result: { fileName: string; bytes: number };
	};
	/** Create a project from a bundle; open it by navigating to `/editor/<id>`. */
	"project.importBundle": { params: { file: File }; result: ApiProjectSummary };
	/**
	 * Downloads the bundle an "Open in ConzaCut" link (or a bundle URL)
	 * points at and adds it as a project. https only (http on localhost);
	 * the bundle's server must allow this app to download it (CORS).
	 */
	/**
	 * Uploads the open project and its media for 7 days and returns the
	 * Open in ConzaCut link to give out. Needs the server's sharing passcode
	 * (the one remembered in this browser when left out).
	 */
	"project.share": { params: { passcode?: string }; result: { link: string; bundleUrl: string; bytes: number; expiresAt: string } };
	"project.openLink": { params: { link: string }; result: ApiProjectSummary & { bundleUrl: string } };
	/** Starter templates (ConzaCut's own) and the ones saved in this browser. */
	"templates.list": { params: Record<string, never>; result: { templates: TemplateInfo[] } };
	/** Makes a new project from a template; open it by navigating to `/editor/<projectId>`. */
	"templates.use": { params: { id: string; name?: string }; result: { projectId: string } };
	/** Saves the open project (with its media) as a template in this browser. */
	"templates.save": { params: { name: string; description?: string }; result: TemplateInfo };

	"scenes.list": { params: Record<string, never>; result: ApiSceneInfo[] };
	"scenes.switch": { params: { sceneId: string }; result: ApiSceneInfo[] };

	"media.list": { params: Record<string, never>; result: ApiMediaInfo[] };
	"media.importUrl": {
		params: { url: string; name?: string };
		result: ApiMediaInfo;
	};
	"media.importFiles": { params: { files: File[] }; result: ApiMediaInfo[] };
	"media.remove": { params: { mediaId: string }; result: { removed: true } };
	/**
	 * Makes a small proxy of a video (longer side 960 px, H.264 or VP9, no
	 * audio) that the preview decodes instead of the original; export always
	 * reads the original. Resolves when the proxy is stored.
	 */
	"media.makeProxy": { params: { mediaId: string }; result: ApiMediaInfo };
	"media.removeProxy": { params: { mediaId: string }; result: ApiMediaInfo };
	/** How safe the browser's copy of the projects is, and how much it uses. */
	"storage.status": { params: Record<string, never>; result: ApiStorageStatus };
	/** Asks the browser to keep this site's storage (`navigator.storage.persist()`). */
	"storage.persist": { params: Record<string, never>; result: ApiStorageStatus };
	/** Per project: bytes of media and proxies, missing files and backup state. */
	"storage.usage": { params: Record<string, never>; result: ApiProjectStorage[] };
	/** Deletes a project's proxies (made again on request); default: the open project. */
	"storage.clearProxies": { params: { projectId?: string }; result: { freedBytes: number } };
	/** The open project's media whose file is gone from the browser's storage. */
	"media.missing": { params: Record<string, never>; result: ApiMissingMedia[] };
	/** Gives a media item (missing or not) a new file under the same id; its clips play the new file. */
	"media.relink": { params: { mediaId: string; file: File }; result: ApiMediaInfo };
	/** Preview quality and proxy settings (kept per browser). */
	"preview.getSettings": { params: Record<string, never>; result: ApiPreviewSettings };
	"preview.setSettings": { params: Partial<ApiPreviewSettings>; result: ApiPreviewSettings };
	/**
	 * Renders the preview's own scene (proxies and quality applied) at
	 * `time` seconds (default: the playhead) and reports which decoders it
	 * used; `sample` returns the average colour of a centred box.
	 */
	/**
	 * Renders the preview's scene at `time` seconds (default: the playhead)
	 * and draws a video scope of it (shared with the desktop): the luma
	 * levels and clipped share it read. `show` also opens it beside the
	 * preview.
	 */
	"preview.scope": { params: { scope: ApiVideoScope; time?: number; show?: boolean }; result: ApiScopeReading };
	/**
	 * Freehand drawing (row 90): adds a stroke (`points` as [x, y, pressure,
	 * time]: canvas units 0 to 1, pressure 0 to 1, seconds since the stroke
	 * began) to the selected drawing showing at the playhead, or to a new
	 * drawing there, which is then selected. `tool` pen, marker or
	 * highlighter (default pen), `color` #rrggbb, `width` a share of the
	 * canvas height (the tool's default).
	 */
	"drawing.addStroke": {
		params: { points: [number, number, number?, number?][]; tool?: string; color?: string; width?: number };
		result: { trackId: string; elementId: string; strokes: number };
	};
	/** Takes a drawing's last stroke away. */
	"drawing.removeLastStroke": { params: ApiElementLocator; result: { trackId: string; elementId: string; strokes: number } };
	"preview.capture": {
		params: { time?: number };
		result: {
			width: number;
			height: number;
			centre: [number, number, number];
			decoders: { key: string; width: number | null; height: number | null }[];
		};
	};

	"timeline.get": { params: Record<string, never>; result: ApiTimelineInfo };
	"timeline.addClip": {
		params: {
			mediaId: string;
			/** Start time in seconds; defaults to the playhead. */
			at?: number;
			/** Duration in seconds; defaults to the media duration. */
			duration?: number;
			trackId?: string;
			trackType?: ApiTrackType;
			name?: string;
		};
		result: ApiElementInfo;
	};
	/**
	 * Searches the sticker packs: `emoji` (Twemoji or Noto Emoji, in the set
	 * and skin tone chosen in the panel), `brands` (Simple Icons logos; they
	 * are trademarks), `icons` (open-licence icon sets), `flags`, or `all`.
	 */
	"stickers.search": {
		params: { query: string; pack?: "all" | "emoji" | "brands" | "icons" | "flags"; limit?: number };
		result: { id: string; name: string; pack: string; license?: string; requiresCredit?: boolean; credit?: string }[];
	};
	/**
	 * The text template library (row 98): titles, lower thirds, subscribe
	 * cards, end credits and a crawl, by category (the shared `titles` crate).
	 */
	"titles.list": {
		params: Record<string, never>;
		result: {
			categories: { id: string; label: string }[];
			templates: { id: string; label: string; category: string; description: string; duration: number; clips: number }[];
		};
	};
	/**
	 * Puts a text template at `at` seconds (default the playhead) as grouped
	 * text clips, placed for the canvas, in one undo step.
	 */
	"titles.add": {
		params: { id: string; at?: number };
		result: ApiElementInfo[];
	};
	/**
	 * The word-art style gallery (row 99): gradient fills, stacked
	 * outlines and inner shadows (our own designs, the shared `wordart` crate).
	 */
	"wordArt.list": {
		params: Record<string, never>;
		result: { styles: { id: string; label: string; description: string; params: Record<string, string | number> }[] };
	};
	/**
	 * Puts a gallery style on text clips (one undo step): sets every
	 * word-art param (clearing a previous style) and any colour it names.
	 * "none" makes them plain again.
	 */
	"wordArt.apply": {
		params: { elementIds: string[]; id: string };
		result: { changed: number };
	};
	/**
	 * Fits an audio clip to a length (row 101): cuts bars out or repeats
	 * bars where the music sounds the same either side (the shared
	 * `musicfit` crate), replacing the clip with pieces of the same source
	 * back to back that crossfade into each other, in one undo step.
	 * Without `seconds` it ends with everything else on the timeline.
	 */
	"music.fitToLength": {
		params: { elementId: string; trackId?: string; seconds?: number };
		result: {
			kind: "cut" | "loop" | "trim";
			seconds: number;
			bpm: number | null;
			trimmed: number;
			joins: { fromSeconds: number; toSeconds: number; score: number }[];
			pieces: ApiElementInfo[];
		};
	};
	/**
	 * Makes an audiogram (row 100) of a video or audio clip: listens to its
	 * sound and puts a graphic over the clip's span whose bars move with it
	 * (the shared `audiogram` crate; up to 10 minutes). Style it with
	 * `timeline.updateElement` params `style` (bars, mirror, line, circle),
	 * `color`, `barCount`, `size`, `width`, `y`, `gap`.
	 */
	"audiogram.make": {
		params: { elementId: string; trackId?: string };
		result: ApiElementInfo;
	};
	/** Puts a sticker (an id from `stickers.search`) on the timeline. */
	"timeline.addSticker": {
		params: { stickerId: string; at?: number; duration?: number; name?: string };
		result: ApiElementInfo;
	};
	"timeline.addText": {
		params: {
			text: string;
			at?: number;
			duration?: number;
			name?: string;
			params?: ApiParamValues;
			trackId?: string;
		};
		result: ApiElementInfo;
	};
	"timeline.updateElement": {
		params: ApiElementLocator & { patch: ApiElementPatch };
		result: ApiElementInfo;
	};
	"timeline.moveElement": {
		params: ApiElementLocator & { startTime: number };
		result: ApiElementInfo;
	};
	/** Sets, or with `transition: null` removes, the transition a clip leads out of. */
	/**
	 * `side` "end" (default) is the transition the clip leads out of; "start"
	 * is the one it comes in with: the previous clip's when one sits directly
	 * before it (one transition per cut), else its own, in from nothing.
	 */
	"timeline.setTransition": {
		params: ApiElementLocator & {
			transition: Omit<ApiTransition, "duration"> & { duration?: number } | null;
			side?: "start" | "end";
		};
		result: ApiElementInfo;
	};
	/** Every transition the renderer can draw, with its parameters. */
	/**
	 * Stock search: Openverse (CC0, public domain and CC BY pictures; no
	 * key, from the browser) or Pixabay (pictures and videos through the API
	 * worker, which needs PIXABAY_API_KEY). Results carry licence, creator,
	 * source page and a credit line.
	 */
	"stock.search": {
		params: { query: string; provider?: "openverse" | "pixabay"; kind?: "image" | "video"; page?: number };
		result: { items: ApiStockItem[]; total: number };
	};
	/** Downloads a search result into the project with its credit (and puts it at the playhead unless `addToTimeline` is false). */
	"stock.import": { params: { item: ApiStockItem; addToTimeline?: boolean }; result: { mediaId: string; name: string } };
	/** The credit lines of everything in the project that has one (stock media, sounds, stickers), one per line. */
	"stock.credits": { params: Record<string, never>; result: { text: string } };
	/**
	 * Script to B-roll, step 1: one shot per line of the script (the
	 * voice-over's lines), timed at `wordsPerSecond` (default 2.5, at least
	 * 1.5 s a shot) with the editor's own search words. With `claude: true`
	 * the API worker asks Claude for better words for the same lines (needs
	 * the server's ANTHROPIC_API_KEY and the passcode).
	 */
	"broll.plan": {
		params: { script: string; wordsPerSecond?: number; claude?: boolean; passcode?: string };
		result: { shots: ApiShot[]; model?: string };
	};
	/**
	 * Script to B-roll, step 2: for each shot, the first stock result for
	 * its query, imported with its credit and placed at `at` (seconds,
	 * default the playhead) + the shot's start, in one undo step.
	 */
	"broll.assemble": {
		params: { shots: ApiShot[]; provider?: "openverse" | "pixabay"; kind?: "image" | "video"; at?: number };
		result: { results: Array<{ shot: ApiShot; placed?: string; problem?: string }> };
	};
	/**
	 * The project's asset manifest, as a bundle's `assets.json` would hold
	 * it: every media file, library sound, sticker and font with its origin,
	 * licence (null when not known), source, credit line and use count.
	 * `lookUp: false` skips the online icon set and font licence lookups.
	 */
	"project.assets": { params: { lookUp?: boolean }; result: AssetManifest };
	/**
	 * Cleans up an image clip's picture on this device (for drawings on
	 * white): flood-fill white paper away with a soft edge, remove the white
	 * halo, feather `feather` px inwards, tint the ink (`tint` "#rrggbb"),
	 * crop to the drawing, and trace its longest skeleton path (the Shortform
	 * Studio's `TracedPath`: points as fractions, top end first). The result
	 * is a new PNG the clip shows; its original is kept for a restore.
	 */
	"picture.cleanup": {
		params: ApiElementLocator & {
			removeWhite?: boolean;
			decontaminate?: boolean;
			feather?: number;
			tint?: string | null;
			crop?: boolean;
			trace?: boolean;
		};
		result: { mediaId: string; width: number; height: number; path: { id: string; points: [number, number][] } | null };
	};
	/** Shows a cleaned clip's original picture again. */
	"picture.restore": { params: ApiElementLocator; result: ApiElementInfo };
	/**
	 * Voice-over: speaks `script` on this device with Kokoro (English voices,
	 * `voice` default "af_heart", `speed` 0.5–2), one line per sentence with
	 * pauses, adds the WAV to the project at `at` (timeline seconds, default
	 * the playhead) on an audio track, and with `captions` (default true) a
	 * caption track timed line by line.
	 */
	"audio.textToSpeech": {
		params: { script: string; voice?: string; speed?: number; captions?: boolean; at?: number };
		result: { mediaId: string; elementId: string | null; captionTrackId: string | null; lines: number; seconds: number };
	};
	/**
	 * Translates a caption track (default: the top one with captions) on this
	 * device into a new text track straight above it: the browser's built-in
	 * translator when it has the pair (`engine` "auto"), else Opus-MT models
	 * (through English when there is no direct one). Copies keep timing and
	 * look; word timings are dropped.
	 */
	"captions.translate": {
		params: { trackId?: string; from: string; to: string; engine?: "auto" | "browser" | "opus" };
		result: { trackId: string; captions: number; engine: "browser" | "opus"; models: string[]; translations: string[] };
	};
	/**
	 * Picks the keywords of every caption with timed words (or `elementIds`):
	 * `source` "local" (default) scores numbers, then names, then long
	 * words, skipping stop words, one to three per caption (the desktop's
	 * pick too); "claude" asks Claude through the API worker with the server
	 * `passcode`. Keywords are drawn in `caption.keywordColor` at
	 * `caption.keywordScale`. One undo step.
	 */
	"captions.pickKeywords": {
		params: { elementIds?: string[]; source?: "local" | "claude"; passcode?: string };
		result: { captions: number; keywords: number; model?: string };
	};
	/** Clears the keywords of every caption (or `elementIds`). One undo step. */
	"captions.clearKeywords": {
		params: { elementIds?: string[] };
		result: { captions: number; keywords: number };
	};
	/** Sets one caption's keywords to exactly these word indices. One undo step. */
	"captions.setKeywords": {
		params: ApiElementLocator & { indices: number[] };
		result: { captions: number; keywords: number };
	};
	/** Inserts captions as a new text track at the top of the overlays. */
	"captions.insert": {
		params: { captions: ApiCaption[]; style?: ApiCaptionStyle };
		result: { trackId: string; elements: ApiElementRef[] };
	};
	/**
	 * The scene's captions (visible text elements, or those of one track) as
	 * a subtitle file: SubRip, WebVTT (inline word timestamps) or ASS (`\k`
	 * karaoke tags, one style per look). `content` is the file's text.
	 */
	"captions.export": {
		params: { format: "srt" | "vtt" | "ass"; trackId?: string };
		result: { format: "srt" | "vtt" | "ass"; fileName: string; cueCount: number; content: string };
	};
	/**
	 * Transcribes the scene's audio in the page (Whisper, downloaded on first
	 * use) and, unless `insert` is false, inserts the captions.
	 */
	"captions.transcribe": {
		params: {
			language?: string;
			modelId?: string;
			wordsPerCaption?: number;
			insert?: boolean;
			style?: ApiCaptionStyle;
		};
		result: {
			text: string;
			words: ApiCaptionWord[];
			captions: ApiCaption[];
			trackId: string | null;
		};
	};
	"transitions.list": {
		params: Record<string, never>;
		result: ApiTransitionInfo[];
	};
	"timeline.split": {
		params: { elements: ApiElementLocator[]; time: number };
		result: ApiElementRef[];
	};
	/**
	 * Holds a video clip's frame at `time` (default: the playhead) for
	 * `seconds` (default 3): the clip splits there, a full-size still with
	 * the clip's look goes in between, and everything starting at or after
	 * `time` on unlocked tracks moves along. Without `elementId` it takes
	 * the selected video clip under the playhead, else the topmost.
	 */
	"timeline.freezeFrame": {
		params: { elementId?: string; trackId?: string; time?: number; seconds?: number };
		result: { assetId: string; trackId: string; elementId: string; sourceSeconds: number; seconds: number };
	};
	"timeline.delete": {
		params: { elements: ApiElementLocator[] };
		result: { deleted: number };
	};
	"timeline.duplicate": {
		params: { elements: ApiElementLocator[] };
		result: ApiElementRef[];
	};
	"timeline.addTrack": {
		params: { type: ApiTrackType; index?: number };
		result: ApiTrackInfo;
	};
	"timeline.removeTrack": {
		params: { trackId: string };
		result: { removed: true };
	};
	"timeline.setTrackMuted": {
		params: { trackId: string; muted: boolean };
		result: ApiTrackInfo;
	};
	/**
	 * The track mixer: sets an audio or video track's solo, volume in dB
	 * (-60, silent, to +12) and pan (-1 left to 1 right), those given, in
	 * one undo step. While any track is soloed only soloed tracks are heard
	 * (mute wins). The shared Rust `trackmix` crate, as on the desktop.
	 */
	"timeline.setTrackMix": {
		params: { trackId: string; solo?: boolean; volume?: number; pan?: number };
		result: ApiTrackInfo;
	};
	"timeline.setTrackHidden": {
		params: { trackId: string; hidden: boolean };
		result: ApiTrackInfo;
	};
	/** Locks a track (its clips cannot be selected, moved, trimmed, split or deleted; ripple edits skip it; nothing new lands on it) or unlocks it. */
	"timeline.setTrackLocked": {
		params: { trackId: string; locked: boolean };
		result: ApiTrackInfo;
	};
	/** Selects every clip on unlocked tracks overlapping `[from, to)` seconds (default: the in and out marks). */
	"selection.selectRange": {
		params: { from?: number; to?: number };
		result: { trackId: string; elementId: string }[];
	};
	/** Groups two or more clips: they are selected and moved together (Alt-click picks one). */
	"timeline.group": { params: { elements: ApiElementLocator[] }; result: ApiElementInfo[] };
	/** Ungroups the groups these clips belong to. */
	"timeline.ungroup": { params: { elements: ApiElementLocator[] }; result: ApiElementInfo[] };
	/** Combines the clips into one compound clip spanning them (on a video track); renderers and the mix play its children. */
	"timeline.createCompound": { params: { elements: ApiElementLocator[] }; result: ApiElementInfo };
	/** Puts a compound clip's visible content back on the timeline as grouped clips. */
	"timeline.breakApart": { params: ApiElementLocator; result: ApiElementInfo[] };
	/** The active scene's markers (the timeline's bookmarks), sorted by time. */
	"markers.list": { params: Record<string, never>; result: ApiMarker[] };
	/** Adds a marker at `time` seconds (default: the playhead); a marker already there is updated instead. */
	"markers.add": { params: { time?: number; note?: string; color?: string; duration?: number }; result: ApiMarker[] };
	"markers.update": { params: { time: number; note?: string | null; color?: string | null; duration?: number | null }; result: ApiMarker[] };
	"markers.remove": { params: { time: number }; result: ApiMarker[] };
	/** The markers as YouTube chapters (`MM:SS Title` lines from 00:00). */
	"markers.chapters": { params: Record<string, never>; result: { text: string } };

	"playback.getState": {
		params: Record<string, never>;
		result: ApiPlaybackState;
	};
	"playback.play": { params: Record<string, never>; result: ApiPlaybackState };
	"playback.pause": {
		params: Record<string, never>;
		result: ApiPlaybackState;
	};
	"playback.seek": { params: { time: number }; result: ApiPlaybackState };

	"selection.get": { params: Record<string, never>; result: ApiElementRef[] };
	"selection.set": {
		params: { elements: ApiElementLocator[] };
		result: ApiElementRef[];
	};
	"selection.clear": { params: Record<string, never>; result: ApiElementRef[] };

	"history.undo": {
		params: Record<string, never>;
		result: { canUndo: boolean; canRedo: boolean };
	};
	"history.redo": {
		params: Record<string, never>;
		result: { canUndo: boolean; canRedo: boolean };
	};
	"history.state": {
		params: Record<string, never>;
		result: { canUndo: boolean; canRedo: boolean };
	};

	"export.render": { params: ApiExportOptions; result: ApiExportResult };
	"export.download": {
		params: ApiExportOptions;
		result: { fileName: string; mimeType: string; bytes: number; mastered?: ApiExportResult["mastered"] };
	};
	/**
	 * Auto-ducking: measures the voice (every other audible clip, or
	 * `voice`) over the clip and writes volume keys that dip it while
	 * speech plays, replacing the clip's volume keys in one undo step.
	 */
	"audio.duck": { params: ApiDuckOptions; result: ApiDuckResult };
	/**
	 * EBU R128 loudness of a clip's audio (its trimmed source, before its
	 * volume): integrated LUFS, range, true peak and the loudest windows.
	 */
	"audio.measureLoudness": {
		params: ApiElementLocator;
		result: ApiLoudnessReport;
	};
	/**
	 * Measures the clip and sets its volume so the integrated loudness meets
	 * `targetLufs` (default -14) without the true peak passing
	 * `truePeakDbtp` (default -1); `target` names a preset instead.
	 */
	"audio.normalizeLoudness": {
		params: ApiElementLocator & { targetLufs?: number; truePeakDbtp?: number; target?: string };
		result: {
			element: ApiElementInfo;
			report: ApiLoudnessReport;
			gainDb: number;
			volumeDb: number;
			peakLimited: boolean;
			animatedVolume: boolean;
		};
	};
	/**
	 * Scans a video clip for shot changes (colour-histogram difference
	 * between frames, 10 a second) and, with `apply`, bookmarks the cuts or
	 * splits the clip there in one undo step.
	 */
	"shots.detect": {
		params: ApiElementLocator & {
			threshold?: number;
			minGapSeconds?: number;
			sampleFps?: number;
			apply?: "none" | "bookmarks" | "split";
		};
		result: {
			cuts: { sourceSeconds: number; timelineSeconds: number | null; score: number }[];
			framesScored: number;
			seconds: number;
			applied: number;
		};
	};
	/**
	 * Auto-reframe: scales a video clip to cover the canvas and pans it to
	 * keep faces (MediaPipe BlazeFace, on this device) or, in shots without
	 * any, movement and skin tone in view. `camera` steady holds one crop
	 * per shot, smooth (default) and follow pan; with `resetAtShots`
	 * (default) each shot is framed on its own. Writes the scale and
	 * `transform.positionX` / `positionY` keys in one undo step; key times
	 * are clip seconds, values canvas pixels from the centre.
	 */
	"reframe.apply": {
		params: ApiElementLocator & {
			camera?: "steady" | "smooth" | "follow";
			resetAtShots?: boolean;
			faces?: boolean;
			sampleFps?: number;
		};
		result: {
			scale: number;
			positionX: number;
			positionY: number;
			keysX: { time: number; value: number }[];
			keysY: { time: number; value: number }[];
			shots: number;
			cuts: number[];
			samples: number;
			faceSamples: number;
			fallbackShots: number;
			keyframes: number;
		};
	};
	/**
	 * Background removal: segments the people in a video clip on this device
	 * (MediaPipe Selfie Segmenter, `sampleFps` default 15), stores the matte
	 * as a small greyscale video in the project and gives the clip a
	 * `cutout` (white keeps). The desktop editor renders it too.
	 */
	"cutout.removeBackground": {
		params: ApiElementLocator & { sampleFps?: number };
		result: { matteId: string; matteName: string; frames: number; seconds: number; keptShare: number };
	};
	/** Keeps the background and hides the person instead (or back). */
	"cutout.setInverted": { params: ApiElementLocator & { inverted: boolean }; result: ApiElementInfo };
	/**
	 * Portrait mode: shows what the matte leaves out blurred (`blur`, the
	 * Blur effect's 1 to 100; default 60) instead of hiding it, or hides it
	 * again with `blur: 0`. Drawn by both editors as the whole clip blurred
	 * under the sharp cut-out.
	 */
	"cutout.setBackgroundBlur": { params: ApiElementLocator & { blur?: number }; result: ApiElementInfo };
	/** Shows the whole clip again; the matte stays in the media. */
	"cutout.clear": { params: ApiElementLocator; result: ApiElementInfo };
	/**
	 * Auto zoom punch-ins on a video or picture clip, planned from the
	 * captions over it: sentences alternate between no zoom and the normal
	 * zoom, sentences with a keyword, number or "!" take the emphasis zoom
	 * (`strength` subtle 1.08/1.15, medium 1.15/1.3, strong 1.25/1.5). With
	 * `face` (default for video) the zoom centres on the median face of a few
	 * frames. Written to `zoom.punches` (and `zoom.focusX/Y`), drawn in both
	 * editors on top of keyframes. One undo step. Times are clip seconds.
	 */
	"zoom.auto": {
		params: ApiElementLocator & { strength?: "subtle" | "medium" | "strong"; face?: boolean };
		result: { punches: { start: number; end: number; scale: number }[]; words: number; focus: { x: number; y: number } | null };
	};
	/** The split-screen layouts `layout.splitScreen` takes; cells are [x, y, width, height] shares of the canvas. */
	"layout.splitLayouts": { params: Record<string, never>; result: { id: string; label: string; cells: number[][] }[] };
	/**
	 * Puts 2 to 4 video or picture clips (in cell order) into a split-screen
	 * layout: each covers its cell (scale over the contain fit, position from
	 * the canvas centre), its transform keyframes are cleared and its masks
	 * replaced by a rectangle cropped to the cell. `gap` is canvas pixels
	 * between cells. One undo step.
	 */
	"layout.splitScreen": {
		params: { layout: string; elements: ApiElementRef[]; gap?: number };
		result: {
			elements: ApiElementRef[];
			placements: { positionX: number; positionY: number; scale: number; maskWidth: number; maskHeight: number }[];
		};
	};
	/**
	 * Face-only retouch: finds the faces in a video (5 looks a second over
	 * the part it plays) or picture on this device (MediaPipe BlazeFace),
	 * keeps them on the clip (`faces.track`) and sets its Retouch effect to
	 * "faces only" (adding it at its defaults if needed), so skin outside
	 * the faces - hands, arms - is left alone. One undo step.
	 */
	"retouch.faces": {
		params: ApiElementLocator;
		result: { trackId: string; elementId: string; looks: number; found: number };
	};
	/**
	 * Face blur: finds the faces in a video or picture as `retouch.faces`
	 * does, keeps them on the clip (`faces.track`) and turns on its Face
	 * blur effect (added at its defaults: blur, strength 1, size 1.2), which
	 * hides each face as it moves. Change the style (blur, pixelate, black)
	 * with `timeline.updateEffect`. One undo step.
	 */
	"faces.blur": {
		params: ApiElementLocator;
		result: { trackId: string; elementId: string; looks: number; found: number };
	};
	/**
	 * Multicam: lines two or more video clips up by their sound (the first
	 * stays put; each must be on its own track) and groups them as angles
	 * 1, 2, 3…; only angle 1's sound plays. Lags are seconds after angle
	 * 1's start; score is how well the sound matched (0 to 1). One undo step.
	 */
	"multicam.sync": {
		params: { elements: { elementId: string; trackId?: string }[] };
		result: { group: string; angles: { elementId: string; name: string; angle: number; lag: number; score: number }[] };
	};
	/** Shows `angle` (from 1) of the clip's multicam group from timeline second `at` (default: the playhead) on. One undo step. */
	"multicam.switch": {
		params: ApiElementLocator & { angle: number; at?: number };
		result: { cuts: string; count: number };
	};
	/** Takes every cut out of the clip's multicam group: angle 1 shows throughout. One undo step. */
	"multicam.clearCuts": {
		params: ApiElementLocator;
		result: { cleared: true };
	};
	/**
	 * Rich text: gives characters `start..end` of a text element's text
	 * (counting characters, not UTF-16 units; end exclusive) a colour
	 * (`#rgb` / `#rrggbb`), weight, slant or size (`scale` 0.25 to 4,
	 * relative to the clip's font size; the line keeps its height). Fields
	 * left out are not changed. The styles follow later edits to the text;
	 * drawn the same in both editors (`richtext`). One undo step.
	 */
	"text.styleRange": {
		params: ApiElementLocator & {
			start: number;
			end: number;
			color?: string;
			bold?: boolean;
			italic?: boolean;
			scale?: number;
		};
		result: { trackId: string; elementId: string; richText: { text: string; spans: unknown[] } };
	};
	/** Puts characters `start..end` (default: all) of a text element back to its own style. One undo step. */
	"text.clearStyles": {
		params: ApiElementLocator & { start?: number; end?: number };
		result: { trackId: string; elementId: string; richText: { text: string; spans: unknown[] } };
	};
	/**
	 * Deflicker on a video clip: evens out frame-to-frame brightness flicker
	 * (timelapses, LED or fluorescent light). The first strength other than
	 * `off` measures the clip's frames (160 px, 30 a second, over the part
	 * of the file it plays) and stores the levels on the clip; each frame is
	 * then multiplied by its neighbours' average level over its own (light 5,
	 * medium 11, strong 25 frames; cuts left alone), the same in both
	 * editors. One undo step.
	 */
	"video.deflicker": {
		params: ApiElementLocator & { strength: "off" | "light" | "medium" | "strong" };
		result: {
			trackId: string;
			elementId: string;
			strength: "off" | "light" | "medium" | "strong";
			frames: number;
			measured: boolean;
		};
	};
	/**
	 * Motion blur on a visual clip: how much of a frame the shutter stays
	 * open, 0 (off) to 100 (the whole frame; 50 is the film look). Where the
	 * clip moves (keyframes, one-click animations) it is drawn at evenly
	 * spaced points along the way and averaged, the same in both editors
	 * (the shared compositor). One undo step.
	 */
	"transform.motionBlur": {
		params: ApiElementLocator & { amount: number };
		result: { trackId: string; elementId: string; amount: number };
	};
	/**
	 * Curved text: bends a text element's line along an arc, -100 (a smile:
	 * the ends rise) to 100 (an arch: the ends drop; the text wraps 350
	 * degrees); 0 is flat. Drawn the same in both editors (`textwarp`). One
	 * undo step; refused while the curve has keyframes.
	 */
	"text.setCurve": {
		params: ApiElementLocator & { curve: number };
		result: { trackId: string; elementId: string; curve: number };
	};
	/**
	 * Text on a path (row 91): lays a text element along a shape (`wave`,
	 * `rise`, `swoosh`, `circle`; `amount` 0 to 100, default 50), along
	 * `points` (cubic Bézier control points, 3k + 1 of them, in units of the
	 * text's width around its anchor, y down), or flat again with
	 * `path: "none"`. Takes the place of the curve; drawn the same in both
	 * editors (`textwarp::warp_path`). One undo step.
	 */
	/**
	 * Tracked corner pin (row 92): `pinned` (a video, picture or sticker
	 * whose corner pin sits over a screen or sign at the playhead) gets
	 * corner pin keys that follow its four corners through `video` until
	 * either ends. One undo step.
	 */
	"tracking.cornerPin": {
		params: { video: ApiElementLocator; pinned: ApiElementLocator };
		result: { frames: number; lost: number; keys: number };
	};
	/**
	 * Tracked mask (row 92): a video's first mask follows its centre through
	 * the video from the playhead (`clear` stops it). Stored on the mask
	 * (`track`) and played back by both editors.
	 */
	"tracking.mask": { params: ApiElementLocator & { clear?: boolean }; result: { frames: number; lost: number } };
	/**
	 * Slip, slide or roll (row 93): `slip` moves what the clip shows by
	 * `delta` seconds (positive: later media), `slide` moves it between its
	 * neighbours (they give and take), `roll` moves the cut at its end (or
	 * at its start with `cut: "start"`). Clamped to what the media and
	 * neighbours allow; one undo step. The same edit as the desktop
	 * (`trimedit`).
	 */
	/**
	 * Caption find and replace (row 94) across every caption: `replace`
	 * (whole words, any case; the words keep their timing; empty
	 * `replacement` removes them), `highlight` (matches become keywords),
	 * `addEmoji` / `removeEmoji` (one emoji from our own word list after the
	 * first fitting word of each caption). `count` only counts matches.
	 * One undo step; the same as the desktop (`captionedit`).
	 */
	"captions.findReplace": {
		params: { op: "count" | "replace" | "highlight" | "addEmoji" | "removeEmoji"; query?: string; replacement?: string };
		result: { matches: number; changed: number };
	};
	"timeline.trimEdit": {
		params: ApiElementLocator & { tool: "slip" | "slide" | "roll"; delta: number; cut?: "start" | "end" };
		result: { delta: number; changed: string[] };
	};
	"text.setPath": {
		params: ApiElementLocator & { path: string; amount?: number; points?: [number, number][] };
		result: { trackId: string; elementId: string; path: string; amount: number };
	};
	/**
	 * Corner pin on a video, picture or sticker clip: its corners pinned in
	 * perspective (drawn by the shared compositor). `corners` are each
	 * corner's place in percent of the clip's own box before rotation
	 * (`topLeft` [0, 0] to `bottomRight` [100, 100]); `canvasCorners` are
	 * canvas pixels instead (where the corners should land now). Setting
	 * corners switches the pin on; a corner with keyframes gets a keyframe at
	 * the playhead. `reset` puts them back first. One undo step.
	 */
	"transform.cornerPin": {
		params: ApiElementLocator & {
			enabled?: boolean;
			corners?: Record<"topLeft" | "topRight" | "bottomRight" | "bottomLeft", [number, number]>;
			canvasCorners?: Record<"topLeft" | "topRight" | "bottomRight" | "bottomLeft", [number, number]>;
			reset?: boolean;
		};
		result: {
			trackId: string;
			elementId: string;
			enabled: boolean;
			/** False when the pin is off, at the clip's own corners, or not drawable. */
			drawn: boolean;
			corners: Record<"topLeft" | "topRight" | "bottomRight" | "bottomLeft", [number, number]>;
		};
	};
	/** The audio effect presets (from the shared Rust list). */
	"audio.fxPresets": {
		params: Record<string, never>;
		result: { id: string; label: string; low: number; mid: number; high: number; compressor: [number, number] | null; limiter: number | null }[];
	};
	/**
	 * Audio effects on an audio or video clip: a preset and/or the equaliser
	 * bands (`low` 120 Hz shelf, `mid` 1 kHz bell, `high` 8 kHz shelf; -12 to
	 * 12 dB), `compressor` ({ threshold dB, ratio } or null) and `limiter`
	 * (ceiling dB or null). Played live and in exports by the shared Rust
	 * code; the media is not changed. One undo step.
	 */
	"audio.setFx": {
		params: ApiElementLocator & {
			preset?: string;
			low?: number;
			mid?: number;
			high?: number;
			compressor?: { threshold: number; ratio: number } | null;
			limiter?: number | null;
		};
		result: { trackId: string; elementId: string; low: number; mid: number; high: number; compressor: [number, number] | null; limiter: number | null };
	};
	/**
	 * Motion tracking: the spot of `video` under `follower`'s centre at
	 * timeline time `at` (default: the playhead) is tracked through the video
	 * (template matching, the shared `tracking` crate) until either clip
	 * ends, and the follower (sticker, text, picture or clip) gets
	 * `transform.positionX` / `positionY` keys that keep it there. Frames
	 * where the spot was not found hold the last position. One undo step.
	 */
	"tracking.follow": {
		params: { video: ApiElementRef; follower: ApiElementRef; at?: number };
		result: { trackId: string; elementId: string; from: number; to: number; frames: number; lost: number; keys: number };
	};
	/** The project's cover settings (defaults when none were made). */
	"cover.get": { params: Record<string, never>; result: { time: number; title: string; style: "bold" | "band" | "top"; embed: boolean } };
	/** Changes the cover (title, style bold | band | top, frame time in timeline seconds, embed in MP4 exports). One undo step. */
	"cover.set": {
		params: { title?: string; style?: "bold" | "band" | "top"; time?: number; embed?: boolean };
		result: { time: number; title: string; style: "bold" | "band" | "top"; embed: boolean };
	};
	/** The timeline seconds of the best-looking of 16 evenly spaced frames (sharp, colourful, well exposed). Does not change the cover. */
	"cover.suggest": { params: Record<string, never>; result: { time: number } };
	/** The cover as a base64 PNG at the canvas shape (default), 16:9 (1280×720) or 9:16 (1080×1920). */
	"cover.render": {
		params: { shape?: "canvas" | "landscape" | "portrait" };
		result: { width: number; height: number; png: string };
	};
	/**
	 * Resize project: the canvas becomes `width` × `height` (16 to 8192) and
	 * every clip, text and caption is re-fitted. `mode` (default fit): fit
	 * shows the whole old picture, fill covers the new canvas with the edges
	 * cropped, keep keeps pixel sizes. Positions, position and scale
	 * keyframes, outline/shadow pixels and mask feather/stroke follow;
	 * captions are laid out again at the bottom. One undo step.
	 */
	"project.resize": {
		params: { width: number; height: number; mode?: "fit" | "fill" | "keep" };
		result: {
			from: { width: number; height: number };
			to: { width: number; height: number };
			mode: "fit" | "fill" | "keep";
			content: number;
			changed: number;
			captions: number;
		};
	};
	/**
	 * Colour match: `targets` (video or picture clips) take on the colours of
	 * `reference`'s frame at timeline time `at` seconds (default: the playhead;
	 * the reference's middle when outside it). Per-channel Lab mean and spread
	 * (Reinhard transfer) baked into a `lut` effect on each target; a target
	 * matched before has that effect updated. One undo step.
	 */
	"color.match": {
		params: { reference: ApiElementRef; targets: ApiElementRef[]; at?: number };
		result: {
			reference: ApiElementRef;
			referenceSeconds: number;
			referenceLab: { mean: number[]; spread: number[] };
			targets: (ApiElementRef & { lab: { mean: number[]; spread: number[] } })[];
		};
	};
	/** Removes a clip's punch-ins. One undo step. */
	"zoom.clear": { params: ApiElementLocator; result: ApiElementInfo };
	/** Puts a reframed clip back: scale 1, centred, no position or scale keys. */
	"reframe.clear": {
		params: ApiElementLocator;
		result: ApiElementInfo;
	};
	/**
	 * Tempo and beats of a clip's audio (onset energy, 60–200 BPM); with
	 * `apply: "bookmarks"` every `every`-th beat (default 1) is bookmarked,
	 * and the timeline snaps to bookmarks.
	 */
	"audio.detectBeats": {
		params: ApiElementLocator & { every?: number; apply?: "none" | "bookmarks" };
		result: {
			bpm: number | null;
			confidence: number;
			beats: { sourceSeconds: number; timelineSeconds: number | null }[];
			applied: number;
		};
	};
	/**
	 * Writes a denoised copy of the clip's audio (spectral subtraction of
	 * the steady background, `reductionDb` default 12) as a new media asset
	 * and points the clip at it; a video clip's audio is separated first.
	 */
	/**
	 * Crossfades a clip into the next audio or video clip on its track over
	 * `seconds` (default 1, at most 10) on `curve` (`equalPower`, the
	 * default, or `linear`), in one undo step; the shared `crossfade` crate,
	 * as on the desktop. Both play over the cut from their spare source
	 * audio; where that is short the two are trimmed to make room and the
	 * clips after move left (`trimmedSeconds`). `seconds: 0` removes it.
	 */
	"audio.crossfade": {
		params: ApiElementLocator & { seconds?: number; curve?: "equalPower" | "linear" };
		result: { seconds: number; trimmedSeconds: number; curve: "equalPower" | "linear"; nextElementId: string | null };
	};
	"audio.reduceNoise": {
		params: ApiElementLocator & { reductionDb?: number };
		result: { assetId: string; assetName: string; elementId: string; trackId: string; seconds: number; channels: number };
	};
	/**
	 * Finds the filler words ("um", "uh", …) of a clip from the captions
	 * over it or a transcription, and with `apply` (default true) cuts them
	 * out softly in one undo step.
	 */
	/** The voice effects `audio.voiceEffect` takes. */
	"audio.voiceEffects": { params: Record<string, never>; result: { id: string; label: string }[] };
	/**
	 * Changes the voice of a clip (`deeper`, `higher`, `chipmunk`, `monster`,
	 * `robot`, `echo`, `radio`, `hall`) into a new WAV asset the clip then
	 * plays, the same length; a video clip's audio is separated first. One
	 * undo step.
	 */
	"audio.voiceEffect": {
		params: ApiElementLocator & { effect: string };
		result: { assetId: string; assetName: string; elementId: string; trackId: string; seconds: number };
	};
	/**
	 * Enhance voice: one pass for clearer speech (80 Hz high-pass, noise
	 * reduction when the clip has pauses, 250 Hz cut and 3.5 kHz presence,
	 * de-esser, compressor with a pause expander, -16 LUFS under a -1 dBFS
	 * limiter) into a new WAV asset the clip then plays; the shared Rust
	 * `voice` crate, as the desktop runs it. Loudness is `null` for silence.
	 * One undo step.
	 */
	"audio.enhanceVoice": {
		params: ApiElementLocator;
		result: {
			assetId: string;
			assetName: string;
			elementId: string;
			trackId: string;
			seconds: number;
			inputLufs: number | null;
			outputLufs: number | null;
			noiseReduced: boolean;
		};
	};
	"audio.removeFillers": {
		params: ApiElementLocator & {
			language?: string;
			source?: "captions" | "transcribe" | "auto";
			ripple?: "all" | "track" | "none";
			apply?: boolean;
			modelId?: string;
		};
		result: {
			source: "captions" | "transcribe";
			language: string;
			words: number;
			fillers: { text: string; start: number; end: number }[];
			intervals: { start: number; end: number }[];
			seconds: number;
			applied: boolean;
			pieces: ApiElementRef[];
		};
	};
	/**
	 * Long video to shorts, step one: the open project's transcript (its
	 * word-timed captions) as lines, and the moments that stand on their
	 * own as shorts, picked by Claude through the API worker when
	 * `passcode` (the server passcode) is given, else locally.
	 */
	"shorts.plan": {
		params: { count?: number; minSeconds?: number; maxSeconds?: number; passcode?: string };
		result: {
			by: "claude" | "local";
			model: string;
			lines: { start: number; end: number; text: string }[];
			picks: { first: number; last: number; start: number; end: number; title: string; reason: string }[];
		};
	};
	/**
	 * Long video to shorts, step two: saves each pick (timeline seconds) as
	 * a new 9:16 project cut from the open one's scene, with its captions
	 * and a centred cover crop. The open project is unchanged.
	 */
	"shorts.make": {
		params: { picks: { start: number; end: number; title?: string }[]; paddingSeconds?: number };
		result: { projectId: string; name: string; start: number; end: number; seconds: number }[];
	};
	/**
	 * A clip's transcript: the timed words (timeline seconds) of the
	 * visible captions over it, indexed for `transcript.cut`.
	 */
	"transcript.get": {
		params: ApiElementLocator;
		result: { from: number; to: number; words: { index: number; text: string; start: number; end: number }[] };
	};
	/**
	 * Retakes in a clip's transcript (the shared `retakes` crate): earlier
	 * takes of a line (a phrase opening like one of the next three and
	 * mostly said again in it, with short chatter between) and false starts
	 * inside a phrase. `delete` lists every word index to cut; pass it to
	 * `transcript.cut` to keep only the last take of each line.
	 */
	"transcript.retakes": {
		params: ApiElementLocator & { pauseSeconds?: number; similarity?: number; windowSeconds?: number; restarts?: boolean };
		result: {
			retakes: { kind: "retake" | "restart"; words: number[]; kept: number; similarity: number; start: number; end: number; text: string; keptText: string }[];
			delete: number[];
		};
	};
	/**
	 * Text-based editing: cuts the words at `delete` (indices from
	 * `transcript.get`) out of the clip in one undo step, from the middle of
	 * the pause before each run to the middle of the pause after it, with
	 * the captions retimed to match. `apply: false` only reports the cuts.
	 */
	"transcript.cut": {
		params: ApiElementLocator & { delete: number[]; ripple?: "all" | "track" | "none"; apply?: boolean };
		result: { cuts: { start: number; end: number }[]; seconds: number; words: number; applied: boolean; pieces: ApiElementRef[] };
	};
	/**
	 * Parses a LUT file (.cube, .3dl or a HALD CLUT image) into the `lut`
	 * parameter of the LUT effect (`name|size|base64`, at most 33 a side).
	 */
	"luts.parse": {
		params: { file: File };
		result: { name: string; size: number; lut: string };
	};
	/**
	 * Sets a video or audio clip's speed: a constant `rate` (0.01–5), a
	 * `ramp` of rates at seconds from the clip's start, `reverse`, and
	 * `maintainPitch` for constant rates; `clear` removes it. The clip's
	 * duration changes so the same source span still plays.
	 */
	"timeline.setRetime": {
		params: ApiElementLocator & {
			rate?: number;
			maintainPitch?: boolean;
			reverse?: boolean;
			ramp?: { time: number; rate: number }[];
			clear?: boolean;
		};
		result: ApiElementInfo;
	};
	/** The named loudness targets `audio.normalizeLoudness` accepts. */
	"audio.loudnessTargets": {
		params: Record<string, never>;
		result: { id: string; label: string; lufs: number; truePeakDbtp: number }[];
	};
	/** Drops the clip's volume keys; its own volume applies again. */
	"audio.clearDucking": { params: { element: ApiElementRef }; result: ApiElementInfo };
	/** Open-licence fonts from Fontsource, each with its SPDX licence. */
	"fonts.list": {
		params: { query?: string; limit?: number };
		result: ApiFontInfo[];
	};
	/** Loads a Fontsource family into the page so text elements can use it. */
	"fonts.load": {
		params: { family: string };
		result: { family: string; license: string; faces: number };
	};
	/** Every keyframe of an element, by animated path (colour channels appear per component, e.g. `color.r`). */
	"timeline.getKeyframes": {
		params: ApiElementLocator;
		result: Record<string, ApiKeyframeInfo[]>;
	};
	/**
	 * Sets a keyframe at `time` seconds from the element's start on an
	 * animatable path (`transform.positionX`, `opacity`, `volume`, `color`,
	 * `effects.<id>.params.<key>`, …); a key already at that time is replaced.
	 */
	"timeline.setKeyframe": {
		params: ApiElementLocator & {
			path: string;
			time: number;
			value: number | string | boolean;
			interpolation?: "linear" | "hold" | "bezier";
		};
		result: ApiElementInfo;
	};
	/** Removes the key at `time` (or with `keyframeId`) on `path`. */
	"timeline.removeKeyframe": {
		params: ApiElementLocator & { path: string; time?: number; keyframeId?: string };
		result: ApiElementInfo;
	};
	/** Drops every key of `path`; the element's own value applies again. */
	"timeline.clearKeyframes": {
		params: ApiElementLocator & { path: string };
		result: ApiElementInfo;
	};
	/** The mask shapes available, with their parameters. */
	"masks.list": { params: Record<string, never>; result: ApiMaskDefinitionInfo[] };
	/** Puts a mask of `type` on a video or image element (replacing any mask), at its defaults plus `params`. */
	"timeline.setMask": {
		params: ApiElementLocator & { type: string; params?: Record<string, unknown> };
		result: ApiElementInfo;
	};
	/** Changes the element's mask parameters (`inverted` is a shortcut for the param of that name). */
	"timeline.updateMask": {
		params: ApiElementLocator & { params?: Record<string, unknown>; inverted?: boolean };
		result: ApiElementInfo;
	};
	"timeline.removeMask": { params: ApiElementLocator; result: ApiElementInfo };
	/** The effects both editors offer, with their parameters. */
	"effects.list": { params: Record<string, never>; result: ApiEffectDefinitionInfo[] };
	/** Adds an effect to a visual element at its defaults, then applies `params`. */
	"timeline.addEffect": {
		params: ApiElementLocator & { type: string; params?: ApiParamValues };
		result: ApiElementInfo;
	};
	"timeline.updateEffect": {
		params: ApiElementLocator & { effectId: string; params?: ApiParamValues; enabled?: boolean };
		result: ApiElementInfo;
	};
	"timeline.removeEffect": {
		params: ApiElementLocator & { effectId: string };
		result: ApiElementInfo;
	};
	/** Finds the silent stretches of a clip without changing anything. */
	"audio.detectSilence": { params: ApiSilenceOptions; result: ApiSilenceReport };
	/**
	 * Cuts the silent stretches out of a clip in one undo step: splits at
	 * each stretch's edges, removes the pieces inside and, with `ripple`
	 * "all" (default) or "track", moves everything that starts after a
	 * removed stretch left to close the gap. Elements on other tracks that
	 * straddle a stretch stay put.
	 */
	"audio.cutSilence": {
		params: ApiSilenceOptions & { ripple?: "all" | "track" | "none" };
		result: ApiSilenceCutResult;
	};
	/** Platform presets with the encoder settings for the open project. */
	"export.presets": { params: Record<string, never>; result: ApiPlatformPreset[] };
	/** The video codecs a container takes and which of them this browser can encode at the canvas size. */
	"export.codecs": { params: { format?: ApiExportFormat; hardware?: boolean }; result: ApiExportCodecs };
	/** What the platform would object to in the project as it is. */
	"export.check": {
		params: { preset: string; includeAudio?: boolean };
		result: ApiPlatformCheck;
	};
}

export type EditorApiMethodName = keyof EditorApiMethods;

export type EditorApiParams<M extends EditorApiMethodName> =
	EditorApiMethods[M]["params"];
export type EditorApiResult<M extends EditorApiMethodName> =
	EditorApiMethods[M]["result"];

/** Change notifications broadcast by the bridge. */
export type EditorApiEventName =
	| "project"
	| "timeline"
	| "media"
	| "playback"
	| "selection"
	| "history";

export interface EditorApiEvent {
	event: EditorApiEventName;
	/** Monotonic counter so late listeners can detect missed events. */
	sequence: number;
}

/** Wire protocol shared by every remote consumer. */
export interface ApiRequestMessage {
	type: "opencut:request";
	id: string;
	method: string;
	params?: unknown;
}

export interface ApiResponseMessage {
	type: "opencut:response";
	id: string;
	ok: boolean;
	result?: unknown;
	error?: { message: string; code: ApiErrorCode };
}

export interface ApiEventMessage {
	type: "opencut:event";
	event: EditorApiEventName;
	sequence: number;
}

export interface ApiReadyMessage {
	type: "opencut:ready";
	apiVersion: string;
}

export type ApiMessage =
	| ApiRequestMessage
	| ApiResponseMessage
	| ApiEventMessage
	| ApiReadyMessage;

export type ApiErrorCode =
	| "not_found"
	| "invalid_params"
	| "no_project"
	| "unknown_method"
	| "failed";

export class EditorApiError extends Error {
	readonly code: ApiErrorCode;

	constructor({ code, message }: { code: ApiErrorCode; message: string }) {
		super(message);
		this.name = "EditorApiError";
		this.code = code;
	}
}

export function isApiMessage(value: unknown): value is ApiMessage {
	return (
		typeof value === "object" &&
		value !== null &&
		typeof (value as { type?: unknown }).type === "string" &&
		(value as { type: string }).type.startsWith("opencut:")
	);
}
