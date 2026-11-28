import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { StabilisePath, stabiliseEncodePath, stabiliseMeasureRate, stabiliseMeasureSize, stabiliseStep } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import {
	STABILISE_PATH_PARAM,
	STABILISE_STRENGTHS,
	STABILISE_STRENGTH_PARAM,
	readStabilisePath,
	readStabiliseStrength,
	stabiliseParams,
	stabiliseZoom,
	steadyCorrectionAt,
	steadyTransform,
} from "../steady";

const root = resolve(__dirname, "../../../../..");
// biome-ignore lint/suspicious/noExplicitAny: fixture JSON
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/stabilise/stabilise.json"), "utf8")) as any;

const near = (a: ArrayLike<number>, b: ArrayLike<number>, tolerance: number) => {
	expect(a.length).toBe(b.length);
	for (let i = 0; i < a.length; i++) expect(Math.abs(a[i] - b[i])).toBeLessThan(tolerance);
};

describe("stabilise", () => {
	it("measures frame steps as the reference does, through WASM", () => {
		for (const c of fixture.steps) {
			const got = stabiliseStep(new Uint8Array(c.p), new Uint8Array(c.q), c.width, c.height);
			if (c.step === null) expect(got.length).toBe(0);
			else near(got, c.step, 1e-6);
		}
	});

	it("encodes the path and corrects frames as the reference does", () => {
		const path = fixture.path;
		const flat: number[] = [];
		for (const step of path.steps) flat.push(...(step ?? [Number.NaN, Number.NaN, Number.NaN, Number.NaN]));
		const text = stabiliseEncodePath(path.media, path.start, path.rate, path.aspect, new Float64Array(flat));
		expect(text).toBe(path.encoded);
		const parsed = new StabilisePath(text);
		expect(parsed.mediaId()).toBe("clip-1");
		expect(parsed.frames()).toBe(path.steps.length + 1);
		for (const lookup of path.lookups) near(parsed.correctionAt("medium", lookup.seconds), lookup.correction, 1e-9);
		// The zooms, from the unrounded steps in the reference: within the rounding.
		for (const c of path.corrections) near([parsed.zoom(c.strength)[0]], [c.zoom], 1e-4);
		parsed.free();
		expect(() => new StabilisePath("clip;0;0;1;")).toThrow();
	});

	it("reads the params, and only a path from the clip's own file", () => {
		const params = { [STABILISE_PATH_PARAM]: fixture.path.encoded, [STABILISE_STRENGTH_PARAM]: "still" };
		expect(readStabiliseStrength(params)).toBe("still");
		expect(readStabiliseStrength({ [STABILISE_STRENGTH_PARAM]: "bogus" })).toBe("off");
		expect(readStabilisePath(params, "other")).toBeNull();
		expect(stabiliseParams(params, "clip-1")).toEqual({ strength: "still", path: fixture.path.encoded });
		expect(stabiliseParams({ ...params, [STABILISE_STRENGTH_PARAM]: "off" }, "clip-1")).toBeNull();
		const zoom = stabiliseZoom(params, "clip-1");
		expect(zoom?.zoom).toBeGreaterThan(1);
		expect(STABILISE_STRENGTHS).toEqual(["off", "light", "medium", "strong", "still"]);
		expect(stabiliseMeasureSize(1920, 1080)).toEqual(new Uint32Array([192, 108]));
		expect(stabiliseMeasureRate(0)).toBe(30);
		expect(stabiliseMeasureRate(59.94)).toBe(59.94);
	});

	it("moves the drawn frame by the correction, along the clip's own axes", () => {
		const correction = steadyCorrectionAt({
			stabilise: { strength: "medium", path: fixture.path.encoded },
			sourceSeconds: fixture.path.lookups[3].seconds,
		});
		expect(correction).not.toBeNull();
		// A tenth of a 500 px frame along its x, turned 90°: straight down.
		const moved = steadyTransform(
			{ position: { x: 10, y: 0 }, scaleX: 0.5, scaleY: 0.5, rotate: 90, extra: "kept" },
			[1.1, 0, 0.1, 0],
			1000,
		);
		expect(moved.position.x).toBeCloseTo(10, 6);
		expect(moved.position.y).toBeCloseTo(50, 6);
		expect(moved.scaleX).toBeCloseTo(0.55, 9);
		expect(moved.rotate).toBe(90);
		expect(moved.extra).toBe("kept");
		// Mirrored: the move and the turn flip with the picture.
		const mirrored = steadyTransform({ position: { x: 0, y: 0 }, scaleX: -1, scaleY: 1, rotate: 0 }, [Math.cos(0.01), Math.sin(0.01), 0.1, 0], 100);
		expect(mirrored.position.x).toBeCloseTo(-10, 6);
		expect(mirrored.rotate).toBeCloseTo((-0.01 * 180) / Math.PI, 9);
	});
});
