import { describe, expect, test } from "vitest";
import type { MediaAsset } from "@/media/types";
import { previewVideoSource } from "../scene-builder";

const original = new File(["original"], "big.mp4", { type: "video/mp4" });
const proxy = new File(["proxy"], "big.proxy.mp4", { type: "video/mp4" });
const canvas = { width: 1920, height: 1080 };

function asset(extra: Partial<MediaAsset> = {}): MediaAsset {
	return { id: "m", name: "big.mp4", type: "video", file: original, width: 3840, height: 2160, ...extra };
}

describe("preview video source", () => {
	test("proxies are decoded at their own size when they fit", () => {
		const source = previewVideoSource({
			asset: asset({ proxyFile: proxy, proxyWidth: 960, proxyHeight: 540 }),
			canvasSize: canvas,
			options: { quality: "auto", useProxies: true },
		});
		expect(source?.file).toBe(proxy);
		expect(source?.decodeKey).toBe("m|proxy@native");
		expect(source?.decodeSize).toBeUndefined();
	});

	test("without a proxy the original is decoded at the fitted size", () => {
		const source = previewVideoSource({ asset: asset(), canvasSize: canvas, options: { quality: "auto", useProxies: true } });
		expect(source?.file).toBe(original);
		expect(source?.decodeSize).toEqual({ width: 1920, height: 1080 });
		expect(source?.decodeKey).toBe("m|original@1920x1080");
	});

	test("proxies off and full quality leave the original untouched", () => {
		expect(
			previewVideoSource({
				asset: asset({ proxyFile: proxy, proxyWidth: 960, proxyHeight: 540 }),
				canvasSize: canvas,
				options: { quality: "full", useProxies: false },
			}),
		).toBeNull();
	});

	test("lower quality scales the proxy too", () => {
		const source = previewVideoSource({
			asset: asset({ proxyFile: proxy, proxyWidth: 960, proxyHeight: 540 }),
			canvasSize: canvas,
			options: { quality: "quarter", useProxies: true },
		});
		expect(source?.decodeSize).toEqual({ width: 240, height: 134 });
		expect(source?.decodeKey).toBe("m|proxy@240x134");
	});
});
