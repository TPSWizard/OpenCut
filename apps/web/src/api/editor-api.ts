import type { EditorCore } from "@/core";
import { processMediaAssets } from "@/media/processing";
import type { MediaAsset } from "@/media/types";
import { floatToFrameRate, frameRateToFloat } from "@/fps/utils";
import { getExportFileExtension, getExportMimeType } from "@/export";
import {
	buildElementFromMedia,
	buildTextElement,
} from "@/timeline/element-utils";
import { DEFAULT_NEW_ELEMENT_DURATION } from "@/timeline/creation";
import type {
	SceneTracks,
	TimelineElement,
	TimelineTrack,
	TrackType,
} from "@/timeline/types";
import type { TProject, TProjectSettings } from "@/project/types";
import {
	mediaTimeFromSeconds,
	mediaTimeToSeconds,
	type MediaTime,
} from "@/wasm";
import {
	EDITOR_API_VERSION,
	EditorApiError,
	type ApiElementInfo,
	type ApiElementLocator,
	type ApiElementRef,
	type ApiExportOptions,
	type ApiMediaInfo,
	type ApiPlaybackState,
	type ApiProjectInfo,
	type ApiProjectSummary,
	type ApiSceneInfo,
	type ApiTimelineInfo,
	type ApiTrackInfo,
	type EditorApiMethodName,
	type EditorApiMethods,
	type EditorApiParams,
	type EditorApiResult,
} from "./types";

type Handlers = {
	[M in EditorApiMethodName]: (
		params: EditorApiParams<M>,
	) => EditorApiResult<M> | Promise<EditorApiResult<M>>;
};

export interface EditorApi {
	readonly version: string;
	dispatch: <M extends EditorApiMethodName>(
		method: M,
		params: EditorApiParams<M>,
	) => Promise<EditorApiResult<M>>;
	methods: readonly EditorApiMethodName[];
}

function seconds(time: MediaTime): number {
	return mediaTimeToSeconds({ time });
}

function ticks(value: number, field: string): MediaTime {
	if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
		throw new EditorApiError({
			code: "invalid_params",
			message: `${field} must be a non-negative number of seconds`,
		});
	}
	return mediaTimeFromSeconds({ seconds: value });
}

function requireString(value: unknown, field: string): string {
	if (typeof value !== "string" || value.length === 0) {
		throw new EditorApiError({
			code: "invalid_params",
			message: `${field} must be a non-empty string`,
		});
	}
	return value;
}

