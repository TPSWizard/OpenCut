import { describe, expect, it } from "vitest";
import {
	SILENCE_DB,
	buildDuckingKeys,
	rmsDbWindows,
	speechIntervals,
} from "../envelope";

describe("rmsDbWindows", () => {
	it("measures each window in dBFS", () => {
		const sampleRate = 100;
		const samples = new Float32Array(300);
		samples.fill(0.5, 100, 200);
		const levels = rmsDbWindows({ samples, sampleRate, windowSeconds: 1 });
		expect(levels).toHaveLength(3);
		expect(levels[0]).toBe(SILENCE_DB);
		expect(levels[1]).toBeCloseTo(20 * Math.log10(0.5), 5);
		expect(levels[2]).toBe(SILENCE_DB);
	});
});

describe("speechIntervals", () => {
	const loud = -20;
	const quiet = -60;

	it("turns runs of loud windows into intervals and bridges short gaps", () => {
		const levels = [quiet, loud, loud, quiet, loud, quiet, quiet, quiet, loud, loud];
		const intervals = speechIntervals({
			levels,
			windowSeconds: 0.1,
			thresholdDb: -40,
			holdSeconds: 0.15,
			minSpeechSeconds: 0.1,
			offsetSeconds: 10,
		});
		expect(intervals).toEqual([
			{ start: 10.1, end: 10.5 },
			{ start: 10.8, end: 11 },
		]);
	});

	it("drops speech shorter than the minimum", () => {
		const intervals = speechIntervals({
			levels: [quiet, loud, quiet, loud, loud, loud],
			windowSeconds: 0.05,
			thresholdDb: -40,
			holdSeconds: 0.01,
			minSpeechSeconds: 0.1,
		});
		expect(intervals).toEqual([{ start: 0.15, end: 0.3 }]);
	});
});

describe("buildDuckingKeys", () => {
	it("ramps down before speech and back up after it, relative to the clip", () => {
		const keys = buildDuckingKeys({
			intervals: [{ start: 5, end: 7 }],
			clipStart: 4,
			clipDuration: 10,
			baseDb: -3,
			amountDb: -12,
			attackSeconds: 0.1,
			releaseSeconds: 0.5,
		});
		expect(keys).toEqual([
			{ time: 0.9, value: -3 },
			{ time: 1, value: -15 },
			{ time: 3, value: -15 },
			{ time: 3.5, value: -3 },
		]);
	});

	it("merges dips whose ramps would cross and clamps to the clip", () => {
		const keys = buildDuckingKeys({
			intervals: [
				{ start: 0, end: 1 },
				{ start: 1.3, end: 2 },
				{ start: 9.8, end: 12 },
			],
			clipStart: 0,
			clipDuration: 10,
			baseDb: 0,
			amountDb: -10,
			attackSeconds: 0.1,
			releaseSeconds: 0.5,
		});
		expect(keys).toEqual([
			{ time: 0, value: -10 },
			{ time: 2, value: -10 },
			{ time: 2.5, value: 0 },
			{ time: 9.7, value: 0 },
			{ time: 9.8, value: -10 },
			{ time: 10, value: -10 },
		]);
	});

	it("writes nothing when no speech overlaps the clip", () => {
		expect(
			buildDuckingKeys({
				intervals: [{ start: 20, end: 25 }],
				clipStart: 0,
				clipDuration: 10,
				baseDb: 0,
				amountDb: -12,
				attackSeconds: 0.1,
				releaseSeconds: 0.5,
			}),
		).toEqual([]);
	});
});
