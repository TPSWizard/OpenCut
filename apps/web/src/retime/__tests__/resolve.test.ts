import { describe, expect, test } from "vitest";
import {
	getClipTimeAtSourceTime,
	getEffectiveRateAt,
	getSourceTimeAtClipTime,
	getTimelineDurationForSourceSpan,
} from "@/retime";
import type { RetimeConfig } from "@/timeline";

const twoX: RetimeConfig = { rate: 2 };
const halfX: RetimeConfig = { rate: 0.5 };

describe("retime resolve", () => {
	test("maps clip time to source time at 2x speed", () => {
		expect(getSourceTimeAtClipTime({ clipTime: 5, retime: twoX })).toBe(10);
	});

	test("maps clip time to source time at 0.5x speed", () => {
		expect(getSourceTimeAtClipTime({ clipTime: 4, retime: halfX })).toBe(2);
	});

	test("returns clip time unchanged when no retime", () => {
		expect(getSourceTimeAtClipTime({ clipTime: 7 })).toBe(7);
	});

	test("inverts source time back to clip time at 2x speed", () => {
		expect(getClipTimeAtSourceTime({ sourceTime: 10, retime: twoX })).toBe(5);
	});

	test("returns effective rate", () => {
		expect(getEffectiveRateAt({ retime: twoX })).toBe(2);
		expect(getEffectiveRateAt({})).toBe(1);
	});

	test("derives timeline duration for a visible source span", () => {
		expect(
			getTimelineDurationForSourceSpan({ sourceSpan: 10, retime: twoX }),
		).toBe(5);
		expect(
			getTimelineDurationForSourceSpan({ sourceSpan: 10, retime: halfX }),
		).toBe(20);
	});

	test("clamps invalid rates to 1", () => {
		expect(getSourceTimeAtClipTime({ clipTime: 5, retime: { rate: 0 } })).toBe(
			5,
		);
		expect(
			getSourceTimeAtClipTime({ clipTime: 5, retime: { rate: -1 } }),
		).toBe(5);
	});

	test("caps retime rates above 5x", () => {
		expect(getSourceTimeAtClipTime({ clipTime: 5, retime: { rate: 100 } })).toBe(
			25,
		);
		expect(
			getTimelineDurationForSourceSpan({ sourceSpan: 10, retime: { rate: 100 } }),
		).toBe(2);
	});

	test("ramps integrate the rate, in seconds or ticks", () => {
		// 1x for the first second, rising to 3x by the third, then flat.
		const ramp: RetimeConfig = { rate: 1, ramp: [{ time: 1, rate: 1 }, { time: 3, rate: 3 }] };
		expect(getEffectiveRateAt({ clipTime: 2, retime: ramp })).toBe(2);
		expect(getSourceTimeAtClipTime({ clipTime: 1, retime: ramp })).toBeCloseTo(1, 9);
		expect(getSourceTimeAtClipTime({ clipTime: 3, retime: ramp })).toBeCloseTo(5, 9);
		expect(getSourceTimeAtClipTime({ clipTime: 4, retime: ramp })).toBeCloseTo(8, 9);
		expect(getClipTimeAtSourceTime({ sourceTime: 8, retime: ramp })).toBeCloseTo(4, 6);
		expect(getTimelineDurationForSourceSpan({ sourceSpan: 8, retime: ramp })).toBeCloseTo(4, 6);
		expect(getSourceTimeAtClipTime({ clipTime: 4000, retime: ramp, ticksPerSecond: 1000 })).toBeCloseTo(8000, 6);
	});

	test("reverse plays the span backwards when the duration is known", () => {
		const back: RetimeConfig = { rate: 2, reverse: true };
		expect(getSourceTimeAtClipTime({ clipTime: 0, retime: back, duration: 5 })).toBe(10);
		expect(getSourceTimeAtClipTime({ clipTime: 5, retime: back, duration: 5 })).toBe(0);
		expect(getSourceTimeAtClipTime({ clipTime: 1, retime: back, duration: 5 })).toBe(8);
		expect(getClipTimeAtSourceTime({ sourceTime: 8, retime: back, duration: 5 })).toBeCloseTo(1, 9);
		expect(getSourceTimeAtClipTime({ clipTime: 1, retime: back })).toBe(2);
	});
});
