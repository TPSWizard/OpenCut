import { describe, expect, it } from "vitest";
import { SILENCE_DEFAULTS, coveredSeconds, silenceIntervals } from "../detect";

const loud = -20;
const quiet = -70;

describe("silenceIntervals", () => {
	it("removes long quiet stretches and keeps padding next to speech", () => {
		// 0.1 s windows over a 3 s clip at timeline 10: speech 0-0.5, silence 0.5-2, speech 2-3.
		const levels = [
			...Array(5).fill(loud),
			...Array(15).fill(quiet),
			...Array(10).fill(loud),
		];
		const cuts = silenceIntervals({
			levels,
			clipStart: 10,
			clipEnd: 13,
			options: { ...SILENCE_DEFAULTS, windowSeconds: 0.1 },
		});
		expect(cuts).toEqual([{ start: 10.6, end: 11.9 }]);
		expect(coveredSeconds({ intervals: cuts })).toBeCloseTo(1.3, 6);
	});

	it("cuts silence at the clip's edges right up to the edge", () => {
		const levels = [
			...Array(10).fill(quiet),
			...Array(10).fill(loud),
			...Array(10).fill(quiet),
		];
		const cuts = silenceIntervals({
			levels,
			clipStart: 0,
			clipEnd: 3,
			options: { ...SILENCE_DEFAULTS, windowSeconds: 0.1 },
		});
		expect(cuts).toEqual([
			{ start: 0, end: 0.9 },
			{ start: 2.1, end: 3 },
		]);
	});

	it("leaves short pauses alone", () => {
		const levels = [...Array(5).fill(loud), ...Array(4).fill(quiet), ...Array(5).fill(loud)];
		expect(
			silenceIntervals({
				levels,
				clipStart: 0,
				clipEnd: 1.4,
				options: { ...SILENCE_DEFAULTS, windowSeconds: 0.1 },
			}),
		).toEqual([]);
	});

	it("drops a cut the padding would swallow", () => {
		const levels = [...Array(5).fill(loud), ...Array(6).fill(quiet), ...Array(5).fill(loud)];
		expect(
			silenceIntervals({
				levels,
				clipStart: 0,
				clipEnd: 1.6,
				options: { ...SILENCE_DEFAULTS, windowSeconds: 0.1, paddingSeconds: 0.3 },
			}),
		).toEqual([]);
	});
});
