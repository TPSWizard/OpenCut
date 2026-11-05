/**
 * Auto-ducking: music drops while someone speaks. The voice is measured as
 * an RMS level per short window, windows above a threshold become speech
 * intervals (short gaps are bridged), and each interval becomes a dip in
 * the music's volume keyframes with a ramp down before it and a ramp back
 * up after it.
 *
 * The desktop keeps the same arithmetic in `crates/editor/src/ducking.rs`.
 */

export interface DuckingOptions {
	/** How far the music drops while speech plays, in dB (negative). */
	amountDb: number;
	/** RMS level, in dBFS, above which a window counts as speech. */
	thresholdDb: number;
	/** Seconds the music takes to drop before speech starts. */
	attackSeconds: number;
	/** Seconds the music takes to come back after speech ends. */
	releaseSeconds: number;
	/** Gaps in speech shorter than this stay ducked. */
	holdSeconds: number;
	/** Length of one measurement window in seconds. */
	windowSeconds: number;
	/** Speech shorter than this is ignored. */
	minSpeechSeconds: number;
}

export const DUCKING_DEFAULTS: DuckingOptions = {
	amountDb: -12,
	thresholdDb: -40,
	attackSeconds: 0.1,
	releaseSeconds: 0.5,
	holdSeconds: 0.3,
	windowSeconds: 0.05,
	minSpeechSeconds: 0.1,
};

/** The level reported for a window with no signal. */
export const SILENCE_DB = -100;

export interface Interval {
	start: number;
	end: number;
}

/** Times are kept to the microsecond so window arithmetic stays exact. */
function round({ seconds }: { seconds: number }): number {
	return Math.round(seconds * 1e6) / 1e6;
}

/** RMS level in dBFS of each consecutive window of mono samples. */
export function rmsDbWindows({
	samples,
	sampleRate,
	windowSeconds,
}: {
	samples: ArrayLike<number>;
	sampleRate: number;
	windowSeconds: number;
}): Float64Array {
	const windowSize = Math.max(1, Math.round(sampleRate * windowSeconds));
	const count = Math.ceil(samples.length / windowSize);
	const levels = new Float64Array(count);
	for (let window = 0; window < count; window++) {
		const start = window * windowSize;
		const end = Math.min(samples.length, start + windowSize);
		let energy = 0;
		for (let index = start; index < end; index++) {
			const value = samples[index];
			energy += value * value;
		}
		levels[window] = meanSquareToDb({ meanSquare: energy / Math.max(1, end - start) });
	}
	return levels;
}

export function meanSquareToDb({ meanSquare }: { meanSquare: number }): number {
	if (!(meanSquare > 0)) return SILENCE_DB;
	return Math.max(SILENCE_DB, 10 * Math.log10(meanSquare));
}

/**
 * Where speech plays, from per-window levels. Window `i` covers
 * `offsetSeconds + i * windowSeconds` onward. Gaps shorter than
 * `holdSeconds` are bridged; intervals shorter than `minSpeechSeconds`
 * are dropped.
 */
export function speechIntervals({
	levels,
	windowSeconds,
	thresholdDb,
	holdSeconds,
	minSpeechSeconds,
	offsetSeconds = 0,
}: {
	levels: ArrayLike<number>;
	windowSeconds: number;
	thresholdDb: number;
	holdSeconds: number;
	minSpeechSeconds: number;
	offsetSeconds?: number;
}): Interval[] {
	const raw: Interval[] = [];
	let open: Interval | null = null;
	for (let index = 0; index < levels.length; index++) {
		const loud = levels[index] >= thresholdDb;
		const start = round({ seconds: offsetSeconds + index * windowSeconds });
		const end = round({ seconds: start + windowSeconds });
		if (loud) {
			if (open) {
				open.end = end;
			} else {
				open = { start, end };
			}
		} else if (open) {
			raw.push(open);
			open = null;
		}
	}
	if (open) raw.push(open);

	const merged: Interval[] = [];
	for (const interval of raw) {
		const last = merged[merged.length - 1];
		if (last && interval.start - last.end < holdSeconds) {
			last.end = interval.end;
		} else {
			merged.push({ ...interval });
		}
	}
	return merged.filter(
		(interval) => interval.end - interval.start >= minSpeechSeconds - 1e-9,
	);
}

export interface DuckingKey {
	/** Seconds from the start of the music clip. */
	time: number;
	/** Volume in dB. */
	value: number;
}

/**
 * Volume keys (seconds from the clip start, dB) that dip the music for
 * each speech interval, given in timeline seconds. Ramps that would cross
 * each other merge into one dip; keys outside the clip are clamped to
 * its edges.
 */
export function buildDuckingKeys({
	intervals,
	clipStart,
	clipDuration,
	baseDb,
	amountDb,
	attackSeconds,
	releaseSeconds,
}: {
	intervals: Interval[];
	clipStart: number;
	clipDuration: number;
	baseDb: number;
	amountDb: number;
	attackSeconds: number;
	releaseSeconds: number;
}): DuckingKey[] {
	const ducked = baseDb + amountDb;
	const clipEnd = clipStart + clipDuration;
	const inClip = intervals
		.filter((interval) => interval.end > clipStart && interval.start < clipEnd)
		.sort((a, b) => a.start - b.start);
	if (inClip.length === 0 || clipDuration <= 0) return [];

	// Merge dips whose release would run into the next attack.
	const dips: Interval[] = [];
	for (const interval of inClip) {
		const last = dips[dips.length - 1];
		if (last && interval.start - attackSeconds <= last.end + releaseSeconds) {
			last.end = Math.max(last.end, interval.end);
		} else {
			dips.push({ ...interval });
		}
	}

	const keys: DuckingKey[] = [];
	const push = ({ time, value }: DuckingKey) => {
		const clamped = round({
			seconds: Math.min(clipDuration, Math.max(0, time - clipStart)),
		});
		const last = keys[keys.length - 1];
		if (last && Math.abs(last.time - clamped) < 1e-6) {
			// Keys that fall before the clip collapse onto its start and the
			// latest wins; keys past its end collapse onto the end and the
			// earliest wins, so a dip still running at the end stays down.
			if (time - clipStart < clipDuration) last.value = value;
			return;
		}
		keys.push({ time: clamped, value });
	};
	for (const dip of dips) {
		push({ time: dip.start - attackSeconds, value: baseDb });
		push({ time: dip.start, value: ducked });
		push({ time: dip.end, value: ducked });
		push({ time: dip.end + releaseSeconds, value: baseDb });
	}
	return keys;
}

/** Seconds of speech inside a set of intervals. */
export function totalSeconds({ intervals }: { intervals: Interval[] }): number {
	return intervals.reduce((sum, interval) => sum + (interval.end - interval.start), 0);
}
