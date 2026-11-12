/**
 * Scanning a video clip for shot changes: its source span is decoded at a
 * small size through mediabunny, every sampled frame is scored against the
 * one before, and the cuts can become bookmarks or splits.
 */

import { ALL_FORMATS, BlobSource, CanvasSink, Input } from "mediabunny";
import { BatchCommand } from "@/commands";
import { SplitElementsCommand } from "@/commands/timeline/element/split-elements";
import type { EditorCore } from "@/core";
import type { TimelineElement, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND, mediaTime } from "@/wasm";
import {
	SHOT_DEFAULTS,
	histogram,
	histogramDistance,
	pickCuts,
	sourceToTimeline,
	type FrameScore,
	type ShotOptions,
} from "./detect";

const SCAN_WIDTH = 64;
const SCAN_HEIGHT = 36;

export interface ShotCut extends FrameScore {
	/** Where the cut lands on the timeline, in ticks; `null` outside the clip. */
	timelineTicks: number | null;
}

export interface ShotScan {
	cuts: ShotCut[];
	framesScored: number;
	/** Source seconds scanned. */
	seconds: number;
}

export type ShotApply = "none" | "bookmarks" | "split";

function findVideo({
	editor,
	trackId,
	elementId,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
}): VideoElement {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || element.type !== "video") {
		throw new Error("Shot detection needs a video clip");
	}
	return element;
}

/** Scores the clip's source span and picks the cuts. */
export async function scanShots({
	editor,
	trackId,
	elementId,
	options = {},
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<ShotOptions>;
	onProgress?: (fraction: number) => void;
}): Promise<ShotScan> {
	const settings: ShotOptions = { ...SHOT_DEFAULTS, ...options };
	const element = findVideo({ editor, trackId, elementId });
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) {
		throw new Error("The clip's media file is not available");
	}
	const sourceFrom = element.trimStart / TICKS_PER_SECOND;
	const sourceTo = (element.trimStart + element.duration) / TICKS_PER_SECOND;
	const input = new Input({ source: new BlobSource(asset.file), formats: ALL_FORMATS });
	const scores: FrameScore[] = [];
	try {
		const videoTrack = await input.getPrimaryVideoTrack();
		if (!videoTrack || !(await videoTrack.canDecode())) {
			throw new Error("The clip's video cannot be decoded in this browser");
		}
		const sink = new CanvasSink(videoTrack, { width: SCAN_WIDTH, height: SCAN_HEIGHT, fit: "fill" });
		const step = 1 / Math.max(1, settings.sampleFps);
		let previous: Float32Array | null = null;
		let nextSample = sourceFrom;
		for await (const wrapped of sink.canvases(sourceFrom, sourceTo)) {
			if (wrapped.timestamp + 1e-6 < nextSample) continue;
			nextSample = wrapped.timestamp + step;
			const context = (wrapped.canvas as HTMLCanvasElement | OffscreenCanvas).getContext("2d") as
				| CanvasRenderingContext2D
				| OffscreenCanvasRenderingContext2D
				| null;
			if (!context) continue;
			const current = histogram({ rgba: context.getImageData(0, 0, SCAN_WIDTH, SCAN_HEIGHT).data });
			if (previous) {
				scores.push({ seconds: wrapped.timestamp, score: histogramDistance({ a: previous, b: current }) });
			}
			previous = current;
			onProgress?.(Math.min(1, (wrapped.timestamp - sourceFrom) / Math.max(1e-6, sourceTo - sourceFrom)));
		}
	} finally {
		input.dispose();
	}
	const cuts = pickCuts({ scores, threshold: settings.threshold, minGapSeconds: settings.minGapSeconds }).map(
		(cut) => ({ ...cut, timelineTicks: sourceToTimeline({ element, sourceSeconds: cut.seconds }) }),
	);
	return { cuts, framesScored: scores.length, seconds: sourceTo - sourceFrom };
}

/**
 * Puts the cuts on the timeline: as bookmarks (those not already there),
 * or as splits of the clip in one undo step. Returns how many were applied.
 */
export function applyShotCuts({
	editor,
	trackId,
	elementId,
	cuts,
	apply,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	cuts: ShotCut[];
	apply: ShotApply;
}): number {
	const times = cuts
		.map((cut) => cut.timelineTicks)
		.filter((ticks): ticks is number => ticks !== null)
		.sort((a, b) => a - b);
	if (apply === "none" || times.length === 0) return 0;
	if (apply === "bookmarks") {
		let added = 0;
		for (const ticks of times) {
			const time = mediaTime({ ticks });
			if (editor.scenes.isBookmarked({ time })) continue;
			void editor.scenes.toggleBookmark({ time });
			added += 1;
		}
		return added;
	}
	// Latest first: each split leaves the left piece with the original id.
	const commands = [...times]
		.reverse()
		.map((ticks) => new SplitElementsCommand({ elements: [{ trackId, elementId }], splitTime: mediaTime({ ticks }) }));
	editor.command.execute({ command: new BatchCommand(commands) });
	return times.length;
}
