import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { bendTextAlongPath, textPathPoints, textPathSize } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { bentRect, textPathAt } from "../curve";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/text-path/cases.json"), "utf8")) as {
	width: number;
	height: number;
	flat: number[];
	cases: { preset: string; amount: number; custom: string; points: number[][]; width: number; height: number; rgba: number[] }[];
};

describe("text on a path through WASM", () => {
	it("lays text along each shape as the reference (and the desktop)", () => {
		const flat = new Uint8Array(fixture.flat);
		for (const item of fixture.cases) {
			const name = `${item.preset} ${item.amount}`;
			const points = textPathPoints(item.preset, item.amount, item.custom);
			expect(Array.from(points), name).toEqual(item.points.flat().map((value) => expect.closeTo(value, 12)));
			expect(Array.from(textPathSize(points, fixture.width, fixture.height)), name).toEqual([item.width, item.height]);
			const laid = bendTextAlongPath(points, fixture.width, fixture.height, flat);
			let worst = 0;
			laid.forEach((value, index) => {
				worst = Math.max(worst, Math.abs(value - item.rgba[index]));
			});
			expect(worst, name).toBeLessThanOrEqual(1);
		}
	});

	it("reads the path params and sizes the bounds from it", () => {
		expect(textPathAt({ textPath: "none" })).toBeNull();
		expect(textPathAt({})).toBeNull();
		expect(textPathAt({ textPath: "custom", textPathPoints: "[[0,0],[1,1]]" })).toBeNull();
		const wave = textPathAt({ textPath: "wave", textPathAmount: 60 });
		expect(wave?.length).toBe(8);
		const rect = bentRect({ left: -24, top: -6, width: 48, height: 12 }, 0, wave);
		expect([rect.width, rect.height]).toEqual([60, 22]);
		expect(rect.left + rect.width / 2).toBe(0);
	});
});
