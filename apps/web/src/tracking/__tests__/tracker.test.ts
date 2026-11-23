import { describe, expect, it } from "vitest";
import { followKeys, PointTracker } from "opencut-wasm";

const W = 160;
const H = 90;

/** Noisy background with a checkered square of 16 px centred at (`x`, `y`). */
function frame(x: number, y: number, seed: number): Uint8Array {
	const rgba = new Uint8Array(W * H * 4);
	let state = (seed * 2654435761) >>> 0 || 1;
	for (let py = 0; py < H; py++) {
		for (let px = 0; px < W; px++) {
			state ^= state << 13;
			state >>>= 0;
			state ^= state >>> 17;
			state ^= state << 5;
			state >>>= 0;
			let value = 60 + (state % 40);
			if (Math.abs(px - x) <= 8 && Math.abs(py - y) <= 8) value = (Math.floor((px - x + 100) / 4) + Math.floor((py - y + 100) / 4)) % 2 ? 230 : 160;
			const index = (py * W + px) * 4;
			rgba[index] = value;
			rgba[index + 1] = value;
			rgba[index + 2] = value;
			rgba[index + 3] = 255;
		}
	}
	return rgba;
}

describe("motion tracking through WASM", () => {
	it("follows a moving square like the desktop", () => {
		const tracker = new PointTracker(frame(40, 40, 1), W, H, 40, 40, 16, 16);
		expect(Array.from(tracker.position())).toEqual([40, 40]);
		for (let i = 1; i <= 20; i++) {
			const [x, y, score, lost] = tracker.step(frame(40 + i * 3, 40 + i, i + 1));
			expect(lost, `frame ${i}`).toBe(0);
			expect(score).toBeGreaterThan(0.5);
			expect(Math.abs(x - (40 + i * 3))).toBeLessThanOrEqual(1);
			expect(Math.abs(y - (40 + i))).toBeLessThanOrEqual(1);
		}
		tracker.free();
	});

	it("refuses a flat spot", () => {
		const flat = new Uint8Array(W * H * 4).fill(128);
		expect(() => new PointTracker(flat, W, H, 80, 45, 16, 16)).toThrow(/nothing to follow/);
	});

	it("turns the tracked path into follower keys on the canvas", () => {
		const keys = followKeys(
			new Float64Array([0, 0.5, 1]),
			new Float64Array([0.5, 0.625, 0.75]),
			new Float64Array([0.5, 0.5, 0.5]),
			0,
			0,
			1920,
			1080,
			10,
			-5,
			1,
		);
		// A straight run keeps only its ends.
		expect(Array.from(keys)).toEqual([0, 10, -5, 1, 490, -5]);
	});
});
