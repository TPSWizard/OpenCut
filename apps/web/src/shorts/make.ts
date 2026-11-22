/**
 * Long video to shorts in the web editor: the open project's transcript
 * (its word-timed captions) as lines, the moments picked by Claude through
 * the API worker or locally, and each moment saved as its own 9:16
 * project with its media, captions and a centred cover crop. The project
 * that is open stays open and unchanged.
 */

import type { EditorCore } from "@/core";
import { wordsFromCaptions } from "@/fillers/remove";
import { apiUrl } from "@/lib/api";
import type { TProject } from "@/project/types";
import { getProjectDurationFromScenes } from "@/timeline/scenes";
import { buildSubtitleTextElement } from "@/subtitles/build-subtitle-text-element";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { generateUUID } from "@/utils/id";
import { TICKS_PER_SECOND } from "@/wasm";
import { coverCanvas, cropTracks } from "./crop";
import { localPicks, transcriptLines, type ShortPick, type ShortsOptions, type TimedLine } from "./lines";

export const SHORT_CANVAS = { width: 1080, height: 1920 };
/**
 * Captions end this share of the height above the bottom, clear of the
 * buttons and descriptions TikTok, Reels and Shorts lay over the bottom.
 */
export const SHORT_CAPTION_MARGIN = 0.25;
/** Seconds of air kept before the first word and after the last. */
export const SHORT_PADDING_SECONDS = 0.25;

/**
 * Captions laid out again for another canvas: re-wrapped to its width and
 * placed as the caption tool places them (above the platforms' buttons), keeping their words, timing and
 * look.
 */
export function relayoutCaptions({ tracks, canvas }: { tracks: SceneTracks; canvas: { width: number; height: number } }): SceneTracks {
	const relayout = (element: TimelineElement): TimelineElement => {
		if (element.type !== "text" || !element.words?.length) return element;
		const params = element.params as Record<string, unknown>;
		const pick = <T,>(key: string) => (params[key] === undefined ? undefined : (params[key] as T));
		const built = buildSubtitleTextElement({
			index: 0,
			caption: {
				text: element.words.map((word) => word.text.trim()).join(" "),
				startTime: 0,
				duration: element.duration / TICKS_PER_SECOND,
				style: {
					fontSize: pick<number>("fontSize"),
					fontFamily: pick<string>("fontFamily"),
					fontWeight: pick("fontWeight"),
					fontStyle: pick("fontStyle"),
					textAlign: pick("textAlign"),
					letterSpacing: pick<number>("letterSpacing"),
					lineHeight: pick<number>("lineHeight"),
					placement: { verticalAlign: "bottom", marginVerticalRatio: SHORT_CAPTION_MARGIN },
				},
			},
			canvasSize: canvas,
		});
		return {
			...element,
			params: {
				...element.params,
				content: built.params.content,
				"transform.positionX": built.params["transform.positionX"],
				"transform.positionY": built.params["transform.positionY"],
			},
		} as TimelineElement;
	};
	const relayoutTrack = <T extends TimelineTrack>(track: T): T =>
		({ ...track, elements: (track.elements as TimelineElement[]).map(relayout) }) as T;
	return { overlay: tracks.overlay.map(relayoutTrack), main: relayoutTrack(tracks.main), audio: tracks.audio.map(relayoutTrack) };
}

/** The open project's transcript as lines: the words of all visible captions. */
export function projectLines({ editor }: { editor: EditorCore }): TimedLine[] {
	const words = wordsFromCaptions({ tracks: editor.scenes.getActiveScene().tracks, from: 0, to: Number.POSITIVE_INFINITY });
	return transcriptLines({ words });
}

