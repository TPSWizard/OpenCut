import { describe, expect, it, vi } from "vitest";

// A 30 fps clip as mediabunny reports it: frame k starts at k/30 s and lasts
// 1/30 s. In floats, frame k's end (start + duration) can sit a hair past
// frame k+1's start, which used to make a time exactly on a frame boundary
// return the frame before.
const FPS = 30;
const FRAME_COUNT = 90;
const frameAt = (index: number) => ({
	canvas: { width: 320, height: 180 },
	timestamp: (index * 4000) / 120000,
	duration: 4000 / 120000,
});

vi.mock("mediabunny", () => {
	class CanvasSink {
		async *canvases(start = 0) {
			// Like the decoder: begin at the frame whose span holds `start`.
			let index = 0;
			while (index < FRAME_COUNT - 1) {
				const frame = frameAt(index);
				if (start >= frame.timestamp && start < frame.timestamp + frame.duration) {
					break;
				}
				index++;
			}
			for (; index < FRAME_COUNT; index++) {
				// Decoding takes a moment, so the prefetch is still running
				// when the next frame is asked for (as in the browser).
				await new Promise((resolve) => setTimeout(resolve, 1));
				yield frameAt(index);
			}
		}
	}
	class Input {
		async getPrimaryVideoTrack() {
			return { canDecode: async () => true };
		}
		dispose() {}
	}
	return { Input, CanvasSink, BlobSource: class {}, ALL_FORMATS: [] };
});

const { VideoCache } = await import("./service");

const file = new File([], "clip.mp4");
/** Frame k's time as the renderer asks for it (timeline ticks / 120000). */
const tickTime = (index: number) => (index * 4000) / 120000;
const indexOf = (timestamp: number) => Math.round(timestamp * FPS);

describe("VideoCache frame boundaries", () => {
	it("returns the frame that starts at an exact frame time, frame by frame", async () => {
		const cache = new VideoCache();
		const wrong: string[] = [];
		for (let k = 30; k < 75; k++) {
			const frame = await cache.getFrameAt({ mediaId: "m", file, time: tickTime(k) });
			if (!frame || indexOf(frame.timestamp) !== k) {
				wrong.push(`${k} -> ${frame ? indexOf(frame.timestamp) : "null"}`);
			}
		}
		expect(wrong).toEqual([]);
	});

	it("returns the right frame after a seek to an exact frame time", async () => {
		// Frames whose float end overshoots the next frame's start.
		for (const k of [32, 34, 47, 59]) {
			const cache = new VideoCache();
			const frame = await cache.getFrameAt({ mediaId: "m", file, time: tickTime(k) });
			expect(frame && indexOf(frame.timestamp)).toBe(k);
		}
	});

	it("still returns the current frame for times inside it", async () => {
		const cache = new VideoCache();
		for (let k = 30; k < 60; k++) {
			for (const part of [0.5, 0.99]) {
				const frame = await cache.getFrameAt({
					mediaId: "m",
					file,
					time: (k + part) / FPS,
				});
				expect(frame && indexOf(frame.timestamp)).toBe(k);
			}
		}
	});
});
