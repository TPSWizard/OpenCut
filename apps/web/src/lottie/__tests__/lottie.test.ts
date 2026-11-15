import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { LottieAnimation, isLottie } from "opencut-wasm";
import { hasLottieName, isLottieFile, lottieRasterSize } from "@/lottie";

const dir = join(__dirname, "../../../../../fixtures/lottie");
const cases = JSON.parse(readFileSync(join(dir, "cases.json"), "utf8"));
const bytes = (name: string) => new Uint8Array(readFileSync(join(dir, name)));

describe("Lottie through opencut-wasm (fixtures/lottie)", () => {
	it("reads both files and draws the shared pixels", () => {
		for (const file of cases.files as string[]) {
			const data = bytes(file);
			expect(isLottie(data)).toBe(true);
			const animation = new LottieAnimation(data);
			expect(animation.width).toBe(cases.info.width);
			expect(animation.height).toBe(cases.info.height);
			expect(animation.frameRate).toBe(cases.info.frameRate);
			expect(animation.inFrame).toBe(cases.info.inFrame);
			expect(animation.outFrame).toBe(cases.info.outFrame);
			expect(animation.durationSeconds).toBe(cases.info.durationSeconds);
			for (const { seconds, frame } of cases.frameAt) expect(animation.frameAt(seconds)).toBe(frame);
			const { width, height } = cases.render;
			for (const pixel of cases.pixels) {
				const rgba = animation.renderRgba(pixel.frame, width, height);
				expect(rgba.length).toBe(width * height * 4);
				const at = (pixel.y * width + pixel.x) * 4;
				const got = Array.from(rgba.slice(at, at + 4));
				const close = got.every((value, index) => Math.abs(value - pixel.rgba[index]) <= cases.tolerance);
				const bothClear = pixel.rgba[3] === 0 && got[3] <= cases.tolerance;
				expect(close || bothClear, `${file} ${pixel.note}: ${got}`).toBe(true);
			}
			animation.free();
		}
	});

	it("draws mattes, strokes and gradients like the desktop", () => {
		const features = cases.features;
		const animation = new LottieAnimation(bytes(features.file));
		const rgba = animation.renderRgba(features.frame, features.width, features.height);
		for (const pixel of features.pixels) {
			const at = (pixel.y * features.width + pixel.x) * 4;
			const got = Array.from(rgba.slice(at, at + 4));
			const close = got.every((value, index) => Math.abs(value - pixel.rgba[index]) <= features.tolerance);
			const bothClear = pixel.rgba[3] === 0 && got[3] <= features.tolerance;
			expect(close || bothClear, `${pixel.note}: ${got}`).toBe(true);
		}
		animation.free();
	});

	it("refuses the same files the desktop refuses", () => {
		for (const reject of cases.rejects) {
			expect(() => new LottieAnimation(new TextEncoder().encode(reject.json)), reject.note ?? reject.json).toThrow();
		}
	});
});

describe("Lottie helpers", () => {
	it("recognises Lottie files by name and contents", async () => {
		expect(hasLottieName({ name: "Hello.JSON" })).toBe(true);
		expect(hasLottieName({ name: "x.lottie" })).toBe(true);
		expect(hasLottieName({ name: "x.png" })).toBe(false);
		const lottie = new File([bytes("bounce.json")], "bounce.json", { type: "application/json" });
		expect(await isLottieFile({ file: lottie })).toBe(true);
		const other = new File(['{"name":"package"}'], "package.json", { type: "application/json" });
		expect(await isLottieFile({ file: other })).toBe(false);
		const renamed = new File([bytes("bounce.json")], "bounce.txt");
		expect(await isLottieFile({ file: renamed })).toBe(false);
	});

	it("draws at the shown size, within limits", () => {
		const bounds = { width: 1920, height: 1080 };
		expect(lottieRasterSize({ width: 200, height: 200, bounds })).toEqual({ width: 1080, height: 1080 });
		expect(lottieRasterSize({ width: 400, height: 200, bounds: { width: 100, height: 100 } })).toEqual({ width: 256, height: 128 });
		expect(lottieRasterSize({ width: 200, height: 100, bounds: { width: 9000, height: 9000 } })).toEqual({ width: 2048, height: 1024 });
		expect(lottieRasterSize({ width: 200, height: 100, bounds, maxSide: 1024 })).toEqual({ width: 1024, height: 512 });
	});
});