/** Claude's picks through the API worker (needs the server passcode). */
export async function claudePicks({
	lines,
	options,
	passcode,
	fetchImpl = fetch,
}: {
	lines: TimedLine[];
	options: ShortsOptions;
	passcode: string;
	fetchImpl?: typeof fetch;
}): Promise<{ picks: ShortPick[]; model: string }> {
	const response = await fetchImpl(apiUrl("/shorts/plan"), {
		method: "POST",
		headers: { "Content-Type": "application/json", "X-Share-Key": passcode },
		body: JSON.stringify({
			lines: lines.map((line) => ({ start: line.start, end: line.end, text: line.text })),
			count: options.count,
			minSeconds: options.minSeconds,
			maxSeconds: options.maxSeconds,
		}),
	});
	const body = (await response.json().catch(() => null)) as { clips?: unknown; model?: unknown; error?: unknown } | null;
	if (!response.ok) throw new Error(typeof body?.error === "string" ? body.error : `Picking shorts failed (${response.status})`);
	const clips = Array.isArray(body?.clips) ? (body.clips as Partial<ShortPick>[]) : null;
	if (!clips) throw new Error("The server's answer had no clips");
	const picks = clips
		.filter((clip) => Number.isInteger(clip.first) && Number.isInteger(clip.last) && lines[clip.first as number] && lines[clip.last as number])
		.map((clip, index) => ({
			first: clip.first as number,
			last: clip.last as number,
			start: lines[clip.first as number].start,
			end: lines[clip.last as number].end,
			title: typeof clip.title === "string" && clip.title.trim() ? clip.title.trim() : `Short ${index + 1}`,
			reason: typeof clip.reason === "string" ? clip.reason : "",
		}));
	return { picks, model: typeof body?.model === "string" ? body.model : "" };
}

/** Picks with Claude when a passcode is given, else locally. */
export async function pickShorts({
	lines,
	options,
	passcode,
	fetchImpl,
}: {
	lines: TimedLine[];
	options: ShortsOptions;
	passcode?: string;
	fetchImpl?: typeof fetch;
}): Promise<{ picks: ShortPick[]; by: "claude" | "local"; model: string }> {
	if (passcode?.trim()) {
		const { picks, model } = await claudePicks({ lines, options, passcode: passcode.trim(), fetchImpl });
		return { picks, by: "claude", model };
	}
	return { picks: localPicks({ lines, options }), by: "local", model: "" };
}

export interface MadeShort {
	projectId: string;
	name: string;
	pick: ShortPick;
	seconds: number;
}

/** Saves each pick as a new 9:16 project cut from the open one's active scene. */
export async function makeShorts({
	editor,
	picks,
	canvas = SHORT_CANVAS,
	paddingSeconds = SHORT_PADDING_SECONDS,
	onProgress,
}: {
	editor: EditorCore;
	picks: ShortPick[];
	canvas?: { width: number; height: number };
	paddingSeconds?: number;
	onProgress?: (done: number, total: number) => void;
}): Promise<MadeShort[]> {
	const source = editor.project.getActive();
	const scene = editor.scenes.getActiveScene();
	const total = editor.timeline.getTotalDuration() / TICKS_PER_SECOND;
	const sizes = new Map(editor.media.getAssets().map((asset) => [asset.id, asset]));
	const sizeOf = (mediaId: string) => {
		const asset = sizes.get(mediaId);
		return asset?.width && asset.height ? { width: asset.width, height: asset.height } : null;
	};
	const made: MadeShort[] = [];
	for (const [index, pick] of picks.entries()) {
		const start = Math.max(0, pick.start - paddingSeconds);
		const end = Math.min(total, pick.end + paddingSeconds);
		const tracks = relayoutCaptions({
			tracks: coverCanvas({
				tracks: cropTracks({ tracks: scene.tracks, start: Math.round(start * TICKS_PER_SECOND), end: Math.round(end * TICKS_PER_SECOND) }),
				canvas,
				sizeOf,
			}),
			canvas,
		});
		const now = new Date();
		const shortScene = { ...scene, id: generateUUID(), name: "Main scene", isMain: true, tracks, bookmarks: [], createdAt: now, updatedAt: now };
		const name = `${source.metadata.name} – ${pick.title}`;
		const project: TProject = {
			...source,
			metadata: {
				...source.metadata,
				id: generateUUID(),
				name,
				thumbnail: undefined,
				duration: getProjectDurationFromScenes({ scenes: [shortScene] }),
				createdAt: now,
				updatedAt: now,
			},
			scenes: [shortScene],
			currentSceneId: shortScene.id,
			settings: { ...source.settings, canvasSize: { ...canvas }, canvasSizeMode: "preset", originalCanvasSize: null },
			timelineViewState: undefined,
		};
		const mediaIds = new Set<string>();
		for (const track of [...tracks.overlay, tracks.main, ...tracks.audio]) {
			for (const element of track.elements as TimelineElement[]) {
				if ("mediaId" in element && typeof element.mediaId === "string") mediaIds.add(element.mediaId);
			}
		}
		const projectId = await editor.project.saveDerivedProject({ project, sourceProjectId: source.metadata.id, mediaIds });
		made.push({ projectId, name, pick, seconds: end - start });
		onProgress?.(index + 1, picks.length);
	}
	return made;
}
