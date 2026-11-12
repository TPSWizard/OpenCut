/**
 * Shot (scene-change) detection from frame differences: each sampled
 * frame is reduced to a colour histogram, consecutive histograms are
 * compared, and a difference above the threshold that is far enough from
 * the previous cut is a cut. The desktop uses FFmpeg's `scdet` for the
 * scores and shares the cut picking (`crates/editor/src/shots.rs`).
 */

import type { TimelineElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";

/** A frame's difference from the one before it, 0 (same) to 100 (nothing in common). */
export interface FrameScore {
	/** Source time of the frame in seconds. */
	seconds: number;
	score: number;
}

export interface ShotOptions {
	/** A score at or above this is a cut. */
	threshold: number;
	/** Cuts closer than this to the previous cut are dropped. */
	minGapSeconds: number;
	/** How many frames a second are compared. */
	sampleFps: number;
}

export const SHOT_DEFAULTS: ShotOptions = {
	threshold: 30,
	minGapSeconds: 0.5,
	sampleFps: 10,
};

const BINS_PER_CHANNEL = 8;
const BINS = BINS_PER_CHANNEL ** 3;

/** A normalised RGB histogram (8 bins per channel) of RGBA pixels. */
export function histogram({ rgba }: { rgba: Uint8ClampedArray | Uint8Array }): Float32Array {
	const counts = new Float32Array(BINS);
	const pixels = Math.floor(rgba.length / 4);
	if (pixels === 0) return counts;
	const shift = 8 - Math.log2(BINS_PER_CHANNEL);
	for (let index = 0; index < pixels; index++) {
		const r = rgba[index * 4] >> shift;
		const g = rgba[index * 4 + 1] >> shift;
		const b = rgba[index * 4 + 2] >> shift;
		counts[(r * BINS_PER_CHANNEL + g) * BINS_PER_CHANNEL + b] += 1;
	}
	for (let bin = 0; bin < BINS; bin++) counts[bin] /= pixels;
	return counts;
}

/** Half the L1 distance between two normalised histograms, as a percentage. */
export function histogramDistance({ a, b }: { a: Float32Array; b: Float32Array }): number {
	let sum = 0;
	for (let bin = 0; bin < BINS; bin++) sum += Math.abs(a[bin] - b[bin]);
	return Math.min(100, (sum / 2) * 100);
}

/** The cuts among frame scores: at or above the threshold, at least `minGapSeconds` apart. */
export function pickCuts({
	scores,
	threshold,
	minGapSeconds,
}: {
	scores: FrameScore[];
	threshold: number;
	minGapSeconds: number;
}): FrameScore[] {
	const cuts: FrameScore[] = [];
	for (const frame of scores) {
		if (frame.score < threshold) continue;
		const last = cuts[cuts.length - 1];
		if (last && frame.seconds - last.seconds < minGapSeconds) {
			if (frame.score > last.score) cuts[cuts.length - 1] = frame;
			continue;
		}
		cuts.push(frame);
	}
	return cuts;
}

/**
 * Where a source time of the clip lands on the timeline, in ticks, or
 * `null` when it is trimmed away or sits at the clip's edges. Speed changes
 * are not followed.
 */
export function sourceToTimeline({
	element,
	sourceSeconds,
}: {
	element: Pick<TimelineElement, "startTime" | "duration" | "trimStart">;
	sourceSeconds: number;
}): number | null {
	const offset = Math.round(sourceSeconds * TICKS_PER_SECOND) - element.trimStart;
	if (offset <= 0 || offset >= element.duration) return null;
	return element.startTime + offset;
}
