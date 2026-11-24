import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
	CORNER_PIN_ENABLED,
	CORNER_PIN_KEYS,
	type CornerPin,
	boxToCanvas,
	canvasToBox,
	cornerPinAt,
	cornerPinParams,
	readCornerPin,
	resetCornerPinParams,
} from "../pin";
import type { ParamValues } from "@/params";

type Point = [number, number];
const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/corner-pin/cases.json"), "utf8")) as {
	read: { name: string; params: Record<string, unknown>; corners: Point[] | null }[];
	canvas: { bounds: { cx: number; cy: number; width: number; height: number; rotation: number }; corners: Point[]; canvas: Point[] }[];
};

describe("corner pin params", () => {
	it.each(cases.read.map((c) => [c.name, c] as const))("%s", (_, c) => {
		const got = readCornerPin(c.params as ParamValues);
		if (c.corners === null) {
			expect(got).toBeNull();
		} else {
			expect(got).not.toBeNull();
			got!.forEach((point, i) => {
				expect(point[0]).toBeCloseTo(c.corners![i][0], 9);
				expect(point[1]).toBeCloseTo(c.corners![i][1], 9);
			});
		}
	});

	it("places the corners on the canvas and back", () => {
		for (const c of cases.canvas) {
			c.corners.forEach((corner, i) => {
				const got = boxToCanvas(c.bounds, corner);
				expect(got[0]).toBeCloseTo(c.canvas[i][0], 6);
				expect(got[1]).toBeCloseTo(c.canvas[i][1], 6);
				const back = canvasToBox(c.bounds, got);
				expect(back[0]).toBeCloseTo(corner[0], 9);
				expect(back[1]).toBeCloseTo(corner[1], 9);
			});
		}
	});

	it("writes params that read back, and resets", () => {
		const corners: CornerPin = [[0.1, 0.05], [0.85, 0.15], [0.9, 0.8], [0.05, 0.95]];
		const params = { [CORNER_PIN_ENABLED]: true, ...cornerPinParams(corners) };
		expect(readCornerPin(params)).toEqual(corners);
		expect(CORNER_PIN_KEYS).toHaveLength(8);
		expect(readCornerPin(resetCornerPinParams(true))).toBeNull();
		expect(resetCornerPinParams(true)["cornerPin.bottomRightY"]).toBe(100);
	});

	it("follows the corners' keyframes", () => {
		const params: ParamValues = { [CORNER_PIN_ENABLED]: true, "cornerPin.topLeftX": 0 };
		const animations = {
			"cornerPin.topLeftX": {
				keys: [
					{ id: "a", time: 0, value: 0, segmentToNext: "linear", tangentMode: "auto" },
					{ id: "b", time: 100, value: 40, segmentToNext: "linear", tangentMode: "auto" },
				],
			},
		};
		const half = cornerPinAt({ params, animations: animations as never, localTime: 50 });
		expect(half?.[0][0]).toBeCloseTo(0.2, 6);
		expect(cornerPinAt({ params, animations: animations as never, localTime: 0 })).toBeNull();
	});
});