function laneOf({
	tracks,
	trackId,
}: {
	tracks: SceneTracks;
	trackId: string;
}): ApiTrackInfo["lane"] {
	if (tracks.main.id === trackId) return "main";
	if (tracks.audio.some((track) => track.id === trackId)) return "audio";
	return "overlay";
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

function toElementInfo({
	element,
	trackId,
}: {
	element: TimelineElement;
	trackId: string;
}): ApiElementInfo {
	return {
		id: element.id,
		trackId,
		type: element.type,
		name: element.name,
		startTime: seconds(element.startTime),
		duration: seconds(element.duration),
		trimStart: seconds(element.trimStart),
		trimEnd: seconds(element.trimEnd),
		mediaId: "mediaId" in element ? element.mediaId : undefined,
		hidden: "hidden" in element ? element.hidden : undefined,
		params: { ...element.params },
	};
}

function toTrackInfo({
	track,
	tracks,
}: {
	track: TimelineTrack;
	tracks: SceneTracks;
}): ApiTrackInfo {
	return {
		id: track.id,
		name: track.name,
		type: track.type,
		lane: laneOf({ tracks, trackId: track.id }),
		muted: "muted" in track ? track.muted : undefined,
		hidden: "hidden" in track ? track.hidden : undefined,
		elements: (track.elements as TimelineElement[]).map((element) =>
			toElementInfo({ element, trackId: track.id }),
		),
	};
}

function toMediaInfo(asset: MediaAsset): ApiMediaInfo {
	return {
		id: asset.id,
		name: asset.name,
		type: asset.type,
		duration: asset.duration,
		width: asset.width,
		height: asset.height,
		fps: asset.fps,
		hasAudio: asset.hasAudio,
	};
}

function fileNameFromUrl(url: string): string {
	try {
		const pathname = new URL(url, "http://localhost").pathname;
		const last = pathname.split("/").filter(Boolean).pop();
		if (last) return decodeURIComponent(last);
	} catch {
		// fall through
	}
	return "media";
}

/**
 * Build the Editor API over a live `EditorCore`. Every method validates its
 * input, converts seconds to ticks, runs through the editor's undoable
 * commands and returns plain JSON.
 */
export function createEditorApi({ editor }: { editor: EditorCore }): EditorApi {
	const requireProject = (): TProject => {
		const project = editor.project.getActiveOrNull();
		if (!project) {
			throw new EditorApiError({
				code: "no_project",
				message: "No project is open",
			});
		}
		return project;
	};

	const tracksOf = (): SceneTracks => {
		requireProject();
		return editor.scenes.getActiveScene().tracks;
	};

	const projectInfo = (): ApiProjectInfo => {
		const project = requireProject();
		return {
			id: project.metadata.id,
			name: project.metadata.name,
			fps: frameRateToFloat(project.settings.fps),
			canvas: { ...project.settings.canvasSize },
			background: project.settings.background,
			duration: seconds(editor.timeline.getTotalDuration()),
			currentSceneId: project.currentSceneId,
			createdAt: new Date(project.metadata.createdAt).toISOString(),
			updatedAt: new Date(project.metadata.updatedAt).toISOString(),
		};
	};

	const resolveElement = (
		locator: ApiElementLocator,
	): { track: TimelineTrack; element: TimelineElement } => {
		const elementId = requireString(locator.elementId, "elementId");
		const tracks = tracksOf();
		for (const track of allTracks(tracks)) {
			if (locator.trackId && track.id !== locator.trackId) continue;
			const element = (track.elements as TimelineElement[]).find(
				(candidate) => candidate.id === elementId,
			);
			if (element) return { track, element };
		}
		throw new EditorApiError({
			code: "not_found",
			message: `Element ${elementId} was not found`,
		});
	};

	const resolveRefs = (locators: ApiElementLocator[]): ApiElementRef[] => {
		if (!Array.isArray(locators)) {
			throw new EditorApiError({
				code: "invalid_params",
				message: "elements must be an array",
			});
		}
		return locators.map((locator) => {
			const { track, element } = resolveElement(locator);
			return { trackId: track.id, elementId: element.id };
		});
	};

	const resolveTrack = (trackId: string): TimelineTrack => {
		const track = editor.timeline.getTrackById({
			trackId: requireString(trackId, "trackId"),
		});
		if (!track) {
			throw new EditorApiError({
				code: "not_found",
				message: `Track ${trackId} was not found`,
			});
		}
		return track;
	};

	const elementInfoById = (ref: ApiElementRef): ApiElementInfo => {
		const { track, element } = resolveElement(ref);
		return toElementInfo({ element, trackId: track.id });
	};

	const timelineInfo = (): ApiTimelineInfo => {
		const tracks = tracksOf();
		return {
			sceneId: editor.scenes.getActiveScene().id,
			duration: seconds(editor.timeline.getTotalDuration()),
			tracks: allTracks(tracks).map((track) =>
				toTrackInfo({ track, tracks }),
			),
		};
	};

	const playbackState = (): ApiPlaybackState => ({
		isPlaying: editor.playback.getIsPlaying(),
		time: seconds(editor.playback.getCurrentTime()),
		duration: seconds(editor.timeline.getTotalDuration()),
		volume: editor.playback.getVolume(),
		muted: editor.playback.isMuted(),
	});

	const sceneList = (): ApiSceneInfo[] => {
		requireProject();
		const active = editor.scenes.getActiveSceneOrNull();
		return editor.scenes.getScenes().map((scene) => ({
			id: scene.id,
			name: scene.name,
			isMain: scene.isMain,
			isActive: scene.id === active?.id,
		}));
	};

	const historyState = () => ({
		canUndo: editor.command.canUndo(),
		canRedo: editor.command.canRedo(),
	});

	const findInsertedElement = ({
		before,
		after,
	}: {
		before: Set<string>;
		after: SceneTracks;
	}): ApiElementInfo => {
		for (const track of allTracks(after)) {
			for (const element of track.elements as TimelineElement[]) {
				if (!before.has(element.id)) {
					return toElementInfo({ element, trackId: track.id });
				}
			}
		}
		throw new EditorApiError({
			code: "failed",
			message: "The element could not be inserted",
		});
	};

	const elementIds = (tracks: SceneTracks): Set<string> =>
		new Set(
			allTracks(tracks).flatMap((track) =>
				(track.elements as TimelineElement[]).map((element) => element.id),
			),
		);

	const importFiles = async (files: File[]): Promise<ApiMediaInfo[]> => {
		const project = requireProject();
		const processed = await processMediaAssets({ files });
		const added: ApiMediaInfo[] = [];
		for (const asset of processed) {
			const saved = await editor.media.addMediaAsset({
				projectId: project.metadata.id,
				asset,
			});
			if (!saved) {
				throw new EditorApiError({
					code: "failed",
					message: `Failed to store ${asset.name}`,
				});
			}
			added.push(toMediaInfo(saved));
		}
		return added;
	};

	const renderExport = async (options: ApiExportOptions) => {
		const project = requireProject();
		const format = options.format ?? "mp4";
		const quality = options.quality ?? "high";
		if (format !== "mp4" && format !== "webm") {
			throw new EditorApiError({
				code: "invalid_params",
				message: `Unknown export format ${String(format)}`,
			});
		}
		const result = await editor.project.export({
			options: {
				format,
				quality,
				includeAudio: options.includeAudio ?? true,
				fps: options.fps ? floatToFrameRate(options.fps) : undefined,
			},
		});
		if (!result.success || !result.buffer) {
			throw new EditorApiError({
				code: "failed",
				message: result.error ?? (result.cancelled ? "Export cancelled" : "Export failed"),
			});
		}
		const baseName = (options.fileName ?? project.metadata.name).replace(
			/[\\/:*?"<>|]+/g,
			"_",
		);
		return {
			fileName: `${baseName}${getExportFileExtension({ format })}`,
			mimeType: getExportMimeType({ format }),
			bytes: result.buffer,
		};
	};

	const handlers: Handlers = {
		"app.info": () => ({
			apiVersion: EDITOR_API_VERSION,
			ready: editor.project.getActiveOrNull() !== null,
			projectId: editor.project.getActiveOrNull()?.metadata.id ?? null,
		}),

		"project.get": () => projectInfo(),
		"project.create": async ({ name }) => {
			const projectName =
				typeof name === "string" && name.trim().length > 0
					? name.trim()
					: "New project";
			const id = await editor.project.createNewProject({ name: projectName });
			await editor.project.loadAllProjects();
			const metadata = editor.project
				.getSavedProjects()
				.find((item) => item.id === id);
			return {
				id,
				name: metadata?.name ?? projectName,
				duration: metadata ? seconds(metadata.duration) : 0,
				createdAt: new Date(metadata?.createdAt ?? Date.now()).toISOString(),
				updatedAt: new Date(metadata?.updatedAt ?? Date.now()).toISOString(),
			};
		},
		"project.list": async (): Promise<ApiProjectSummary[]> => {
			await editor.project.loadAllProjects();
			return editor.project.getSavedProjects().map((metadata) => ({
				id: metadata.id,
				name: metadata.name,
				duration: seconds(metadata.duration),
				createdAt: new Date(metadata.createdAt).toISOString(),
				updatedAt: new Date(metadata.updatedAt).toISOString(),
			}));
		},
		"project.rename": async ({ name }) => {
			const project = requireProject();
			await editor.project.renameProject({
				id: project.metadata.id,
				name: requireString(name, "name").trim(),
			});
			return projectInfo();
		},
		"project.updateSettings": async ({ fps, canvas, background }) => {
			requireProject();
			const settings: Partial<TProjectSettings> = {};
			if (fps !== undefined) {
				if (typeof fps !== "number" || !(fps > 0)) {
					throw new EditorApiError({
						code: "invalid_params",
						message: "fps must be a positive number",
					});
				}
				settings.fps = floatToFrameRate(fps);
			}
			if (canvas !== undefined) {
				if (
					!Number.isInteger(canvas?.width) ||
					!Number.isInteger(canvas?.height) ||
					canvas.width <= 0 ||
					canvas.height <= 0
				) {
					throw new EditorApiError({
						code: "invalid_params",
						message: "canvas needs positive integer width and height",
					});
				}
				settings.canvasSize = { width: canvas.width, height: canvas.height };
				settings.canvasSizeMode = "custom";
			}
			if (background !== undefined) {
				settings.background = background;
			}
			await editor.project.updateSettings({ settings });
			return projectInfo();
		},
		"project.save": async () => {
			requireProject();
			await editor.save.flush();
			return { saved: true as const };
		},

		"scenes.list": () => sceneList(),
		"scenes.switch": async ({ sceneId }) => {
			requireProject();
			await editor.scenes.switchToScene({
				sceneId: requireString(sceneId, "sceneId"),
			});
			return sceneList();
		},

		"media.list": () => {
			requireProject();
			return editor.media.getAssets().map(toMediaInfo);
		},
		"media.importUrl": async ({ url, name }) => {
			requireProject();
			const source = requireString(url, "url");
			const response = await fetch(source);
			if (!response.ok) {
				throw new EditorApiError({
					code: "failed",
					message: `Fetching ${source} failed with ${response.status}`,
				});
			}
			const blob = await response.blob();
			const file = new File([blob], name ?? fileNameFromUrl(source), {
				type: blob.type || "application/octet-stream",
			});
			const [asset] = await importFiles([file]);
			if (!asset) {
				throw new EditorApiError({
					code: "failed",
					message: `${source} is not a supported media file`,
				});
			}
			return asset;
		},
		"media.importFiles": async ({ files }) => {
			if (!Array.isArray(files) || files.some((file) => !(file instanceof File))) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "files must be an array of File objects",
				});
			}
			return importFiles(files);
		},
		"media.remove": ({ mediaId }) => {
			const project = requireProject();
			const id = requireString(mediaId, "mediaId");
			if (!editor.media.getAssets().some((asset) => asset.id === id)) {
				throw new EditorApiError({
					code: "not_found",
					message: `Media ${id} was not found`,
				});
			}
			editor.media.removeMediaAsset({ projectId: project.metadata.id, id });
			return { removed: true as const };
		},

		"timeline.get": () => timelineInfo(),
		"timeline.addClip": ({ mediaId, at, duration, trackId, trackType, name }) => {
			requireProject();
			const id = requireString(mediaId, "mediaId");
			const asset = editor.media.getAssets().find((item) => item.id === id);
			if (!asset) {
				throw new EditorApiError({
					code: "not_found",
					message: `Media ${id} was not found`,
				});
			}
			const startTime =
				at === undefined ? editor.playback.getCurrentTime() : ticks(at, "at");
			const elementDuration =
				duration !== undefined
					? ticks(duration, "duration")
					: asset.duration != null
						? mediaTimeFromSeconds({ seconds: asset.duration })
						: DEFAULT_NEW_ELEMENT_DURATION;
			const element = buildElementFromMedia({
				mediaId: asset.id,
				mediaType: asset.type,
				name: name ?? asset.name,
				duration: elementDuration,
				startTime,
				buffer:
					asset.type === "audio"
						? new AudioBuffer({ length: 1, sampleRate: 44100 })
						: undefined,
			});
			const before = elementIds(tracksOf());
			editor.timeline.insertElement({
				element,
				placement: trackId
					? { mode: "explicit", trackId: resolveTrack(trackId).id }
					: {
							mode: "auto",
							trackType:
								trackType ?? (asset.type === "audio" ? "audio" : "video"),
						},
			});
			return findInsertedElement({ before, after: tracksOf() });
		},
		"timeline.addText": ({ text, at, duration, name, params, trackId }) => {
			requireProject();
			const content = requireString(text, "text");
			const startTime =
				at === undefined ? editor.playback.getCurrentTime() : ticks(at, "at");
			const element = buildTextElement({
				raw: {
					name: name ?? content.slice(0, 24),
					duration: duration !== undefined ? ticks(duration, "duration") : undefined,
					params: { ...(params ?? {}), content },
				},
				startTime,
			});
			const before = elementIds(tracksOf());
			editor.timeline.insertElement({
				element,
				placement: trackId
					? { mode: "explicit", trackId: resolveTrack(trackId).id }
					: { mode: "auto", trackType: "text" },
			});
			return findInsertedElement({ before, after: tracksOf() });
		},
		"timeline.updateElement": ({ elementId, trackId, patch }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (!patch || typeof patch !== "object") {
				throw new EditorApiError({
					code: "invalid_params",
					message: "patch must be an object",
				});
			}
			const timeFields = ["startTime", "duration", "trimStart", "trimEnd"] as const;
			if (timeFields.some((field) => patch[field] !== undefined)) {
				editor.timeline.updateElementTrim({
					elementId: element.id,
					trimStart:
						patch.trimStart !== undefined
							? ticks(patch.trimStart, "trimStart")
							: element.trimStart,
					trimEnd:
						patch.trimEnd !== undefined
							? ticks(patch.trimEnd, "trimEnd")
							: element.trimEnd,
					startTime:
						patch.startTime !== undefined
							? ticks(patch.startTime, "startTime")
							: undefined,
					duration:
						patch.duration !== undefined
							? ticks(patch.duration, "duration")
							: undefined,
				});
			}
			const plain: Partial<TimelineElement> = {};
			if (patch.name !== undefined) plain.name = requireString(patch.name, "name");
			if (patch.hidden !== undefined) {
				(plain as { hidden?: boolean }).hidden = Boolean(patch.hidden);
			}
			if (patch.params !== undefined) {
				plain.params = { ...element.params, ...patch.params };
			}
			if (Object.keys(plain).length > 0) {
				editor.timeline.updateElements({
					updates: [{ trackId: track.id, elementId: element.id, patch: plain }],
				});
			}
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.moveElement": ({ elementId, trackId, startTime }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			editor.timeline.updateElementTrim({
				elementId: element.id,
				trimStart: element.trimStart,
				trimEnd: element.trimEnd,
				startTime: ticks(startTime, "startTime"),
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.split": ({ elements, time }) =>
			editor.timeline.splitElements({
				elements: resolveRefs(elements),
				splitTime: ticks(time, "time"),
			}),
		"timeline.delete": ({ elements }) => {
			const refs = resolveRefs(elements);
			editor.timeline.deleteElements({ elements: refs });
			return { deleted: refs.length };
		},
		"timeline.duplicate": ({ elements }) =>
			editor.timeline.duplicateElements({ elements: resolveRefs(elements) }),
		"timeline.addTrack": ({ type, index }) => {
			requireProject();
			const allowed: TrackType[] = ["video", "text", "audio", "graphic", "effect"];
			if (!allowed.includes(type)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `type must be one of ${allowed.join(", ")}`,
				});
			}
			const trackId = editor.timeline.addTrack({ type, index });
			const tracks = tracksOf();
			return toTrackInfo({ track: resolveTrack(trackId), tracks });
		},
		"timeline.removeTrack": ({ trackId }) => {
			const track = resolveTrack(trackId);
			editor.timeline.removeTrack({ trackId: track.id });
			return { removed: true as const };
		},
		"timeline.setTrackMuted": ({ trackId, muted }) => {
			const track = resolveTrack(trackId);
			if (!("muted" in track)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `${track.type} tracks cannot be muted`,
				});
			}
			if (track.muted !== Boolean(muted)) {
				editor.timeline.toggleTrackMute({ trackId: track.id });
			}
			return toTrackInfo({ track: resolveTrack(track.id), tracks: tracksOf() });
		},
		"timeline.setTrackHidden": ({ trackId, hidden }) => {
			const track = resolveTrack(trackId);
			if (!("hidden" in track)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `${track.type} tracks cannot be hidden`,
				});
			}
			if (track.hidden !== Boolean(hidden)) {
				editor.timeline.toggleTrackVisibility({ trackId: track.id });
			}
			return toTrackInfo({ track: resolveTrack(track.id), tracks: tracksOf() });
		},

		"playback.getState": () => {
			requireProject();
			return playbackState();
		},
		"playback.play": () => {
			requireProject();
			editor.playback.play();
			return playbackState();
		},
		"playback.pause": () => {
			requireProject();
			editor.playback.pause();
			return playbackState();
		},
		"playback.seek": ({ time }) => {
			requireProject();
			editor.playback.seek({ time: ticks(time, "time") });
			return playbackState();
		},

		"selection.get": () => {
			requireProject();
			return editor.selection.getSelectedElements();
		},
		"selection.set": ({ elements }) => {
			editor.selection.setSelectedElements({ elements: resolveRefs(elements) });
			return editor.selection.getSelectedElements();
		},
		"selection.clear": () => {
			requireProject();
			editor.selection.clearSelection();
			return editor.selection.getSelectedElements();
		},

		"history.undo": () => {
			editor.command.undo();
			return historyState();
		},
		"history.redo": () => {
			editor.command.redo();
			return historyState();
		},
		"history.state": () => historyState(),

		"export.render": (options) => renderExport(options),
		"export.download": async (options) => {
			const rendered = await renderExport(options);
			const blob = new Blob([rendered.bytes], { type: rendered.mimeType });
			const url = URL.createObjectURL(blob);
			const link = document.createElement("a");
			link.href = url;
			link.download = rendered.fileName;
			document.body.appendChild(link);
			link.click();
			document.body.removeChild(link);
			URL.revokeObjectURL(url);
			return {
				fileName: rendered.fileName,
				mimeType: rendered.mimeType,
				bytes: rendered.bytes.byteLength,
			};
		},
	};

	const methods = Object.keys(handlers) as EditorApiMethodName[];

	return {
		version: EDITOR_API_VERSION,
		methods,
		dispatch: async (method, params) => {
			const handler = handlers[method] as
				| ((params: unknown) => unknown)
				| undefined;
			if (!handler) {
				throw new EditorApiError({
					code: "unknown_method",
					message: `Unknown Editor API method ${String(method)}`,
				});
			}
			if (params !== undefined && (typeof params !== "object" || params === null)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `${method} expects a params object`,
				});
			}
			return (await handler(params ?? {})) as EditorApiMethods[typeof method]["result"];
		},
	};
}
