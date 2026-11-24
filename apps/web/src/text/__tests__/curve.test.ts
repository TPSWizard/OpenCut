import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { bendText, bentTextSize } from "opencut-wasm";
import { bentRect, curveAt, isCurved, readCurve } from "../curve";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/curved-text/cases.json"), "utf8")) as {
	sizes: { name: string; curve: number; width: number; height: number; bent: [number, number] }[];
};

describe("curved text through WASM", () => {
	it.each(cases.sizes.map((c) => [c.name, c] as const))("sizes %s like the reference", (_, c) => {
		const [width, height] = bentTextSize(c.curve, c.width, c.height);
		expect(Math.abs(width - c.bent[0])).toBeLessThanOrEqual(2);
		expect(Math.abs(height - c.bent[1])).toBeLessThanOrEqual(2);
	});

	it("reads the param and keeps flat text flat", () => {
		expect(readCurve(undefined)).toBe(0);
		expect(readCurve(250)).toBe(100);
		expect(readCurve(-30)).toBe(-30);
		expect(isCurved(0.2)).toBe(false);
		const rect = { left: -100, top: -20, width: 200, height: 40 };
		expect(bentRect(rect, 0)).toBe(rect);
		const bent = bentRect(rect, 50);
		expect(bent.left + bent.width / 2).toBeCloseTo(0, 6);
		expect(bent.height).toBeGreaterThan(rect.height * 2);
		expect(bendText(0, 4, 4, new Uint8Array(64))).toHaveLength(0);
	});

	it("bends pixels: an arch drops the ends", () => {
		const width = 200;
		const height = 30;
		const flat = new Uint8Array(width * height * 4);
		for (let i = 0; i < width * height; i++) flat.set([255, 0, 0, 255], i * 4);
		const [bw, bh] = bentTextSize(50, width, height);
		const bent = bendText(50, width, height, flat);
		expect(bent).toHaveLength(bw * bh * 4);
		const alphaAt = (x: number, y: number) => bent[(y * bw + x) * 4 + 3];
		// The middle column is filled around the centre; the top corners are empty.
		expect(alphaAt(Math.floor(bw / 2), Math.floor(bh / 2))).toBe(255);
		expect(alphaAt(2, 2)).toBe(0);
		expect(alphaAt(bw - 3, 2)).toBe(0);
	});

	it("follows curve keyframes", () => {
		const animations = {
			curve: {
				keys: [
					{ id: "a", time: 0, value: 0, segmentToNext: "linear", tangentMode: "auto" },
					{ id: "b", time: 100, value: 60, segmentToNext: "linear", tangentMode: "auto" },
				],
			},
		};
		expect(curveAt({ params: { curve: 0 }, animations: animations as never, localTime: 50 })).toBeCloseTo(30, 6);
		expect(curveAt({ params: { curve: 20 }, animations: undefined, localTime: 50 })).toBe(20);
	});
});
