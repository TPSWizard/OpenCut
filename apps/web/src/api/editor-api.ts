import {
	DEFAULT_TRANSITION_DURATION,
	getTransitionCatalogue,
	getTransitionInfo,
	isVisualElement,
	type ElementTransition,
} from "@/transitions";
import { setClipTransition } from "@/transitions/apply";
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
	TICKS_PER_SECOND,
	type MediaTime,
} from "@/wasm";
import { useProxyJobs } from "@/media/proxy-store";
import { chaptersText } from "@/timeline/bookmarks/chapters";
import { resolveStickerId, resolveStickerIntrinsicSize, searchStickers as searchStickerPacks } from "@/stickers";
import { buildStickerElement } from "@/timeline/element-utils";
import { breakApartCompound, compoundChildCount, createCompound, groupElements, ungroupElements } from "@/timeline/compound";
import { TracksEditCommand } from "@/commands/timeline/tracks-edit";
import { generateUUID } from "@/utils/id";
import { elementsInRange } from "@/timeline/edits";
import { useTimelineStore } from "@/timeline/timeline-store";
import { needsBackup, readBackups, readDurability, requestPersistence } from "@/services/storage/durability";
import { readStorageQuotaStatus } from "@/services/storage/quota";
import { storageService } from "@/services/storage/service";
import { isPreviewQuality } from "@/media/proxy";
import { usePreviewStore } from "@/preview/preview-store";
import { buildScene } from "@/services/renderer/scene-builder";
import { CanvasRenderer } from "@/services/renderer/canvas-renderer";
import { videoCache } from "@/services/video-cache/service";
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
	type ApiPreviewSettings,
	type ApiStorageStatus,
	type ApiMarker,
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
import { clearReframe, reframeClip } from "@/reframe/scan";
import { REFRAME_CAMERAS } from "@/reframe/path";
import { listCaptionTracks, translateCaptionTrack } from "@/translation/captions";
import { TRANSLATION_LANGUAGE_CODES } from "@/translation/languages";
import { KOKORO_VOICES, generateVoiceover } from "@/voiceover/generate";
import { cleanUpPicture, restoreOriginalPicture } from "@/picture/apply";
import { importStockItem, projectCredits, searchStock } from "@/stock/stock";
import { clearCutout, removeBackground, setCutoutInverted } from "@/cutout/remove";
import { analyzeClipBeats, beatTimes, bookmarkBeats } from "@/beats/analyze";
import { reduceNoiseOnElement } from "@/denoise/apply";
import { cutIntervals, findClipFillers } from "@/fillers/remove";
import { encodeLutParam, parseLutFile } from "@/luts/parse";
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
		...(element.groupId ? { groupId: element.groupId } : {}),
		...(element.type === "compound"
			? { compound: { clips: compoundChildCount(element), tracks: element.tracks.length } }
			: {}),
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
		transitionIn:
			"transitionIn" in element && element.transitionIn
				? {
						type: element.transitionIn.type,
						duration: seconds(element.transitionIn.duration),
						...(element.transitionIn.params && { params: element.transitionIn.params }),
					}
				: undefined,
		retime: "retime" in element && element.retime ? { ...element.retime } : undefined,
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
		...(track.locked ? { locked: true } : {}),
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
		...(asset.proxyFile && asset.proxyWidth && asset.proxyHeight
			? { proxy: { width: asset.proxyWidth, height: asset.proxyHeight } }
			: {}),
		...(asset.id in useProxyJobs.getState().making ? { proxyPending: true } : {}),
	};
}

async function storageStatus(): Promise<ApiStorageStatus> {
	const durability = await readDurability();
	const quota = await readStorageQuotaStatus();
	return {
		level: durability.level,
		persisted: durability.persisted,
		browser: durability.browser,
		standalone: durability.standalone,
		sevenDayRule: durability.sevenDayRule,
		reasons: durability.reasons,
		advice: durability.advice,
		usageBytes: quota.usageBytes,
		quotaBytes: quota.quotaBytes,
	};
}

