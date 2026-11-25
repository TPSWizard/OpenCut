import { cornerKeys, maskTrackEncode, maskTrackOffset } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { withMaskTrack } from "../mask-track";

describe("tracked masks and corner pin through WASM", () => {
	it("stores a mask's motion and plays it back on its centre", () => {
		const times = Array.from({ length: 11 }, (_, i) => 1 + i * 0.1);
		const track = maskTrackEncode(new Float64Array(times), new Float64Array(times.map((_, i) => 0.3 + i * 0.01)), new Float64Array(times.map((_, i) => 0.5 - i * 0.02)), 10);
		expect(Array.from(maskTrackOffset(track, 0.5))).toEqual([0, 0]);
		const [dx, dy] = maskTrackOffset(track, 1.55);
		expect(dx).toBeCloseTo(0.055, 4);
		expect(dy).toBeCloseTo(-0.11, 4);
		const moved = withMaskTrack({ params: { centerX: 0.1, centerY: 0, track } }, 1.55 * 120000);
		expect((moved.params as { centerX: number }).centerX).toBeCloseTo(0.155, 4);
		const still = { params: { centerX: 0.1 } };
		expect(withMaskTrack(still, 120000)).toBe(still);
	});

	it("keys four corners that slide together with only their ends", () => {
		const times = Array.from({ length: 21 }, (_, i) => i / 10);
		const xs = [0, 1, 2, 3].flatMap((corner) => times.map((_, i) => 0.2 + corner * 0.1 + i * 0.01));
		const ys = [0, 1, 2, 3].flatMap(() => times.map(() => 0.4));
		const starts = [-100, -50, 100, -50, 100, 50, -100, 50];
		const flat = cornerKeys(new Float64Array(times), new Float64Array(xs), new Float64Array(ys), 0, 0, 1000, 500, new Float64Array(starts), 1);
		expect(flat.length).toBe(18);
		expect(Array.from(flat.slice(0, 9))).toEqual([0, ...starts]);
		expect(flat[9]).toBe(2);
		expect(flat[10]).toBeCloseTo(100, 9);
		expect(() => cornerKeys(new Float64Array([0]), new Float64Array([0]), new Float64Array([0]), 0, 0, 1, 1, new Float64Array(8), 1)).toThrow();
	});
});
