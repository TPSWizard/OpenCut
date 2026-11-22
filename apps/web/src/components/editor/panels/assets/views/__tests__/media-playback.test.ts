import { describe, expect, it, vi } from "vitest";
import { claimPlayback, releasePlayback, scrubTime } from "../media-playback";

describe("media panel playback", () => {
	it("maps the hover position across the card to a time in the media", () => {
		expect(scrubTime({ x: 0, width: 112, duration: 10 })).toBe(0);
		expect(scrubTime({ x: 56, width: 112, duration: 10 })).toBe(5);
		expect(scrubTime({ x: 112, width: 112, duration: 10 })).toBeCloseTo(9.95, 9);
		expect(scrubTime({ x: -20, width: 112, duration: 10 })).toBe(0);
		expect(scrubTime({ x: 50, width: 0, duration: 10 })).toBe(0);
		expect(scrubTime({ x: 50, width: 100, duration: Number.NaN })).toBe(0);
	});

	it("plays one preview at a time", () => {
		const first = vi.fn();
		const second = vi.fn();
		claimPlayback(first);
		claimPlayback(second);
		expect(first).toHaveBeenCalledTimes(1);
		expect(second).not.toHaveBeenCalled();
		releasePlayback(second);
		claimPlayback(first);
		expect(second).not.toHaveBeenCalled();
		releasePlayback(first);
	});
});
