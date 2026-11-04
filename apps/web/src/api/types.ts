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
	| "effect";

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
}

export interface ApiElementInfo {
	id: string;
	trackId: string;
	type: ApiElementType;
	name: string;
	startTime: number;
	duration: number;
	trimStart: number;
	trimEnd: number;
	mediaId?: string;
	hidden?: boolean;
	params: ApiParamValues;
	/** The transition the clip leads out of (visual elements only). */
	transition?: ApiTransition;
}

export type ApiTransitionParamValue = number | boolean | number[];

/** A clip's transition: a catalogue name, a duration in seconds, parameters. */
export interface ApiTransition {
	type: string;
	duration: number;
	params?: Record<string, ApiTransitionParamValue>;
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
	"timeline.setTransition": {
		params: ApiElementLocator & {
			transition: Omit<ApiTransition, "duration"> & { duration?: number } | null;
		};
		result: ApiElementInfo;
	};
	/** Every transition the renderer can draw, with its parameters. */
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
