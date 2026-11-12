import {
	DEFAULT_TRANSITION_DURATION,
	getTransitionCatalogue,
	getTransitionInfo,
	isVisualElement,
	type ElementTransition,
} from "@/transitions";
import { setElementTransition } from "@/transitions/apply";
import { generateCaptions, insertGeneratedCaptions } from "@/captions/generate";
import { isCaptionHighlight, type CaptionStyle } from "@/captions";
import { insertCaptionChunksAsTextTrack } from "@/subtitles/insert";
import { collectCaptionCues, isSubtitleFormat, subtitleFileName, writeSubtitles } from "@/subtitles/write";
import type { SubtitleCue } from "@/subtitles/types";
import { isTranscriptionModelId } from "@/transcription/models";
import type { TranscriptionLanguage } from "@/transcription/types";
import type { EditorCore } from "@/core";
import { processMediaAssets } from "@/media/processing";
import { PROJECT_BUNDLE_EXTENSION } from "@/project/bundle";
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
	type ApiCaption,
	type ApiCaptionStyle,
	type ApiCaptionWord,
	type ApiElementInfo,
	type ApiElementLocator,
	type ApiElementRef,
	type ApiEffectParamInfo,
	type ApiKeyframeInfo,
	type ApiMaskParamInfo,
	type ApiExportOptions,
	type ApiPlatformCheck,
	type ApiPlatformPreset,
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
import {
	PLATFORM_PRESETS,
	checkPlatformExport,
	estimatePlatformFileBytes,
	getPlatformPreset,
	platformEncodeSettings,
} from "@/platforms";
import { clearDucking, duckElement } from "@/ducking/duck";
import { measureElementLoudness, normalizeElementLoudness } from "@/loudness/measure";
import { LOUDNESS_TARGETS, type LoudnessReport } from "@/loudness/r128";
import { applyShotCuts, scanShots } from "@/shots/scan";
import { analyzeClipBeats, beatTimes, bookmarkBeats } from "@/beats/analyze";
import { reduceNoiseOnElement } from "@/denoise/apply";
import { cutIntervals, findClipFillers } from "@/fillers/remove";
import { cutSilence, detectSilence } from "@/silence/cut";
import { effectsRegistry } from "@/effects";
import { buildDefaultMaskInstance, masksRegistry } from "@/masks";
import { resolveAnimationTarget } from "@/timeline/animation-targets";
import { isLeafChannelData } from "@/animation/channel-data";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import type { Mask, MaskType } from "@/masks/types";
import { fetchFontsourceCatalogue, loadFontsourceFont } from "@/fonts/fontsource";
import { isRippleScope } from "@/silence/detect";

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

function loudnessInfo(report: LoudnessReport) {
	const finite = (value: number) => (Number.isFinite(value) ? value : null);
	return {
		integratedLufs: finite(report.integratedLufs),
		loudnessRangeLu: report.loudnessRangeLu,
		truePeakDbtp: finite(report.truePeakDbtp),
		maxMomentaryLufs: finite(report.maxMomentaryLufs),
		maxShortTermLufs: finite(report.maxShortTermLufs),
		seconds: report.seconds,
		sampleRate: report.sampleRate,
		channels: report.channels,
	};
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

/** The element kinds that carry a mask (video, image and graphic). */
function maskable(element: TimelineElement): element is TimelineElement & { masks?: Mask[] } {
	return element.type === "video" || element.type === "image" || element.type === "graphic";
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
		transition:
			"transition" in element && element.transition
				? {
						type: element.transition.type,
						duration: seconds(element.transition.duration),
						...(element.transition.params && { params: element.transition.params }),
					}
				: undefined,
		masks:
			maskable(element) && element.masks && element.masks.length > 0
				? element.masks.map((mask) => ({
						id: mask.id,
						type: mask.type,
						params: { ...(mask.params as unknown as Record<string, unknown>) },
					}))
				: undefined,
		effects:
			"effects" in element && element.effects && element.effects.length > 0
				? element.effects.map((effect) => ({
						id: effect.id,
						type: effect.type,
						params: { ...effect.params },
						enabled: effect.enabled,
					}))
				: undefined,
		words:
			"words" in element && element.words && element.words.length > 0
				? element.words.map((word) => ({
						text: word.text,
						start: seconds(word.start),
						end: seconds(word.end),
					}))
				: undefined,
	};
}

