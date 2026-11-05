/**
 * Silence auto-cut: stretches of a clip whose level stays under a
 * threshold for long enough are removed, keeping a little padding of
 * silence on each side of the speech so cuts do not clip words. The
 * desktop keeps the same arithmetic in `crates/editor/src/silence.rs`.
 */

import type { Interval } from "@/ducking/envelope";

export type { Interval };

/** What happens to the rest of the timeline when a stretch is removed. */
export type RippleScope = "all" | "track" | "none";

export const RIPPLE_SCOPES: readonly RippleScope[] = ["all", "track", "none"];

export function isRippleScope(value: unknown): value is RippleScope {
	return typeof value === "string" && RIPPLE_SCOPES.some((scope) => scope === value);
}

export interface SilenceOptions {
	/** RMS level, in dBFS, under which a window counts as silent. */
	thresholdDb: number;
	/** Silence shorter than this stays. */
	minSilenceSeconds: number;
	/** Silence kept on each side of the speech around a cut. */
	paddingSeconds: number;
	/** Length of one measurement window in seconds. */
	windowSeconds: number;
}

export const SILENCE_DEFAULTS: SilenceOptions = {
	thresholdDb: -40,
	minSilenceSeconds: 0.5,
	paddingSeconds: 0.1,
	windowSeconds: 0.05,
};

/** Times are kept to the microsecond so window arithmetic stays exact. */
function round({ seconds }: { seconds: number }): number {
	return Math.round(seconds * 1e6) / 1e6;
}

/**
 * The stretches to remove from a clip, in timeline seconds, from the
 * per-window levels of its own audio. Window `i` covers
 * `clipStart + i * windowSeconds` onward. Silence at the clip's edges is
 * removed up to the edge; silence between speech keeps `paddingSeconds`
 * on each side.
 */
export function silenceIntervals({
	levels,
	clipStart,
	clipEnd,
	options,
}: {
	levels: ArrayLike<number>;
	clipStart: number;
	clipEnd: number;
	options: SilenceOptions;
}): Interval[] {
	const { windowSeconds, thresholdDb, minSilenceSeconds, paddingSeconds } = options;
	const runs: Interval[] = [];
	let open: Interval | null = null;
	for (let index = 0; index < levels.length; index++) {
		const start = round({ seconds: clipStart + index * windowSeconds });
		const end = round({ seconds: Math.min(clipEnd, start + windowSeconds) });
		if (end <= start) break;
		if (levels[index] < thresholdDb) {
			if (open) {
				open.end = end;
			} else {
				open = { start, end };
			}
		} else if (open) {
			runs.push(open);
			open = null;
		}
	}
	if (open) runs.push(open);

	const edge = 1e-6;
	const cuts: Interval[] = [];
	for (const run of runs) {
		if (run.end - run.start < minSilenceSeconds - edge) continue;
		const touchesStart = run.start <= clipStart + edge;
		const touchesEnd = run.end >= clipEnd - edge;
		const start = touchesStart ? clipStart : round({ seconds: run.start + paddingSeconds });
		const end = touchesEnd ? clipEnd : round({ seconds: run.end - paddingSeconds });
		if (end - start < windowSeconds - edge) continue;
		cuts.push({ start, end });
	}
	return cuts;
}

/** Seconds covered by a set of intervals. */
export function coveredSeconds({ intervals }: { intervals: Interval[] }): number {
	return intervals.reduce((sum, interval) => sum + (interval.end - interval.start), 0);
}
