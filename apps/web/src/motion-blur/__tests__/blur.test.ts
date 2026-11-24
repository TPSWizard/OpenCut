import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import type { Transform } from "@/rendering";
import {
	canvasQuadFor,
	motionBlurHalfShutter,
	motionTransforms,
	readMotionBlur,
} from "../blur";

const root = resolve(__dirname, "../../../../..");
const cases = JSON.parse(readFileSync(resolve(root, "fixtures/motion-blur/cases.json"), "utf8")) as {
	shutter: { amount: number; fpsNumerator: number; fpsDenominator: number; halfSeconds: number | null }[];
};

const at = (x: number, y = 0, scale = 1, rotate = 0): Transform => ({
	position: { x, y },
	scaleX: scale,
	scaleY: scale,
	rotate,
});

describe("motion blur shutter", () => {
	it.each(cases.shutter.map((c) => [`${c.amount} at ${c.fpsNumerator}/${c.fpsDenominator}`, c] as const))(
		"%s",
		(_, c) => {
			const got = motionBlurHalfShutter(c.amount, { numerator: c.fpsNumerator, denominator: c.fpsDenominator });
			if (c.halfSeconds === null) {
				expect(got).toBeNull();
			} else {
				expect(got).toBeCloseTo(c.halfSeconds, 12);
			}
		},
	);

	it("reads the param, clamped, 0 when missing", () => {
		expect(readMotionBlur(undefined)).toBe(0);
		expect(readMotionBlur({ motionBlur: 40 })).toBe(40);
		expect(readMotionBlur({ motionBlur: 400 })).toBe(100);
		expect(readMotionBlur({ motionBlur: "x" })).toBe(0);
	});
});

describe("motion transforms", () => {
	const fps = { numerator: 30, denominator: 1 };
	const ticksPerSecond = 120_000;
	// 150 px a second along x.
	const moving = (localTime: number) => at((150 * localTime) / ticksPerSecond);

	it("looks half a shutter either side", () => {
		const motion = motionTransforms({ amount: 100, fps, ticksPerSecond, localTime: ticksPerSecond, transformAt: moving });
		expect(motion?.from.position.x).toBeCloseTo(147.5, 9);
		expect(motion?.to.position.x).toBeCloseTo(152.5, 9);
	});

	it("is off at 0 and for a clip that holds still", () => {
		expect(motionTransforms({ amount: 0, fps, ticksPerSecond, localTime: 0, transformAt: moving })).toBeNull();
		expect(motionTransforms({ amount: 100, fps, ticksPerSecond, localTime: 0, transformAt: () => at(5) })).toBeNull();
	});
});

describe("canvas quad for text", () => {
	const size = { width: 640, height: 360 };

	it("is the canvas itself when nothing changes", () => {
		const quad = canvasQuadFor({ drawn: at(40, -20, 2, 15), other: at(40, -20, 2, 15), ...size });
		expect(quad?.centerX).toBeCloseTo(320, 9);
		expect(quad?.centerY).toBeCloseTo(180, 9);
		expect(quad?.width).toBeCloseTo(640, 9);
		expect(quad?.rotationDegrees).toBeCloseTo(0, 9);
	});

	it("moves with the text", () => {
		const quad = canvasQuadFor({ drawn: at(0), other: at(30, 10), ...size });
		expect(quad?.centerX).toBeCloseTo(350, 9);
		expect(quad?.centerY).toBeCloseTo(190, 9);
	});

	it("turns and scales about the text's anchor", () => {
		// Anchor at (420, 180); the canvas centre is 100 px to its left.
		const quad = canvasQuadFor({ drawn: at(100), other: at(100, 0, 2, 90), ...size });
		expect(quad?.centerX).toBeCloseTo(420, 9);
		expect(quad?.centerY).toBeCloseTo(180 - 200, 9);
		expect(quad?.width).toBeCloseTo(1280, 9);
		expect(quad?.rotationDegrees).toBeCloseTo(90, 9);
	});
});