/** Validates one `captions.insert` caption into a subtitle cue (seconds). */
function toSubtitleCue(raw: unknown, index: number): SubtitleCue {
	const invalid = (message: string): never => {
		throw new EditorApiError({
			code: "invalid_params",
			message: `captions[${index}]: ${message}`,
		});
	};
	if (!raw || typeof raw !== "object") invalid("must be an object");
	const caption = raw as ApiCaption;
	if (typeof caption.text !== "string" || !caption.text.trim()) invalid("text is required");
	const number = (value: unknown, field: string): number => {
		if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
			invalid(`${field} must be a non-negative number of seconds`);
		}
		return value as number;
	};
	const startTime = number(caption.startTime, "startTime");
	const duration = number(caption.duration, "duration");
	if (duration <= 0) invalid("duration must be positive");
	let words: ApiCaptionWord[] | undefined;
	if (caption.words !== undefined) {
		if (!Array.isArray(caption.words)) invalid("words must be an array");
		words = caption.words.map((word, wordIndex) => {
			if (!word || typeof word !== "object" || typeof word.text !== "string") {
				invalid(`words[${wordIndex}] must be { text, start, end }`);
			}
			const start = number(word.start, `words[${wordIndex}].start`);
			const end = number(word.end, `words[${wordIndex}].end`);
			if (end < start) invalid(`words[${wordIndex}] ends before it starts`);
			return { text: word.text, start, end };
		});
		const spelled = words.map((word) => word.text.trim()).filter(Boolean).join(" ");
		const content = caption.text.trim().split(/\s+/).join(" ");
		if (spelled !== content) {
			invalid("words joined by spaces must spell text");
		}
	}
	return {
		text: caption.text,
		startTime,
		duration,
		...(words && { words }),
		...(caption.style && typeof caption.style === "object"
			? { style: caption.style as SubtitleCue["style"] }
			: {}),
	};
}