function previewSettings(): ApiPreviewSettings {
	const state = usePreviewStore.getState();
	return { quality: state.previewQuality, useProxies: state.useProxies, autoProxies: state.autoProxies };
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

	/** Refuses edits to clips on a locked track (reads are fine). */
	const requireUnlocked = (track: TimelineTrack) => {
		if (track.locked) {
			throw new EditorApiError({
				code: "invalid_params",
				message: `Track "${track.name}" is locked; unlock it with timeline.setTrackLocked to edit its clips`,
			});
		}
	};

	const unlockedRefs = (locators: ApiElementLocator[]): ApiElementRef[] => {
		const refs = resolveRefs(locators);
		const tracks = allTracks(tracksOf());
		for (const ref of refs) {
			const track = tracks.find((candidate) => candidate.id === ref.trackId);
			if (track) requireUnlocked(track);
		}
		return refs;
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

	const listMarkers = (): ApiMarker[] => {
		requireProject();
		return [...editor.scenes.getActiveScene().bookmarks]
			.sort((a, b) => a.time - b.time)
			.map((bookmark) => ({
				time: mediaTimeToSeconds({ time: bookmark.time }),
				...(bookmark.note ? { note: bookmark.note } : {}),
				...(bookmark.color ? { color: bookmark.color } : {}),
				...(bookmark.duration ? { duration: mediaTimeToSeconds({ time: bookmark.duration }) } : {}),
			}));
	};

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
		"media.makeProxy": async ({ mediaId }) => {
			const project = requireProject();
			const asset = editor.media.getAssets().find((item) => item.id === mediaId);
			if (!asset) throw new EditorApiError({ code: "not_found", message: `No media ${mediaId}` });
			if (asset.type !== "video") {
				throw new EditorApiError({ code: "invalid_params", message: "Only videos get proxies" });
			}
			try {
				return toMediaInfo(await editor.media.makeProxy({ projectId: project.metadata.id, mediaId }));
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : String(error) });
			}
		},
		"media.removeProxy": async ({ mediaId }) => {
			const project = requireProject();
			const updated = await editor.media.removeProxy({ projectId: project.metadata.id, mediaId });
			if (!updated) throw new EditorApiError({ code: "not_found", message: `No media ${mediaId}` });
			return toMediaInfo(updated);
		},
		"timeline.setTrackLocked": ({ trackId, locked }) => {
			requireProject();
			const track = editor.timeline.getTrackById({ trackId });
			if (!track) throw new EditorApiError({ code: "not_found", message: `No track ${trackId}` });
			if (Boolean(track.locked) !== Boolean(locked)) {
				editor.timeline.setTrackLocked({ trackId, locked: Boolean(locked) });
			}
			const updated = editor.timeline.getTrackById({ trackId });
			if (!updated) throw new EditorApiError({ code: "not_found", message: `No track ${trackId}` });
			return toTrackInfo({ track: updated, tracks: editor.scenes.getActiveScene().tracks });
		},
		"selection.selectRange": ({ from, to }) => {
			requireProject();
			const { markIn, markOut } = useTimelineStore.getState();
			const start = from !== undefined ? mediaTimeFromSeconds({ seconds: Number(from) }) : (markIn ?? 0);
			const end =
				to !== undefined ? mediaTimeFromSeconds({ seconds: Number(to) }) : (markOut ?? editor.timeline.getTotalDuration());
			const refs = elementsInRange({ tracks: allTracks(editor.scenes.getActiveScene().tracks), from: start, to: end });
			editor.selection.setSelectedElements({ elements: refs });
			return editor.selection.getSelectedElements().map((ref) => ({ trackId: ref.trackId, elementId: ref.elementId }));
		},
		"timeline.group": ({ elements }) => {
			requireProject();
			const refs = unlockedRefs(elements);
			const tracks = groupElements({ tracks: tracksOf(), refs, groupId: generateUUID() });
			editor.command.execute({ command: new TracksEditCommand(tracks, refs) });
			return refs.map((ref) => elementInfoById(ref));
		},
		"timeline.ungroup": ({ elements }) => {
			requireProject();
			const refs = unlockedRefs(elements);
			let tracks: SceneTracks;
			try {
				tracks = ungroupElements({ tracks: tracksOf(), refs });
			} catch (error) {
				throw new EditorApiError({ code: "invalid_params", message: error instanceof Error ? error.message : String(error) });
			}
			editor.command.execute({ command: new TracksEditCommand(tracks, refs) });
			return refs.map((ref) => elementInfoById(ref));
		},
		"timeline.createCompound": ({ elements }) => {
			requireProject();
			const refs = unlockedRefs(elements);
			const result = createCompound({ tracks: tracksOf(), refs });
			editor.command.execute({ command: new TracksEditCommand(result.tracks, [result.ref]) });
			return elementInfoById(result.ref);
		},
		"timeline.breakApart": ({ elementId, trackId }) => {
			requireProject();
			const { track, element } = resolveElement({ elementId, trackId });
			requireUnlocked(track);
			if (element.type !== "compound") {
				throw new EditorApiError({ code: "invalid_params", message: `${element.name} is not a compound clip` });
			}
			const result = breakApartCompound({ tracks: tracksOf(), ref: { trackId: track.id, elementId: element.id } });
			editor.command.execute({ command: new TracksEditCommand(result.tracks, result.refs) });
			return result.refs.map((ref) => elementInfoById(ref));
		},
		"markers.list": () => listMarkers(),
		"markers.add": async ({ time, note, color, duration }) => {
			requireProject();
			const at = time !== undefined ? mediaTimeFromSeconds({ seconds: Number(time) }) : editor.playback.getCurrentTime();
			if (!editor.scenes.isBookmarked({ time: at })) await editor.scenes.toggleBookmark({ time: at });
			const existing = editor.scenes.getBookmarkAtTime({ time: at });
			const updates: Record<string, unknown> = {};
			if (note !== undefined) updates.note = note || undefined;
			if (color !== undefined) updates.color = color || undefined;
			if (duration !== undefined) updates.duration = duration ? mediaTimeFromSeconds({ seconds: Number(duration) }) : undefined;
			if (Object.keys(updates).length > 0) {
				await editor.scenes.updateBookmark({ time: (existing?.time ?? at) as MediaTime, updates });
			}
			return listMarkers();
		},
		"markers.update": async ({ time, note, color, duration }) => {
			requireProject();
			const at = mediaTimeFromSeconds({ seconds: Number(time) });
			if (!editor.scenes.isBookmarked({ time: at })) {
				throw new EditorApiError({ code: "not_found", message: `No marker at ${time} s` });
			}
			const updates: Record<string, unknown> = {};
			if (note !== undefined) updates.note = note || undefined;
			if (color !== undefined) updates.color = color || undefined;
			if (duration !== undefined) updates.duration = duration ? mediaTimeFromSeconds({ seconds: Number(duration) }) : undefined;
			await editor.scenes.updateBookmark({ time: at, updates });
			return listMarkers();
		},
		"markers.remove": async ({ time }) => {
			requireProject();
			const at = mediaTimeFromSeconds({ seconds: Number(time) });
			if (!editor.scenes.isBookmarked({ time: at })) {
				throw new EditorApiError({ code: "not_found", message: `No marker at ${time} s` });
			}
			await editor.scenes.removeBookmark({ time: at });
			return listMarkers();
		},
		"markers.chapters": () => {
			requireProject();
			return { text: chaptersText({ bookmarks: editor.scenes.getActiveScene().bookmarks, ticksPerSecond: TICKS_PER_SECOND }) };
		},
		"storage.status": () => storageStatus(),
		"storage.persist": async () => {
			await requestPersistence();
			return storageStatus();
		},
		"storage.usage": async () => {
			await editor.project.loadAllProjects();
			const backups = readBackups();
			return Promise.all(
				editor.project.getSavedProjects().map(async (project) => {
					const sizes = await storageService.measureProjectMedia({ projectId: project.id });
					const lastBackupAt = backups[project.id] ?? null;
					return {
						projectId: project.id,
						name: project.name,
						mediaBytes: sizes.mediaBytes,
						proxyBytes: sizes.proxyBytes,
						mediaCount: sizes.mediaCount,
						missing: sizes.missing,
						lastBackupAt: lastBackupAt ? new Date(lastBackupAt).toISOString() : null,
						needsBackup: needsBackup({ updatedAt: new Date(project.updatedAt).getTime(), lastBackupAt }),
					};
				}),
			);
		},
		"storage.clearProxies": async ({ projectId }) => {
			const id = projectId ?? requireProject().metadata.id;
			const freedBytes = await storageService.clearProjectProxies({ projectId: id });
			if (id === editor.project.getActive()?.metadata.id) {
				for (const asset of editor.media.getAssets().filter((item) => item.proxyFile)) {
					await editor.media.removeProxy({ projectId: id, mediaId: asset.id });
				}
			}
			return { freedBytes };
		},
		"media.missing": () => {
			requireProject();
			return editor.media.getMissingAssets().map((item) => ({ id: item.id, name: item.name, type: item.type, size: item.size }));
		},
		"media.relink": async ({ mediaId, file }) => {
			const project = requireProject();
			if (!(file instanceof Blob)) {
				throw new EditorApiError({ code: "invalid_params", message: "file must be a File" });
			}
			try {
				const named = file instanceof File ? file : new File([file], "relinked", { type: (file as Blob).type });
				return toMediaInfo(await editor.media.relinkMedia({ projectId: project.metadata.id, mediaId, file: named }));
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : String(error) });
			}
		},
		"preview.getSettings": () => previewSettings(),
		"preview.setSettings": ({ quality, useProxies, autoProxies }) => {
			const store = usePreviewStore.getState();
			if (quality !== undefined) {
				if (!isPreviewQuality(quality)) {
					throw new EditorApiError({ code: "invalid_params", message: "quality is auto, full, half or quarter" });
				}
				store.setPreviewQuality(quality);
			}
			if (useProxies !== undefined) store.setUseProxies(Boolean(useProxies));
			if (autoProxies !== undefined) store.setAutoProxies(Boolean(autoProxies));
			return previewSettings();
		},
		"preview.capture": async ({ time }) => {
			const project = requireProject();
			const { canvasSize, fps, background } = project.settings;
			const tree = buildScene({
				tracks: editor.scenes.getActiveScene().tracks,
				mediaAssets: editor.media.getAssets(),
				duration: editor.timeline.getTotalDuration(),
				canvasSize,
				background,
				isPreview: true,
				previewSource: { quality: usePreviewStore.getState().previewQuality, useProxies: usePreviewStore.getState().useProxies },
			});
			const at =
				time === undefined
					? editor.playback.getCurrentTime()
					: Math.round(Number(time) * TICKS_PER_SECOND);
			const renderer = new CanvasRenderer({ width: canvasSize.width, height: canvasSize.height, fps });
			const canvas = document.createElement("canvas");
			canvas.width = canvasSize.width;
			canvas.height = canvasSize.height;
			await renderer.renderToCanvas({ node: tree, time: at, targetCanvas: canvas });
			const context = canvas.getContext("2d");
			let centre: [number, number, number] = [0, 0, 0];
			if (context) {
				const box = Math.max(2, Math.round(Math.min(canvas.width, canvas.height) / 8));
				const data = context.getImageData(
					Math.round(canvas.width / 2 - box / 2),
					Math.round(canvas.height / 2 - box / 2),
					box,
					box,
				).data;
				const sum = [0, 0, 0];
				for (let index = 0; index < data.length; index += 4) {
					sum[0] += data[index];
					sum[1] += data[index + 1];
					sum[2] += data[index + 2];
				}
				const count = data.length / 4;
				centre = [Math.round(sum[0] / count), Math.round(sum[1] / count), Math.round(sum[2] / count)];
			}
			return { width: canvas.width, height: canvas.height, centre, decoders: videoCache.describeSinks() };
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
		"stickers.search": async ({ query, pack = "all", limit = 40 }) => {
			const category = ({ all: "all", emoji: "emoji", brands: "logos", icons: "icons", flags: "flags" } as const)[pack];
			if (!category) {
				throw new EditorApiError({ code: "invalid_params", message: "pack is all, emoji, brands, icons or flags" });
			}
			const result = await searchStickerPacks({ query: requireString(query, "query"), category, limit: Math.max(1, Math.min(200, Number(limit) || 40)) });
			return result.items.map((item) => {
				const license = item.metadata.license as { spdx?: string | null; title?: string } | null | undefined;
				return {
					id: item.id,
					name: item.name,
					pack: item.provider === "logos" ? "brands" : item.provider,
					...(license ? { license: license.spdx ?? license.title ?? undefined } : {}),
					...(item.metadata.requiresCredit ? { requiresCredit: true } : {}),
					...(typeof item.metadata.credit === "string" ? { credit: item.metadata.credit } : {}),
				};
			});
		},
		"timeline.addSticker": async ({ stickerId, at, duration, name }) => {
			requireProject();
			const id = requireString(stickerId, "stickerId");
			try {
				resolveStickerId({ stickerId: id });
			} catch (error) {
				throw new EditorApiError({ code: "invalid_params", message: error instanceof Error ? error.message : String(error) });
			}
			const { width, height } = await resolveStickerIntrinsicSize({ stickerId: id });
			const element = buildStickerElement({
				stickerId: id,
				name,
				startTime: at === undefined ? editor.playback.getCurrentTime() : ticks(at, "at"),
				intrinsicWidth: width,
				intrinsicHeight: height,
			});
			if (duration !== undefined) element.duration = ticks(duration, "duration");
			const before = elementIds(tracksOf());
			editor.timeline.insertElement({ element, placement: { mode: "auto" } });
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
			requireUnlocked(track);
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
		"timeline.setTransition": ({ elementId, trackId, transition, side }) => {
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
			if (side !== undefined && side !== "start" && side !== "end") {
				throw new EditorApiError({ code: "invalid_params", message: "side must be start or end" });
			}
			setClipTransition({ editor, track, element, side: side ?? "end", transition: next });
			const updated = resolveElement({ elementId: element.id, trackId: track.id });
			return toElementInfo({ element: updated.element, trackId: track.id });
		},
		"timeline.moveElement": ({ elementId, trackId, startTime }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			requireUnlocked(track);
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
				elements: unlockedRefs(elements),
				splitTime: ticks(time, "time"),
			}),
		"timeline.delete": ({ elements }) => {
			const refs = unlockedRefs(elements);
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
		"reframe.apply": async ({ elementId, trackId, camera, resetAtShots, faces, sampleFps }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "video") {
				throw new EditorApiError({ code: "invalid_params", message: "Auto-reframe needs a video clip" });
			}
			if (camera !== undefined && !REFRAME_CAMERAS.includes(camera)) {
				throw new EditorApiError({ code: "invalid_params", message: "camera must be steady, smooth or follow" });
			}
			if (sampleFps !== undefined && (typeof sampleFps !== "number" || !Number.isFinite(sampleFps) || sampleFps <= 0 || sampleFps > 30)) {
				throw new EditorApiError({ code: "invalid_params", message: "sampleFps must be a number from 0 to 30" });
			}
			const options = {
				...(camera !== undefined && { camera }),
				...(resetAtShots !== undefined && { resetAtShots: Boolean(resetAtShots) }),
				...(faces !== undefined && { faces: Boolean(faces) }),
				...(sampleFps !== undefined && { sampleFps }),
			};
			try {
				const result = await reframeClip({ editor, trackId: track.id, elementId: element.id, options });
				return {
					scale: result.plan.scale,
					positionX: result.plan.positionX,
					positionY: result.plan.positionY,
					keysX: result.plan.keysX.map(([time, value]) => ({ time, value })),
					keysY: result.plan.keysY.map(([time, value]) => ({ time, value })),
					shots: result.plan.shots,
					cuts: result.cuts,
					samples: result.samples.length,
					faceSamples: result.faceSamples,
					fallbackShots: result.fallbackShots,
					keyframes: result.keyframes,
				};
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Auto-reframe failed" });
			}
		},
		"stock.search": async ({ query, provider, kind, page }) => {
			if (typeof query !== "string" || !query.trim()) {
				throw new EditorApiError({ code: "invalid_params", message: "query must be non-empty text" });
			}
			if (provider !== undefined && provider !== "openverse" && provider !== "pixabay") {
				throw new EditorApiError({ code: "invalid_params", message: "provider must be openverse or pixabay" });
			}
			if (kind !== undefined && kind !== "image" && kind !== "video") {
				throw new EditorApiError({ code: "invalid_params", message: "kind must be image or video" });
			}
			try {
				return await searchStock({ query, provider: provider ?? "openverse", kind: kind ?? "image", page: page ?? 1 });
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Search failed" });
			}
		},
		"stock.import": async ({ item, addToTimeline }) => {
			requireProject();
			if (!item || typeof item !== "object" || typeof item.id !== "string" || (item.provider !== "openverse" && item.provider !== "pixabay")) {
				throw new EditorApiError({ code: "invalid_params", message: "item must be a result from stock.search" });
			}
			try {
				return await importStockItem({ editor, item, addToTimeline: addToTimeline ?? true });
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Import failed" });
			}
		},
		"stock.credits": () => ({ text: projectCredits({ editor }) }),
		"picture.cleanup": async ({ elementId, trackId, removeWhite, decontaminate, feather, tint, crop, trace }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "image") {
				throw new EditorApiError({ code: "invalid_params", message: "Picture cleanup needs an image clip" });
			}
			if (feather !== undefined && (typeof feather !== "number" || !Number.isInteger(feather) || feather < 0 || feather > 16)) {
				throw new EditorApiError({ code: "invalid_params", message: "feather must be a whole number from 0 to 16" });
			}
			if (tint !== undefined && tint !== null && !/^#[0-9a-fA-F]{6}$/.test(tint)) {
				throw new EditorApiError({ code: "invalid_params", message: "tint must be #rrggbb or null" });
			}
			const options = Object.fromEntries(
				Object.entries({ removeWhite, decontaminate, feather, tint, crop, trace }).filter(([, value]) => value !== undefined),
			);
			try {
				return await cleanUpPicture({ editor, trackId: track.id, elementId: element.id, options });
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Cleanup failed" });
			}
		},
		"picture.restore": ({ elementId, trackId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "image" || !restoreOriginalPicture({ editor, trackId: track.id, elementId: element.id })) {
				throw new EditorApiError({ code: "invalid_params", message: "The clip has no original picture to restore" });
			}
			const updated = resolveElement({ elementId: element.id, trackId: track.id });
			return toElementInfo({ element: updated.element, trackId: track.id });
		},
		"audio.textToSpeech": async ({ script, voice, speed, captions, at }) => {
			requireProject();
			if (typeof script !== "string" || !script.trim()) {
				throw new EditorApiError({ code: "invalid_params", message: "script must be non-empty text" });
			}
			if (voice !== undefined && !(KOKORO_VOICES as readonly string[]).includes(voice)) {
				throw new EditorApiError({ code: "invalid_params", message: `voice must be one of ${KOKORO_VOICES.join(", ")}` });
			}
			if (speed !== undefined && (typeof speed !== "number" || !(speed >= 0.5 && speed <= 2))) {
				throw new EditorApiError({ code: "invalid_params", message: "speed must be from 0.5 to 2" });
			}
			if (at !== undefined && (typeof at !== "number" || !Number.isFinite(at) || at < 0)) {
				throw new EditorApiError({ code: "invalid_params", message: "at must be seconds, 0 or more" });
			}
			try {
				return await generateVoiceover({ editor, script, voice, speed, captions: captions ?? true, at });
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Voice-over failed" });
			}
		},
		"captions.translate": async ({ trackId, from, to, engine }) => {
			for (const [key, value] of [["from", from], ["to", to]] as const) {
				if (typeof value !== "string" || !TRANSLATION_LANGUAGE_CODES.includes(value)) {
					throw new EditorApiError({
						code: "invalid_params",
						message: `${key} must be one of ${TRANSLATION_LANGUAGE_CODES.join(", ")}`,
					});
				}
			}
			if (engine !== undefined && !["auto", "browser", "opus"].includes(engine)) {
				throw new EditorApiError({ code: "invalid_params", message: "engine must be auto, browser or opus" });
			}
			const target = trackId ?? listCaptionTracks({ editor })[0]?.track.id;
			if (!target) throw new EditorApiError({ code: "invalid_params", message: "There are no captions to translate" });
			try {
				const result = await translateCaptionTrack({ editor, trackId: target, from, to, engine });
				return { trackId: result.trackId, captions: result.captions, engine: result.engine, models: result.models, translations: result.translations };
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Translation failed" });
			}
		},
		"cutout.removeBackground": async ({ elementId, trackId, sampleFps }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "video") {
				throw new EditorApiError({ code: "invalid_params", message: "Background removal needs a video clip" });
			}
			if (sampleFps !== undefined && (typeof sampleFps !== "number" || !Number.isFinite(sampleFps) || sampleFps <= 0 || sampleFps > 60)) {
				throw new EditorApiError({ code: "invalid_params", message: "sampleFps must be a number from 0 to 60" });
			}
			try {
				return await removeBackground({ editor, trackId: track.id, elementId: element.id, sampleFps });
			} catch (error) {
				throw new EditorApiError({ code: "failed", message: error instanceof Error ? error.message : "Background removal failed" });
			}
		},
		"cutout.setInverted": ({ elementId, trackId, inverted }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "video" || !element.cutout) {
				throw new EditorApiError({ code: "invalid_params", message: "The clip has no cut-out" });
			}
			setCutoutInverted({ editor, trackId: track.id, elementId: element.id, inverted: Boolean(inverted) });
			const updated = resolveElement({ elementId: element.id, trackId: track.id });
			return toElementInfo({ element: updated.element, trackId: track.id });
		},
		"cutout.clear": ({ elementId, trackId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "video") {
				throw new EditorApiError({ code: "invalid_params", message: "Background removal needs a video clip" });
			}
			clearCutout({ editor, trackId: track.id, elementId: element.id });
			const updated = resolveElement({ elementId: element.id, trackId: track.id });
			return toElementInfo({ element: updated.element, trackId: track.id });
		},
		"reframe.clear": ({ elementId, trackId }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "video") {
				throw new EditorApiError({ code: "invalid_params", message: "Auto-reframe needs a video clip" });
			}
			clearReframe({ editor, trackId: track.id, elementId: element.id });
			const updated = resolveElement({ elementId: element.id, trackId: track.id });
			return toElementInfo({ element: updated.element, trackId: track.id });
		},
		"timeline.setRetime": ({ elementId, trackId, rate, maintainPitch, reverse, ramp, clear }) => {
			const { track, element } = resolveElement({ elementId, trackId });
			if (element.type !== "video" && element.type !== "audio") {
				throw new EditorApiError({ code: "invalid_params", message: "Only video and audio clips can be retimed" });
			}
			if (clear) {
				editor.timeline.updateElementRetime({ trackId: track.id, elementId: element.id, retime: undefined });
				return elementInfoById({ trackId: track.id, elementId: element.id });
			}
			const current = element.retime ?? { rate: 1 };
			if (rate !== undefined && (typeof rate !== "number" || !Number.isFinite(rate) || rate <= 0)) {
				throw new EditorApiError({ code: "invalid_params", message: "rate must be a positive number" });
			}
			if (ramp !== undefined && (!Array.isArray(ramp) || ramp.some((key) => typeof key?.time !== "number" || typeof key?.rate !== "number" || key.rate <= 0))) {
				throw new EditorApiError({ code: "invalid_params", message: "ramp must be a list of { time, rate } with positive rates" });
			}
			const next = {
				rate: rate ?? current.rate ?? 1,
				...(maintainPitch !== undefined ? { maintainPitch: Boolean(maintainPitch) } : current.maintainPitch ? { maintainPitch: true } : {}),
				...(reverse !== undefined ? { reverse: Boolean(reverse) } : current.reverse ? { reverse: true } : {}),
				...(ramp !== undefined ? (ramp.length > 0 ? { ramp } : {}) : current.ramp?.length ? { ramp: current.ramp } : {}),
			};
			editor.timeline.updateElementRetime({ trackId: track.id, elementId: element.id, retime: next });
			return elementInfoById({ trackId: track.id, elementId: element.id });
		},
		"luts.parse": async ({ file }) => {
			if (!(file instanceof File)) {
				throw new EditorApiError({ code: "invalid_params", message: "file must be a File" });
			}
			try {
				const { name, table } = await parseLutFile({ file });
				const lut = encodeLutParam({ name, table });
				return { name, size: Math.min(table.size, 33), lut };
			} catch (error) {
				throw new EditorApiError({ code: "invalid_params", message: error instanceof Error ? error.message : "Not a LUT file" });
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
