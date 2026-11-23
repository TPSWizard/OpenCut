import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { type CoverStyle, coverSize, coverTitle, frameScore, pickCoverTime, wrapTitle } from "../layout";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/cover/cases.json"), "utf8"));

describe("cover maker", () => {
	it("lays out titles like the shared cases", () => {
		for (const item of cases.titles as { width: number; height: number; style: CoverStyle; title: string; [key: string]: unknown }[]) {
			const out = coverTitle(item);
			expect(out.lines, item.title).toEqual(item.lines);
			expect(out.maxChars).toBe(item.maxChars);
			for (const key of ["fontPx", "fontSize", "positionY", "outlineWidth"] as const) {
				expect(out[key], `${item.title} ${key}`).toBeCloseTo(item[key] as number, 9);
			}
			expect(out.color).toBe(item.color);
			if (item.background) {
				const expected = item.background as Record<string, number | string>;
				for (const [key, value] of Object.entries(expected)) {
					if (typeof value === "number") expect(out.background?.[key as "paddingX"], key).toBeCloseTo(value, 9);
					else expect(out.background?.[key as "color"]).toBe(value);
				}
			} else {
				expect(out.background).toBeUndefined();
			}
		}
	});

	it("wraps words like the shared cases", () => {
		for (const item of cases.wrap as { text: string; maxChars: number; lines: string[] }[]) {
			expect(wrapTitle(item.text, item.maxChars), item.text).toEqual(item.lines);
		}
	});

	it("scores frames like the shared cases", () => {
		for (const item of cases.scores as { name: string; width: number; height: number; pixels: number[][]; [key: string]: unknown }[]) {
			const out = frameScore(item.pixels.flat(), item.width, item.height);
			for (const key of ["sharpness", "colourfulness", "exposure", "contrast", "score"] as const) {
				expect(out[key], `${item.name} ${key}`).toBeCloseTo(item[key] as number, 9);
			}
		}
	});

	it("picks the time like the shared cases", () => {
		for (const item of cases.pick as { duration: number; samples: { time: number; score: number }[]; time: number }[]) {
			expect(pickCoverTime(item.samples, item.duration)).toBe(item.time);
		}
		expect(pickCoverTime([], 10)).toBe(0);
	});

	it("sizes the cover shapes", () => {
		expect(coverSize("canvas", { width: 640, height: 360 })).toEqual({ width: 640, height: 360 });
		expect(coverSize("landscape", { width: 1080, height: 1920 })).toEqual({ width: 1280, height: 720 });
		expect(coverSize("portrait", { width: 1920, height: 1080 })).toEqual({ width: 1080, height: 1920 });
	});
});
