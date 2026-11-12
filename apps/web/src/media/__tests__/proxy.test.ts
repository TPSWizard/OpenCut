import { describe, expect, test } from "vitest";
import { isPreviewQuality, previewDecodeSize, proxySize, wantsProxy } from "../proxy";

describe("proxy sizes", () => {
	test("fit the long edge and stay even, like the desktop", () => {
		expect(proxySize({ width: 3840, height: 2160 })).toEqual({ width: 960, height: 540 });
		expect(proxySize({ width: 2160, height: 3840 })).toEqual({ width: 540, height: 960 });
		expect(proxySize({ width: 1921, height: 1081 })).toEqual({ width: 960, height: 540 });
		expect(proxySize({ width: 640, height: 361 })).toEqual({ width: 640, height: 360 });
	});

	test("only videos above 1080p get one automatically", () => {
		expect(wantsProxy({ width: 3840, height: 2160 })).toBe(true);
		expect(wantsProxy({ width: 1920, height: 1080 })).toBe(false);
		expect(wantsProxy({})).toBe(false);
	});
});

describe("preview decode size", () => {
	const canvas = { width: 1920, height: 1080 };

	test("auto fits the source inside the canvas", () => {
		expect(previewDecodeSize({ sourceWidth: 3840, sourceHeight: 2160, canvas, quality: "auto" })).toEqual({
			width: 1920,
			height: 1080,
		});
		expect(previewDecodeSize({ sourceWidth: 1280, sourceHeight: 720, canvas, quality: "auto" })).toBeNull();
	});

	test("half and quarter scale that down; full keeps every pixel", () => {
		expect(previewDecodeSize({ sourceWidth: 3840, sourceHeight: 2160, canvas, quality: "half" })).toEqual({
			width: 960,
			height: 540,
		});
		expect(previewDecodeSize({ sourceWidth: 1280, sourceHeight: 720, canvas, quality: "quarter" })).toEqual({
			width: 320,
			height: 180,
		});
		expect(previewDecodeSize({ sourceWidth: 3840, sourceHeight: 2160, canvas, quality: "full" })).toBeNull();
	});

	test("sizes move in steps and never go below 16 px", () => {
		const a = previewDecodeSize({ sourceWidth: 3840, sourceHeight: 2160, canvas: { width: 1000, height: 562 }, quality: "auto" });
		const b = previewDecodeSize({ sourceWidth: 3840, sourceHeight: 2160, canvas: { width: 1010, height: 568 }, quality: "auto" });
		expect(a).toEqual(b);
		expect(previewDecodeSize({ sourceWidth: 40, sourceHeight: 20, canvas: { width: 4, height: 2 }, quality: "quarter" })).toEqual({
			width: 16,
			height: 16,
		});
		expect(isPreviewQuality("half")).toBe(true);
		expect(isPreviewQuality("8k")).toBe(false);
	});
});
