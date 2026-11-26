import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { unzipSync } from "fflate";
import { describe, expect, it } from "vitest";
import { encodeGif, gifFrameCount, gifFrameSize, paletteStep, sequenceFileName, zipFrames } from "../kinds";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/gif/cases.json"), "utf8")) as {
	cases: {
		name: string;
		width: number;
		height: number;
		fps: number;
		dither: boolean;
		colours: number;
		sampleEvery: number;
		frames: string[];
		gif: string;
	}[];
};

describe("GIF export through the shared crate", () => {
	it("writes the reference's bytes (and the desktop's)", async () => {
		for (const item of fixture.cases) {
			const frames = item.frames.map((frame) => new Uint8Array(Buffer.from(frame, "base64")));
			const progress: number[] = [];
			const gif = await encodeGif({
				frameCount: frames.length,
				fps: item.fps,
				width: item.width,
				height: item.height,
				dither: item.dither,
				colours: item.colours,
				sampleEvery: item.sampleEvery,
				frameAt: async (index) => frames[index],
				onProgress: (value) => progress.push(value),
			});
			expect(Buffer.from(gif ?? []).toString("base64"), item.name).toBe(item.gif);
			expect(progress.at(-1), item.name).toBe(1);
		}
	});

	it("stops when cancelled", async () => {
		const frame = new Uint8Array(4 * 4 * 4);
		let calls = 0;
		const gif = await encodeGif({
			frameCount: 10,
			fps: 10,
			width: 4,
			height: 4,
			dither: false,
			frameAt: async () => frame,
			isCancelled: () => ++calls > 3,
		});
		expect(gif).toBeNull();
	});

	it("sizes, counts and names like the desktop", () => {
		expect(gifFrameSize({ canvasWidth: 1920, canvasHeight: 1080, width: 480 })).toEqual([480, 270]);
		expect(gifFrameSize({ canvasWidth: 1080, canvasHeight: 1920, width: 480 })).toEqual([480, 852]);
		expect(gifFrameCount({ seconds: 2, fps: 15 })).toBe(30);
		expect(paletteStep(30)).toBe(1);
		expect(paletteStep(300)).toBe(8);
		expect(sequenceFileName(12)).toBe("frame_000012.png");
		const zip = unzipSync(zipFrames([new Uint8Array([1, 2]), new Uint8Array([3])]));
		expect(Object.keys(zip)).toEqual(["frame_000001.png", "frame_000002.png"]);
		expect([...zip["frame_000002.png"]]).toEqual([3]);
	});
});
