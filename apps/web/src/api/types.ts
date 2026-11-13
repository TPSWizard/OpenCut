/**
 * OpenCut Editor API, version 1.
 *
 * Everything here is JSON-serialisable so the same surface can be driven from
 * the in-editor scripting tab (a Worker), plugins (sandboxed iframes), the
 * headless runner (Playwright) and the MCP server. Times are seconds at this
 * boundary; the editor converts them to its integer tick representation.
 */

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
	type: "image" | "video" | "audio";
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
	type: "image" | "video" | "audio";
	size: number;
}

export interface ApiPreviewSettings {
	/** How finely the preview decodes video: auto fits the canvas, half and quarter go lower, full decodes every pixel. */
	quality: ApiPreviewQuality;
	useProxies: boolean;
	autoProxies: boolean;
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
}

/** How a caption's spoken word stands out; see the `caption.*` text params. */
export interface ApiCaptionStyle {
	highlight?: "none" | "word" | "box" | "fill";
	activeColor?: string;
	boxColor?: string;
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

export interface ApiExportOptions {
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
	type: "number" | "select";
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
		result: { fileName: string; mimeType: string; bytes: number };
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
	/** Shows the whole clip again; the matte stays in the media. */
	"cutout.clear": { params: ApiElementLocator; result: ApiElementInfo };
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
	"audio.reduceNoise": {
		params: ApiElementLocator & { reductionDb?: number };
		result: { assetId: string; assetName: string; elementId: string; trackId: string; seconds: number; channels: number };
	};
	/**
	 * Finds the filler words ("um", "uh", …) of a clip from the captions
	 * over it or a transcription, and with `apply` (default true) cuts them
	 * out softly in one undo step.
	 */
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
