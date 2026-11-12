import { describe, expect, it } from "vitest";
import { histogram, histogramDistance, pickCuts, sourceToTimeline } from "../detect";
import { TICKS_PER_SECOND, mediaTime } from "@/wasm";

function solid({ r, g, b, pixels = 64 }: { r: number; g: number; b: number; pixels?: number }): Uint8ClampedArray {
	const out = new Uint8ClampedArray(pixels * 4);
	for (let index = 0; index < pixels; index++) {
		out[index * 4] = r;
		out[index * 4 + 1] = g;
		out[index * 4 + 2] = b;
		out[index * 4 + 3] = 255;
	}
	return out;
}

describe("histogramDistance", () => {
	it("is 0 for the same picture and 100 for a different colour", () => {
		const red = histogram({ rgba: solid({ r: 200, g: 20, b: 20 }) });
		const redAgain = histogram({ rgba: solid({ r: 205, g: 25, b: 18 }) });
		const blue = histogram({ rgba: solid({ r: 20, g: 20, b: 220 }) });
		expect(histogramDistance({ a: red, b: redAgain })).toBe(0);
		expect(histogramDistance({ a: red, b: blue })).toBe(100);
		// Half the pixels change: half the mass moves.
		const half = new Uint8ClampedArray([...solid({ r: 200, g: 20, b: 20, pixels: 32 }), ...solid({ r: 20, g: 20, b: 220, pixels: 32 })]);
		expect(histogramDistance({ a: red, b: histogram({ rgba: half }) })).toBeCloseTo(50, 5);
		expect(histogram({ rgba: new Uint8ClampedArray(0) }).every((bin) => bin === 0)).toBe(true);
	});
});

describe("pickCuts", () => {
	it("keeps strong changes that are far enough apart, preferring the stronger of a cluster", () => {
		const scores = [
			{ seconds: 0.1, score: 2 },
			{ seconds: 2.0, score: 60 },
			{ seconds: 2.1, score: 80 },
			{ seconds: 2.3, score: 40 },
			{ seconds: 4.0, score: 35 },
			{ seconds: 5.0, score: 10 },
		];
		expect(pickCuts({ scores, threshold: 30, minGapSeconds: 0.5 })).toEqual([
			{ seconds: 2.1, score: 80 },
			{ seconds: 4.0, score: 35 },
		]);
		expect(pickCuts({ scores, threshold: 90, minGapSeconds: 0.5 })).toEqual([]);
	});
});

describe("sourceToTimeline", () => {
	it("maps through the trim and drops cuts outside the clip", () => {
		const element = {
			startTime: mediaTime({ ticks: 10 * TICKS_PER_SECOND }),
			duration: mediaTime({ ticks: 4 * TICKS_PER_SECOND }),
			trimStart: mediaTime({ ticks: 1 * TICKS_PER_SECOND }),
		};
		expect(sourceToTimeline({ element, sourceSeconds: 2 })).toBe(11 * TICKS_PER_SECOND);
		expect(sourceToTimeline({ element, sourceSeconds: 1 })).toBeNull();
		expect(sourceToTimeline({ element, sourceSeconds: 5 })).toBeNull();
		expect(sourceToTimeline({ element, sourceSeconds: 0.5 })).toBeNull();
	});
});
