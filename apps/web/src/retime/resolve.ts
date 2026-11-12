/**
 * The map from a clip's own time to its source time. A constant `rate`
 * is the plain case; a `ramp` makes the rate a piecewise-linear function
 * of clip time (keys in seconds from the clip's start, held flat before
 * the first and after the last), and the source time is its integral;
 * `reverse` plays the clip's source span backwards. The desktop's
 * `crates/editor/src/retime.rs` is the same arithmetic.
 */

import type { RetimeConfig, RetimeRampKey } from "@/timeline";
import { clampRetimeRate } from "@/retime/rate";

function getSafeRate({ rate }: { rate: number }): number {
	return clampRetimeRate({ rate });
}

/** The ramp's keys sorted by time with safe rates, or `null` when there is no usable ramp. */
export function normalizeRamp({ retime }: { retime?: RetimeConfig }): RetimeRampKey[] | null {
	if (!retime?.ramp || retime.ramp.length === 0) return null;
	const keys = retime.ramp
		.filter((key) => Number.isFinite(key.time) && key.time >= 0)
		.map((key) => ({ time: key.time, rate: getSafeRate({ rate: key.rate }) }))
		.sort((a, b) => a.time - b.time);
	return keys.length > 0 ? keys : null;
}

/** Whether the clip's source time is not a plain multiple of its clip time. */
export function hasVariableRetime({ retime }: { retime?: RetimeConfig }): boolean {
	return Boolean(retime?.reverse) || normalizeRamp({ retime }) !== null;
}

/** The playback rate at a clip time, in seconds. */
function rateAtSeconds({ seconds, keys, base }: { seconds: number; keys: RetimeRampKey[] | null; base: number }): number {
	if (!keys) return base;
	if (seconds <= keys[0].time) return keys[0].rate;
	const last = keys[keys.length - 1];
	if (seconds >= last.time) return last.rate;
	for (let index = 1; index < keys.length; index++) {
		const from = keys[index - 1];
		const to = keys[index];
		if (seconds <= to.time) {
			const span = to.time - from.time;
			const t = span > 0 ? (seconds - from.time) / span : 1;
			return from.rate + (to.rate - from.rate) * t;
		}
	}
	return last.rate;
}

/** Source seconds consumed by the first `seconds` of clip time. */
function forwardSeconds({ seconds, keys, base }: { seconds: number; keys: RetimeRampKey[] | null; base: number }): number {
	if (seconds <= 0) return 0;
	if (!keys) return seconds * base;
	let total = 0;
	let cursor = 0;
	// Flat before the first key.
	const first = keys[0];
	if (seconds <= first.time) return seconds * first.rate;
	total += first.time * first.rate;
	cursor = first.time;
	for (let index = 1; index < keys.length; index++) {
		const from = keys[index - 1];
		const to = keys[index];
		if (to.time <= cursor) continue;
		const end = Math.min(seconds, to.time);
		if (end > cursor) {
			// Trapezoid under the linear rate between `from` and `to`.
			const span = to.time - from.time;
			const rateAt = (t: number) => (span > 0 ? from.rate + ((to.rate - from.rate) * (t - from.time)) / span : to.rate);
			total += ((rateAt(cursor) + rateAt(end)) / 2) * (end - cursor);
			cursor = end;
		}
		if (seconds <= to.time) return total;
	}
	// Flat after the last key.
	const last = keys[keys.length - 1];
	return total + (seconds - cursor) * last.rate;
}

/** The clip time whose forward map reaches `source` seconds (bisection; the map is increasing). */
function inverseSeconds({ source, keys, base }: { source: number; keys: RetimeRampKey[] | null; base: number }): number {
	if (source <= 0) return 0;
	if (!keys) return source / base;
	let low = 0;
	let high = Math.max(1, source / Math.min(...keys.map((key) => key.rate)));
	while (forwardSeconds({ seconds: high, keys, base }) < source && high < 1e7) high *= 2;
	for (let step = 0; step < 60; step++) {
		const mid = (low + high) / 2;
		if (forwardSeconds({ seconds: mid, keys, base }) < source) low = mid;
		else high = mid;
	}
	return (low + high) / 2;
}

interface MapOptions {
	retime?: RetimeConfig;
	/** The clip's length in the caller's unit; needed for `reverse`. */
	duration?: number;
	/** The caller's time unit in ticks per second (1 when the caller works in seconds). */
	ticksPerSecond?: number;
}

function context({ retime, ticksPerSecond = 1 }: MapOptions) {
	const base = getSafeRate({ rate: retime?.rate ?? 1 });
	const keys = normalizeRamp({ retime })?.map((key) => ({ time: key.time * ticksPerSecond, rate: key.rate })) ?? null;
	return { base, keys };
}

/** Source time (from the trim start) for a clip time, in the caller's unit. */
export function getSourceTimeAtClipTime({ clipTime, ...options }: { clipTime: number } & MapOptions): number {
	const { base, keys } = context(options);
	const forward = forwardSeconds({ seconds: clipTime, keys, base });
	if (options.retime?.reverse && options.duration !== undefined && options.duration > 0) {
		const span = forwardSeconds({ seconds: options.duration, keys, base });
		return Math.max(0, span - forward);
	}
	return forward;
}

/** Clip time for a source time (from the trim start), in the caller's unit. */
export function getClipTimeAtSourceTime({ sourceTime, ...options }: { sourceTime: number } & MapOptions): number {
	const { base, keys } = context(options);
	if (options.retime?.reverse && options.duration !== undefined && options.duration > 0) {
		const span = forwardSeconds({ seconds: options.duration, keys, base });
		return inverseSeconds({ source: Math.max(0, span - sourceTime), keys, base });
	}
	return inverseSeconds({ source: sourceTime, keys, base });
}

/** The rate at a clip time (1 when there is no retime); reverse does not change it. */
export function getEffectiveRateAt({ clipTime = 0, ...options }: { clipTime?: number } & MapOptions): number {
	const { base, keys } = context(options);
	return rateAtSeconds({ seconds: clipTime, keys, base });
}

/** Timeline length that plays `sourceSpan` of source, in the caller's unit. */
export function getTimelineDurationForSourceSpan({ sourceSpan, ...options }: { sourceSpan: number } & MapOptions): number {
	if (sourceSpan <= 0) return 0;
	const { base, keys } = context(options);
	return inverseSeconds({ source: sourceSpan, keys, base });
}
