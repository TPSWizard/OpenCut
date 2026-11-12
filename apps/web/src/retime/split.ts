import type { RetimeConfig } from "@/timeline";
import { TICKS_PER_SECOND } from "@/wasm";
import { getSourceTimeAtClipTime } from "./resolve";

/** Source span consumed by `clipTime` ticks of the clip (ramps are read in ticks). */
export function getSourceSpanAtClipTime({
	clipTime,
	retime,
	ticksPerSecond = TICKS_PER_SECOND,
}: {
	clipTime: number;
	retime?: RetimeConfig;
	ticksPerSecond?: number;
}): number {
	return Math.max(0, getSourceTimeAtClipTime({ clipTime, retime, ticksPerSecond }));
}

/** The retime of each half of a clip split at `splitClipTime` ticks: ramp keys move with the right piece. */
export function splitRetimeAtClipTime({
	retime,
	splitClipTime,
	ticksPerSecond = TICKS_PER_SECOND,
}: {
	retime?: RetimeConfig;
	splitClipTime: number;
	ticksPerSecond?: number;
}): {
	left: RetimeConfig | undefined;
	right: RetimeConfig | undefined;
} {
	if (!retime?.ramp?.length) return { left: retime, right: retime };
	const splitSeconds = splitClipTime / ticksPerSecond;
	return {
		left: retime,
		right: { ...retime, ramp: retime.ramp.map((key) => ({ time: key.time - splitSeconds, rate: key.rate })) },
	};
}

/** The retime after `clipTrimTime` ticks are trimmed from a side: start trims shift the ramp keys. */
export function adjustRetimeForTrimChange({
	retime,
	clipTrimTime,
	side,
	ticksPerSecond = TICKS_PER_SECOND,
}: {
	retime?: RetimeConfig;
	clipTrimTime: number;
	side: "start" | "end";
	ticksPerSecond?: number;
}): RetimeConfig | undefined {
	if (!retime?.ramp?.length || side !== "start") return retime;
	const seconds = clipTrimTime / ticksPerSecond;
	return { ...retime, ramp: retime.ramp.map((key) => ({ time: key.time - seconds, rate: key.rate })) };
}
