/**
 * Colour match on the timeline: the reference clip's frame (at the playhead
 * when it is over the clip, else its middle) is measured, each target clip
 * is measured over a few frames, and each target gets a `lut` effect holding
 * the transfer, in one undo step. A target matched before has its earlier
 * colour-match effect updated instead of gaining another.
 * `colour_match_commands` in the Rust editor does the same.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { buildDefaultEffectInstance } from "@/effects";
import type { Effect } from "@/effects/types";
import { getSourceTimeAtClipTime } from "@/retime/resolve";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { ColourAccumulator, type ColourStats, colourMatchParam, isColourMatchParam } from "./match";

/** Width the frames are measured at. */
const SAMPLE_WIDTH = 160;
/** Frames of each target clip measured. */
export const TARGET_SAMPLES = 5;

export interface ColourMatchRef {
	trackId: string;
	elementId: string;
}

type VisualClip = Extract<TimelineElement, { type: "video" | "image" }>;

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** RGBA pixels of a clip at clip times (seconds from its start). */
export type FrameReader = (input: { editor: EditorCore; element: VisualClip; clipSeconds: number[] }) => Promise<Uint8ClampedArray[]>;

function context2d(canvas: HTMLCanvasElement | OffscreenCanvas) {
	const context = canvas.getContext("2d", { willReadFrequently: true }) as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
	if (!context) throw new Error("The frame could not be read");
	return context;
}

/** Reads frames with mediabunny (video) or the browser's image decoder (pictures). */
export const readClipFrames: FrameReader = async ({ editor, element, clipSeconds }) => {
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error(`The media of ${element.name} is not available`);
	if (element.type === "image") {
		const bitmap = await createImageBitmap(asset.file);
		const width = Math.min(SAMPLE_WIDTH, bitmap.width);
		const height = Math.max(1, Math.round((width * bitmap.height) / bitmap.width));
		const canvas = new OffscreenCanvas(width, height);
		const context = context2d(canvas);
		context.drawImage(bitmap, 0, 0, width, height);
		bitmap.close();
		return [context.getImageData(0, 0, width, height).data];
	}
	const clipLength = element.duration / TICKS_PER_SECOND;
	const trim = element.trimStart / TICKS_PER_SECOND;
	const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
	try {
		const video = await input.getPrimaryVideoTrack();
		if (!video || !(await video.canDecode())) throw new Error(`The video of ${element.name} cannot be decoded in this browser`);
		const height = Math.max(2, Math.round((SAMPLE_WIDTH * (video.displayHeight || 1080)) / (video.displayWidth || 1920) / 2) * 2);
		const sink = new CanvasSink(video, { width: SAMPLE_WIDTH, height, fit: "fill" });
		const times = clipSeconds.map((seconds) => trim + getSourceTimeAtClipTime({ clipTime: seconds, retime: element.retime, duration: clipLength }));
		const frames: Uint8ClampedArray[] = [];
		for await (const wrapped of sink.canvasesAtTimestamps(times)) {
			if (!wrapped) continue;
			const canvas = wrapped.canvas as HTMLCanvasElement | OffscreenCanvas;
			frames.push(context2d(canvas).getImageData(0, 0, canvas.width, canvas.height).data);
		}
		return frames;
	} finally {
		input.dispose();
	}
};

function measure(frames: Uint8ClampedArray[], name: string): ColourStats {
	const accumulator = new ColourAccumulator();
	for (const frame of frames) accumulator.addRgba(frame);
	const stats = accumulator.stats();
	if (!stats) throw new Error(`No pixels could be read from ${name}`);
	return stats;
}

export interface ColourMatchResult {
	/** Clip time of the reference frame, seconds. */
	referenceSeconds: number;
	reference: ColourStats;
	targets: { trackId: string; elementId: string; source: ColourStats }[];
}

/**
 * Makes `targets` match the colours of `reference`'s frame at timeline time
 * `at` seconds (default: the playhead; its middle when `at` is outside it).
 */
export async function matchColours({
	editor,
	reference,
	targets,
	at,
	readFrames = readClipFrames,
}: {
	editor: EditorCore;
	reference: ColourMatchRef;
	targets: readonly ColourMatchRef[];
	at?: number;
	/** Injected for tests. */
	readFrames?: FrameReader;
}): Promise<ColourMatchResult> {
	if (targets.length === 0) throw new Error("Pick at least one clip to match");
	const tracks = allTracks(editor.scenes.getActiveScene().tracks);
	const find = ({ trackId, elementId }: ColourMatchRef) => {
		const track = tracks.find((candidate) => candidate.id === trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
		if (!track || !element || (element.type !== "video" && element.type !== "image")) throw new Error("Colour match works on video and picture clips");
		return { track, element: element as VisualClip };
	};
	const ref = find(reference);
	const clips = targets.map((target) => {
		if (target.elementId === reference.elementId) throw new Error("The reference clip cannot match itself");
		const found = find(target);
		if (found.track.locked) throw new Error("A clip to match is on a locked track");
		return found;
	});

	const refLength = ref.element.duration / TICKS_PER_SECOND;
	const playhead = at ?? editor.playback.getCurrentTime() / TICKS_PER_SECOND;
	const fromStart = playhead - ref.element.startTime / TICKS_PER_SECOND;
	const referenceSeconds = fromStart >= 0 && fromStart < refLength ? fromStart : refLength / 2;
	const referenceStats = measure(await readFrames({ editor, element: ref.element, clipSeconds: [referenceSeconds] }), ref.element.name);

	const measured: ColourMatchResult["targets"] = [];
	const updates: { trackId: string; elementId: string; patch: Partial<TimelineElement> }[] = [];
	for (const { track, element } of clips) {
		const length = element.duration / TICKS_PER_SECOND;
		const clipSeconds = Array.from({ length: TARGET_SAMPLES }, (_, index) => ((index + 0.5) / TARGET_SAMPLES) * length);
		const source = measure(await readFrames({ editor, element, clipSeconds }), element.name);
		measured.push({ trackId: track.id, elementId: element.id, source });
		const param = colourMatchParam(source, referenceStats);
		const effects: Effect[] = element.effects ?? [];
		const existing = effects.findIndex((effect) => effect.type === "lut" && isColourMatchParam(effect.params.lut));
		const next =
			existing >= 0
				? effects.map((effect, index) => (index === existing ? { ...effect, enabled: true, params: { ...effect.params, lut: param, intensity: 1 } } : effect))
				: [...effects, (() => {
						const effect = buildDefaultEffectInstance({ effectType: "lut" });
						return { ...effect, params: { ...effect.params, lut: param, intensity: 1 } };
					})()];
		updates.push({ trackId: track.id, elementId: element.id, patch: { effects: next } as Partial<TimelineElement> });
	}
	editor.command.execute({ command: new UpdateElementsCommand({ updates }) });
	return { referenceSeconds, reference: referenceStats, targets: measured };
}