function toCaptionStyle(raw: unknown): Partial<CaptionStyle> | undefined {
	if (raw === undefined) return undefined;
	if (!raw || typeof raw !== "object") {
		throw new EditorApiError({
			code: "invalid_params",
			message: "style must be an object",
		});
	}
	const style = raw as ApiCaptionStyle;
	if (style.highlight !== undefined && !isCaptionHighlight(style.highlight)) {
		throw new EditorApiError({
			code: "invalid_params",
			message: "style.highlight must be none, word, box or fill",
		});
	}
	for (const key of ["activeColor", "boxColor"] as const) {
		if (style[key] !== undefined && typeof style[key] !== "string") {
			throw new EditorApiError({
				code: "invalid_params",
				message: `style.${key} must be a colour string`,
			});
		}
	}
	return {
		...(style.highlight !== undefined && { highlight: style.highlight }),
		...(style.activeColor !== undefined && { activeColor: style.activeColor }),
		...(style.boxColor !== undefined && { boxColor: style.boxColor }),
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
/** Hands a blob to the browser as a download. */
function downloadBlob({ blob, fileName }: { blob: Blob; fileName: string }) {
	const url = URL.createObjectURL(blob);
	const link = document.createElement("a");
	link.href = url;
	link.download = fileName;
	document.body.appendChild(link);
	link.click();
	document.body.removeChild(link);
	URL.revokeObjectURL(url);
}

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

	const requirePlatformPreset = (id: unknown) => {
		const preset = getPlatformPreset({
			id: typeof id === "string" ? id : null,
		});
		if (!preset) {
			throw new EditorApiError({
				code: "invalid_params",
				message: `Unknown platform preset ${String(id)}; see export.presets`,
			});
		}
		return preset;
	};

	const platformProjectState = () => {
		const project = requireProject();
		return {
			canvas: project.settings.canvasSize,
			fps: frameRateToFloat(project.settings.fps),
			durationSeconds: mediaTimeToSeconds({
				time: editor.timeline.getTotalDuration(),
			}),
		};
	};

	const platformPresetInfo = (
		preset: (typeof PLATFORM_PRESETS)[number],
	): ApiPlatformPreset => {
		const { canvas, fps } = platformProjectState();
		return {
			id: preset.id,
			label: preset.label,
			canvas: { ...preset.canvas },
			fps: preset.fps,
			maxDurationSeconds: preset.maxDurationSeconds,
			maxFileBytes: preset.maxFileBytes,
			safeArea: preset.safeArea ? { ...preset.safeArea } : null,
			verified: preset.verified,
			source: preset.source,
			encode: platformEncodeSettings({ canvas, fps }),
		};
	};

	const checkPlatform = ({
		preset: presetId,
		includeAudio,
	}: {
		preset: unknown;
		includeAudio?: boolean;
	}): ApiPlatformCheck => {
		const preset = requirePlatformPreset(presetId);
		const { canvas, fps, durationSeconds } = platformProjectState();
		const audio = includeAudio ?? true;
		return {
			preset: preset.id,
			issues: checkPlatformExport({
				preset,
				canvas,
				fps,
				durationSeconds,
				includeAudio: audio,
			}).map((issue) => ({
				kind: issue.kind,
				severity: issue.severity,
				message: issue.message,
				...(issue.fix ? { fix: issue.fix } : {}),
			})),
			estimatedBytes: estimatePlatformFileBytes({
				canvas,
				fps,
				durationSeconds,
				includeAudio: audio,
			}),
		};
	};

	const silenceOptions = (params: {
		thresholdDb?: unknown;
		minSilenceSeconds?: unknown;
		paddingSeconds?: unknown;
	}) => {
		const options: { thresholdDb?: number; minSilenceSeconds?: number; paddingSeconds?: number } = {};
		for (const key of ["thresholdDb", "minSilenceSeconds", "paddingSeconds"] as const) {
			const value = params[key];
			if (value === undefined) continue;
			if (typeof value !== "number" || !Number.isFinite(value)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `${key} must be a number`,
				});
			}
			options[key] = value;
		}
		return options;
	};

	const renderExport = async (options: ApiExportOptions) => {
		const project = requireProject();
		const preset =
			options.preset !== undefined ? requirePlatformPreset(options.preset) : null;
		const format = preset ? "mp4" : (options.format ?? "mp4");
		const quality = options.quality ?? "high";
		if (format !== "mp4" && format !== "webm") {
			throw new EditorApiError({
				code: "invalid_params",
				message: `Unknown export format ${String(format)}`,
			});
		}
		const exportFps = options.fps
			? floatToFrameRate(options.fps)
			: project.settings.fps;
		const result = await editor.project.export({
			options: {
				format,
				quality,
				includeAudio: options.includeAudio ?? true,
				fps: exportFps,
				platform: preset
					? platformEncodeSettings({
							canvas: project.settings.canvasSize,
							fps: frameRateToFloat(exportFps),
						})
					: undefined,
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
		"project.exportBundle": async ({ fileName }) => {
			const bundle = await editor.project.exportProjectBundle();
			const requested =
				typeof fileName === "string" && fileName.trim().length > 0
					? `${fileName.trim()}${PROJECT_BUNDLE_EXTENSION}`
					: bundle.fileName;
			downloadBlob({ blob: bundle.blob, fileName: requested });
			return { fileName: requested, bytes: bundle.blob.size };
		},
		"project.importBundle": async ({ file }) => {
			if (!(file instanceof Blob)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "file must be a File",
				});
			}
			const id = await editor.project.importProjectBundle({ file });
			const metadata = editor.project
				.getSavedProjects()
				.find((item) => item.id === id);
			return {
				id,
				name: metadata?.name ?? "Imported project",
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
		"captions.insert": ({ captions, style }) => {
			requireProject();
			if (!Array.isArray(captions) || captions.length === 0) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "captions must be a non-empty array",
				});
			}
			const cues = captions.map(toSubtitleCue);
			const captionStyle = toCaptionStyle(style);
			const before = elementIds(tracksOf());
			const trackId = insertCaptionChunksAsTextTrack({
				editor,
				captions: cues,
				captionStyle: captionStyle ?? { highlight: "word" },
			});
			if (!trackId) {
				throw new EditorApiError({
					code: "failed",
					message: "The captions could not be inserted",
				});
			}
			const elements: ApiElementRef[] = [];
			for (const track of allTracks(tracksOf())) {
				for (const element of track.elements as TimelineElement[]) {
					if (!before.has(element.id)) {
						elements.push({ trackId: track.id, elementId: element.id });
					}
				}
			}
			return { trackId, elements };
		},
		"captions.export": ({ format, trackId }) => {
			const project = requireProject();
			if (!isSubtitleFormat(format)) {
				throw new EditorApiError({ code: "invalid_params", message: "format must be srt, vtt or ass" });
			}
			if (trackId !== undefined) {
				const track = allTracks(tracksOf()).find((candidate) => candidate.id === trackId);
				if (!track) {
					throw new EditorApiError({ code: "not_found", message: `No track ${trackId}` });
				}
			}
			const cues = collectCaptionCues({ tracks: allTracks(tracksOf()) as TimelineTrack[], trackId });
			const content = writeSubtitles({
				format,
				cues,
				canvasSize: project.settings.canvasSize,
				title: project.metadata.name,
			});
			return { format, fileName: subtitleFileName({ name: project.metadata.name, format }), cueCount: cues.length, content };
		},
		"captions.transcribe": async ({ language, modelId, wordsPerCaption, insert, style }) => {
			requireProject();
			if (modelId !== undefined && !isTranscriptionModelId(modelId)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "modelId must be one of whisper-tiny, whisper-base, whisper-small, whisper-medium, whisper-large-v3-turbo",
				});
			}
			if (
				wordsPerCaption !== undefined &&
				(typeof wordsPerCaption !== "number" || !Number.isInteger(wordsPerCaption) || wordsPerCaption < 1)
			) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "wordsPerCaption must be a positive integer",
				});
			}
			const captionStyle = toCaptionStyle(style);
			const { result, captions } = await generateCaptions({
				editor,
				language: (language ?? "auto") as TranscriptionLanguage,
				modelId,
				wordsPerCaption,
			});
			const trackId =
				insert === false
					? null
					: insertGeneratedCaptions({ editor, captions, style: captionStyle });
			return {
				text: result.text,
				words: result.words,
				captions: captions.map((caption) => ({
					text: caption.text,
					startTime: caption.startTime,
					duration: caption.duration,
					...(caption.words && { words: caption.words }),
				})),
				trackId,
			};
		},
		"transitions.list": () => getTransitionCatalogue(),
		"timeline.setTransition": ({ elementId, trackId, transition }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (!isVisualElement(element)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "only video, image, text, sticker and graphic elements take a transition",
				});
			}
			let next: ElementTransition | null = null;
			if (transition !== null) {
				if (!transition || typeof transition !== "object" || typeof transition.type !== "string") {
					throw new EditorApiError({
						code: "invalid_params",
						message: "transition must be { type, duration?, params? } or null",
					});
				}
				if (!getTransitionInfo({ name: transition.type })) {
					throw new EditorApiError({
						code: "invalid_params",
						message: `unknown transition "${transition.type}"; see transitions.list`,
					});
				}
				const duration =
					transition.duration === undefined
						? DEFAULT_TRANSITION_DURATION
						: ticks(transition.duration, "duration");
				if (duration <= 0) {
					throw new EditorApiError({
						code: "invalid_params",
						message: "duration must be positive",
					});
				}
				next = {
					type: transition.type,
					duration,
					...(transition.params && { params: transition.params }),
				};
			}
			setElementTransition({
				editor,
				trackId: track.id,
				elementId: element.id,
				transition: next,
			});
			const updated = resolveElement({ elementId: element.id, trackId: track.id });
			return toElementInfo({ element: updated.element, trackId: track.id });
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
		"shots.detect": async ({ elementId, trackId, threshold, minGapSeconds, sampleFps, apply }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const options: Record<string, number> = {};
			for (const [key, value] of [["threshold", threshold], ["minGapSeconds", minGapSeconds], ["sampleFps", sampleFps]] as const) {
				if (value === undefined) continue;
				if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
					throw new EditorApiError({ code: "invalid_params", message: `${key} must be a positive number` });
				}
				options[key] = value;
			}
			if (apply !== undefined && !["none", "bookmarks", "split"].includes(apply)) {
				throw new EditorApiError({ code: "invalid_params", message: "apply must be none, bookmarks or split" });
			}
			try {
				const scan = await scanShots({ editor, trackId: track.id, elementId: element.id, options });
				const applied = applyShotCuts({ editor, trackId: track.id, elementId: element.id, cuts: scan.cuts, apply: apply ?? "none" });
				return {
					cuts: scan.cuts.map((cut) => ({
						sourceSeconds: cut.seconds,
						timelineSeconds: cut.timelineTicks === null ? null : seconds(cut.timelineTicks as MediaTime),
						score: cut.score,
					})),
					framesScored: scan.framesScored,
					seconds: scan.seconds,
					applied,
				};
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Shot detection failed" });
			}
		},
		"audio.removeFillers": async ({ elementId, trackId, language, source, ripple, apply, modelId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (source !== undefined && !["captions", "transcribe", "auto"].includes(source)) {
				throw new EditorApiError({ code: "invalid_params", message: "source must be captions, transcribe or auto" });
			}
			if (ripple !== undefined && !isRippleScope(ripple)) {
				throw new EditorApiError({ code: "invalid_params", message: "ripple must be all, track or none" });
			}
			if (modelId !== undefined && !isTranscriptionModelId(modelId)) {
				throw new EditorApiError({ code: "invalid_params", message: "modelId is not a transcription model" });
			}
			try {
				const report = await findClipFillers({
					editor,
					trackId: track.id,
					elementId: element.id,
					language: language ?? "auto",
					source: source ?? "auto",
					modelId,
				});
				const applied = apply !== false && report.intervals.length > 0;
				const pieces = applied
					? cutIntervals({ editor, trackId: track.id, elementId: element.id, intervals: report.intervals, ripple: ripple ?? "all" })
					: [{ trackId: track.id, elementId: element.id }];
				return {
					source: report.source,
					language: report.language,
					words: report.words,
					fillers: report.fillers.map((hit) => ({ text: hit.text, start: hit.start, end: hit.end })),
					intervals: report.intervals,
					seconds: report.seconds,
					applied,
					pieces,
				};
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Filler removal failed" });
			}
		},
		"audio.reduceNoise": async ({ elementId, trackId, reductionDb }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (reductionDb !== undefined && (typeof reductionDb !== "number" || !Number.isFinite(reductionDb) || reductionDb < 0 || reductionDb > 60)) {
				throw new EditorApiError({ code: "invalid_params", message: "reductionDb must be between 0 and 60" });
			}
			try {
				return await reduceNoiseOnElement({ editor, trackId: track.id, elementId: element.id, options: { reductionDb } });
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Noise reduction failed" });
			}
		},
		"audio.detectBeats": async ({ elementId, trackId, every, apply }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (every !== undefined && (typeof every !== "number" || !Number.isInteger(every) || every < 1)) {
				throw new EditorApiError({ code: "invalid_params", message: "every must be a positive integer" });
			}
			if (apply !== undefined && apply !== "none" && apply !== "bookmarks") {
				throw new EditorApiError({ code: "invalid_params", message: "apply must be none or bookmarks" });
			}
			try {
				const analysis = await analyzeClipBeats({ editor, trackId: track.id, elementId: element.id });
				const times = beatTimes({ markers: analysis.markers, every: every ?? 1 });
				const applied = apply === "bookmarks" ? bookmarkBeats({ editor, times }) : 0;
				return {
					bpm: analysis.bpm,
					confidence: analysis.confidence,
					beats: analysis.markers.map((marker) => ({
						sourceSeconds: marker.sourceSeconds,
						timelineSeconds: marker.timelineTicks === null ? null : seconds(marker.timelineTicks as MediaTime),
					})),
					applied,
				};
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Beat detection failed" });
			}
		},
		"audio.loudnessTargets": () => LOUDNESS_TARGETS.map((target) => ({ ...target })),
		"audio.measureLoudness": async ({ elementId, trackId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			try {
				return loudnessInfo(await measureElementLoudness({ editor, trackId: track.id, elementId: element.id }));
			} catch (error) {
				throw new EditorApiError({
					code: "failed",
					message: error instanceof Error ? error.message : "Measurement failed",
				});
			}
		},
		"audio.normalizeLoudness": async ({ elementId, trackId, targetLufs, truePeakDbtp, target }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const preset = target !== undefined ? LOUDNESS_TARGETS.find((candidate) => candidate.id === target) : undefined;
			if (target !== undefined && !preset) {
				throw new EditorApiError({ code: "invalid_params", message: `Unknown loudness target ${target}; see audio.loudnessTargets` });
			}
			for (const [key, value] of [["targetLufs", targetLufs], ["truePeakDbtp", truePeakDbtp]] as const) {
				if (value !== undefined && (typeof value !== "number" || !Number.isFinite(value))) {
					throw new EditorApiError({ code: "invalid_params", message: `${key} must be a number` });
				}
			}
			try {
				const result = await normalizeElementLoudness({
					editor,
					trackId: track.id,
					elementId: element.id,
					targetLufs: targetLufs ?? preset?.lufs,
					truePeakDbtp: truePeakDbtp ?? preset?.truePeakDbtp,
				});
				return {
					element: elementInfoById({ trackId: track.id, elementId: element.id }),
					report: loudnessInfo(result.report),
					gainDb: result.gainDb,
					volumeDb: result.volumeDb,
					peakLimited: result.peakLimited,
					animatedVolume: result.animatedVolume,
				};
			} catch (error) {
				throw new EditorApiError({
					code: "failed",
					message: error instanceof Error ? error.message : "Normalising failed",
				});
			}
		},
		"audio.duck": async (params) => {
			const { track, element: music } = resolveElement(params.element);
			const voice = params.voice?.map((ref) => resolveElement(ref).element.id);
			const numbers: Record<string, number> = {};
			for (const key of [
				"amountDb",
				"thresholdDb",
				"attackSeconds",
				"releaseSeconds",
				"holdSeconds",
			] as const) {
				const value = params[key];
				if (value === undefined) continue;
				if (typeof value !== "number" || !Number.isFinite(value)) {
					throw new EditorApiError({
						code: "invalid_params",
						message: `${key} must be a number`,
					});
				}
				numbers[key] = value;
			}
			let result: Awaited<ReturnType<typeof duckElement>>;
			try {
				result = await duckElement({
					editor,
					trackId: track.id,
					elementId: music.id,
					voiceElementIds: voice,
					options: numbers,
				});
			} catch (error) {
				throw new EditorApiError({
					code: "failed",
					message: error instanceof Error ? error.message : "Ducking failed",
				});
			}
			return {
				element: elementInfoById({ trackId: track.id, elementId: music.id }),
				keyframes: result.keyframes,
				intervals: result.intervals,
				speechSeconds: result.speechSeconds,
				voiceElementIds: result.voiceElementIds,
			};
		},
		"fonts.list": async ({ query, limit }) => {
			const catalogue = await fetchFontsourceCatalogue();
			const needle = typeof query === "string" ? query.trim().toLowerCase() : "";
			const cap = typeof limit === "number" && limit > 0 ? Math.floor(limit) : 200;
			return catalogue
				.filter((entry) => !needle || entry.family.toLowerCase().includes(needle))
				.slice(0, cap)
				.map((entry) => ({
					id: entry.id,
					family: entry.family,
					category: entry.category,
					license: entry.license,
					weights: [...entry.weights],
					styles: [...entry.styles],
					variable: entry.variable,
				}));
		},
		"fonts.load": async ({ family }) => {
			const name = requireString(family, "family");
			try {
				const loaded = await loadFontsourceFont({ family: name });
				return { family: name, license: loaded.license, faces: loaded.faces.length };
			} catch (error) {
				throw new EditorApiError({
					code: "not_found",
					message: error instanceof Error ? error.message : `Could not load ${name}`,
				});
			}
		},
		"timeline.getKeyframes": ({ elementId, trackId }) => {
			const { element } = resolveElement({ elementId, trackId });
			const result: Record<string, ApiKeyframeInfo[]> = {};
			const describe = (keys: Array<{ id: string; time: number; value: unknown; segmentToNext?: string }>) =>
				keys.map((key) => ({
					id: key.id,
					time: seconds(key.time as MediaTime),
					value: key.value as number | string | boolean,
					...(key.segmentToNext
						? { interpolation: (key.segmentToNext === "step" ? "hold" : key.segmentToNext) as "linear" | "hold" | "bezier" }
						: {}),
				}));
			for (const [path, data] of Object.entries(element.animations ?? {})) {
				if (!data) continue;
				if (isLeafChannelData(data)) {
					result[path] = describe(data.keys as never);
				} else {
					for (const [component, channel] of Object.entries(data)) {
						if (channel) result[`${path}.${component}`] = describe(channel.keys as never);
					}
				}
			}
			return result;
		},
		"timeline.setKeyframe": ({ elementId, trackId, path, time, value, interpolation }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const propertyPath = requireString(path, "path");
			if (!resolveAnimationTarget({ element, path: propertyPath })) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `${propertyPath} cannot be keyframed on this element`,
				});
			}
			if (typeof time !== "number" || !Number.isFinite(time) || time < 0) {
				throw new EditorApiError({ code: "invalid_params", message: "time must be seconds from the element's start" });
			}
			if (interpolation !== undefined && !["linear", "hold", "bezier"].includes(interpolation)) {
				throw new EditorApiError({ code: "invalid_params", message: "interpolation must be linear, hold or bezier" });
			}
			if (typeof value !== "number" && typeof value !== "string" && typeof value !== "boolean") {
				throw new EditorApiError({ code: "invalid_params", message: "value must be a number, string or boolean" });
			}
			editor.timeline.upsertKeyframes({
				keyframes: [
					{
						trackId: track.id,
						elementId: element.id,
						propertyPath,
						time: ticks(time, "time"),
						value,
						interpolation,
					},
				],
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.removeKeyframe": ({ elementId, trackId, path, time, keyframeId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const propertyPath = requireString(path, "path");
			const data = element.animations?.[propertyPath];
			const channels = !data ? [] : isLeafChannelData(data) ? [data] : Object.values(data).filter(Boolean);
			const wanted = typeof time === "number" ? ticks(time, "time") : null;
			const ids = new Set<string>();
			for (const channel of channels) {
				for (const key of (channel as { keys: Array<{ id: string; time: number }> }).keys) {
					if ((keyframeId !== undefined && key.id === keyframeId) || (wanted !== null && key.time === wanted)) {
						ids.add(key.id);
					}
				}
			}
			if (ids.size === 0) {
				throw new EditorApiError({ code: "not_found", message: `No key on ${propertyPath} at that time` });
			}
			editor.timeline.removeKeyframes({
				keyframes: [...ids].map((id) => ({ trackId: track.id, elementId: element.id, propertyPath, keyframeId: id })),
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.clearKeyframes": ({ elementId, trackId, path }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const propertyPath = requireString(path, "path");
			editor.command.execute({
				command: new SetChannelCommand({ trackId: track.id, elementId: element.id, propertyPath, channel: undefined }),
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"masks.list": () =>
			masksRegistry.getAll().map((definition) => ({
				type: definition.type,
				name: definition.name,
				params: definition.params.map((param) => {
					const info: ApiMaskParamInfo = {
						key: param.key,
						label: param.label,
						type: param.type,
						default: param.default,
					};
					if (param.type === "number") {
						info.min = param.min;
						info.max = param.max;
						info.step = param.step;
					}
					if (param.type === "select") {
						info.options = param.options.map((option) => ({ ...option }));
					}
					return info;
				}),
			})),
		"timeline.setMask": ({ elementId, trackId, type, params }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (!maskable(element)) {
				throw new EditorApiError({ code: "invalid_params", message: "Masks apply to video, image and graphic elements only" });
			}
			const maskType = requireString(type, "type");
			if (!masksRegistry.has(maskType as MaskType)) {
				throw new EditorApiError({ code: "invalid_params", message: `Unknown mask type ${maskType}; see masks.list` });
			}
			const mask = buildDefaultMaskInstance({ maskType: maskType as MaskType });
			const merged = {
				...mask,
				params: { ...mask.params, ...(params && typeof params === "object" ? params : {}) },
			} as Mask;
			editor.timeline.updateElements({
				updates: [{ trackId: track.id, elementId: element.id, patch: { masks: [merged] } as Partial<TimelineElement> }],
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.updateMask": ({ elementId, trackId, params, inverted }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const mask = maskable(element) ? element.masks?.[0] : undefined;
			if (!mask) {
				throw new EditorApiError({ code: "not_found", message: "The element has no mask" });
			}
			const merged = {
				...mask,
				params: {
					...mask.params,
					...(params && typeof params === "object" ? params : {}),
					...(typeof inverted === "boolean" ? { inverted } : {}),
				},
			} as Mask;
			editor.timeline.updateElements({
				updates: [{ trackId: track.id, elementId: element.id, patch: { masks: [merged] } as Partial<TimelineElement> }],
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.removeMask": ({ elementId, trackId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const mask = maskable(element) ? element.masks?.[0] : undefined;
			if (mask) {
				editor.timeline.removeMask({ trackId: track.id, elementId: element.id, maskId: mask.id });
			}
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"effects.list": () =>
			effectsRegistry.getAll().map((definition) => {
				const params: ApiEffectParamInfo[] = [];
				for (const param of definition.params) {
					if (param.type === "number") {
						params.push({
							key: param.key,
							label: param.label,
							type: "number",
							default: param.default,
							min: param.min,
							max: param.max,
							step: param.step,
						});
					} else if (param.type === "select") {
						params.push({
							key: param.key,
							label: param.label,
							type: "select",
							default: param.default,
							options: param.options.map((option) => ({ ...option })),
						});
					}
				}
				return {
					type: definition.type,
					name: definition.name,
					keywords: [...definition.keywords],
					params,
				};
			}),
		"timeline.addEffect": ({ elementId, trackId, type, params }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (typeof type !== "string" || !effectsRegistry.has(type)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: `Unknown effect type ${String(type)}; see effects.list`,
				});
			}
			const effectId = editor.timeline.addClipEffect({
				trackId: track.id,
				elementId: element.id,
				effectType: type,
			});
			if (!effectId) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "Effects apply to visual elements only",
				});
			}
			if (params && typeof params === "object" && Object.keys(params).length > 0) {
				editor.timeline.updateClipEffectParams({
					trackId: track.id,
					elementId: element.id,
					effectId,
					params,
				});
			}
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.updateEffect": ({ elementId, trackId, effectId, params, enabled }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			const effect =
				"effects" in element
					? element.effects?.find((candidate) => candidate.id === effectId)
					: undefined;
			if (!effect) {
				throw new EditorApiError({
					code: "not_found",
					message: `No effect ${String(effectId)} on ${element.id}`,
				});
			}
			if (params && typeof params === "object" && Object.keys(params).length > 0) {
				editor.timeline.updateClipEffectParams({
					trackId: track.id,
					elementId: element.id,
					effectId: effect.id,
					params,
				});
			}
			if (typeof enabled === "boolean" && enabled !== effect.enabled) {
				editor.timeline.toggleClipEffect({
					trackId: track.id,
					elementId: element.id,
					effectId: effect.id,
				});
			}
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"timeline.removeEffect": ({ elementId, trackId, effectId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			editor.timeline.removeClipEffect({
				trackId: track.id,
				elementId: element.id,
				effectId: String(effectId),
			});
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"audio.detectSilence": async (params) => {
			const { track, element } = resolveElement(params.element);
			try {
				return await detectSilence({
					editor,
					trackId: track.id,
					elementId: element.id,
					options: silenceOptions(params),
				});
			} catch (error) {
				throw new EditorApiError({
					code: "failed",
					message: error instanceof Error ? error.message : "Could not measure the clip",
				});
			}
		},
		"audio.cutSilence": async (params) => {
			const { track, element } = resolveElement(params.element);
			if (params.ripple !== undefined && !isRippleScope(params.ripple)) {
				throw new EditorApiError({
					code: "invalid_params",
					message: "ripple must be all, track or none",
				});
			}
			try {
				return await cutSilence({
					editor,
					trackId: track.id,
					elementId: element.id,
					ripple: params.ripple ?? "all",
					options: silenceOptions(params),
				});
			} catch (error) {
				throw new EditorApiError({
					code: "failed",
					message: error instanceof Error ? error.message : "Could not cut the clip",
				});
			}
		},
		"audio.clearDucking": ({ element }) => {
			const { track, element: music } = resolveElement(element);
			clearDucking({ editor, trackId: track.id, elementId: music.id });
			return elementInfoById({ trackId: track.id, elementId: music.id });
		},
		"export.presets": () => PLATFORM_PRESETS.map(platformPresetInfo),
		"export.check": (params) => checkPlatform(params),
		"export.download": async (options) => {
			const rendered = await renderExport(options);
			const blob = new Blob([rendered.bytes], { type: rendered.mimeType });
			downloadBlob({ blob, fileName: rendered.fileName });
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
