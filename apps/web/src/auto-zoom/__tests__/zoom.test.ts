import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { formatPunches, parsePunches, planPunches, readClipZoom, zoomAt, zoomTransform, type ZoomStrength, type ZoomWord } from "../zoom";

const root = resolve(__dirname, "../../../../..");

describe("auto zoom", () => {
	it("plans the shared cases like the desktop", () => {
		const { cases } = JSON.parse(readFileSync(resolve(root, "fixtures/auto-zoom/cases.json"), "utf8")) as {
			cases: { name: string; strength: ZoomStrength; duration: number; words: ZoomWord[]; punches: string }[];
		};
		expect(cases.length).toBeGreaterThan(3);
		for (const { name, strength, duration, words, punches } of cases) {
			expect(formatPunches(planPunches({ words, duration, strength })), name).toBe(punches);
		}
	});

	it("parses, skips junk and reads defaults", () => {
		expect(parsePunches("2-3@1.2; nope ;0.5-1@1.1;4-3@2")).toEqual([
			{ start: 0.5, end: 1, scale: 1.1 },
			{ start: 2, end: 3, scale: 1.2 },
		]);
		expect(readClipZoom({})).toBeNull();
		expect(readClipZoom({ "zoom.punches": "0-1@1.5", "zoom.focusX": 3 })).toEqual({
			punches: [{ start: 0, end: 1, scale: 1.5 }],
			focusX: 0.5,
			focusY: -0.1,
			ramp: 0.15,
		});
	});

	it("eases in and out over the ramp", () => {
		const zoom = { punches: [{ start: 1, end: 3, scale: 1.4 }], focusX: 0, focusY: 0, ramp: 0.2 };
		expect(zoomAt({ zoom, seconds: 0.99 })).toBe(1);
		expect(zoomAt({ zoom, seconds: 1.1 })).toBeCloseTo(1.2, 6);
		expect(zoomAt({ zoom, seconds: 2 })).toBeCloseTo(1.4, 6);
		expect(zoomAt({ zoom, seconds: 2.9 })).toBeCloseTo(1.2, 6);
		expect(zoomAt({ zoom, seconds: 3 })).toBe(1);
		expect(zoomAt({ zoom: { ...zoom, ramp: 0 }, seconds: 1 })).toBe(1.4);
	});

	it("keeps the focus point still", () => {
		const zoom = { punches: [], focusX: 0.25, focusY: -0.1, ramp: 0 };
		const { offsetX, offsetY } = zoomTransform({ zoom, scale: 2, width: 400, height: 200, rotate: 0 });
		// The focus sits 100 px right and 20 px up of the centre; doubled, it would move as far again.
		expect([offsetX, offsetY]).toEqual([-100, 20]);
		const turned = zoomTransform({ zoom, scale: 2, width: 400, height: 200, rotate: 90 });
		expect(turned.offsetX).toBeCloseTo(-20, 6);
		expect(turned.offsetY).toBeCloseTo(-100, 6);
	});
});
