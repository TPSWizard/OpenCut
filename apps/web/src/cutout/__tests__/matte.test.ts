import { describe, expect, test } from "vitest";
import { TemporalSmoother, confidenceToLuma, keptShare, matteSize } from "../matte";

describe("cut-out matte", () => {
	test("matte size keeps the source's shape with even sides", () => {
		expect(matteSize({ sourceWidth: 1920, sourceHeight: 1080 })).toEqual({ width: 256, height: 144 });
		expect(matteSize({ sourceWidth: 1080, sourceHeight: 1920 })).toEqual({ width: 256, height: 456 });
	});

	test("confidence goes through a soft threshold", () => {
		const luma = confidenceToLuma({ confidence: [0, 0.3, 0.5, 0.7, 1] });
		expect(Array.from(luma)).toEqual([0, 0, 128, 255, 255]);
	});

	test("frames are blurred 1-2-1 over time with one frame of delay", () => {
		const smoother = new TemporalSmoother();
		const frames = [0, 100, 200, 100].map((v) => new Uint8Array([v]));
		const out = frames.map((f) => smoother.push(f));
		out.push(smoother.flush());
		expect(out[0]).toBeNull();
		expect(out.slice(1).map((f) => f?.[0])).toEqual([25, 100, 150, 125]);
		expect(smoother.flush()).toBeNull();
	});

	test("kept share counts the bright half", () => {
		expect(keptShare({ luma: new Uint8Array([0, 127, 128, 255]) })).toBe(0.5);
	});
});
