import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Audiogram, audiogramAnalyse } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { audiogramGraphicDefinition } from "@/graphics/definitions/audiogram";
import { audiogramSourceTimes, getAudiogramConstants, getAudiogramStyles } from "../audiogram";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/audiogram/cases.json"), "utf8")) as {
	analysis: Array<{ name: string; sample_rate: number; samples: number[]; times: number[]; bands: number; fps: number; levels: number[] }>;
	renders: Array<{ name: string; levels: string; params: Record<string, unknown>; seconds: number; width: number; height: number; alpha: number[] }>;
};

function decodeLevels(text: string): number[] {
	return [...atob(text.split(":")[3])].map((c) => c.charCodeAt(0));
}

describe("audiogram", () => {
	it("analyses sound as the reference does, through WASM", () => {
		const { fps, bands } = getAudiogramConstants();
		const cases = fixture.analysis.filter((item) => item.bands === bands && item.fps === fps);
		expect(cases.length).toBeGreaterThanOrEqual(1);
		for (const item of cases) {
			const text = audiogramAnalyse(new Float32Array(item.samples), item.sample_rate, new Float64Array(item.times));
			expect(text.startsWith(`ag1:${fps}:${bands}:`)).toBe(true);
			const got = decodeLevels(text);
			expect(got.length).toBe(item.levels.length);
			const worst = Math.max(...got.map((value, i) => Math.abs(value - item.levels[i])));
			expect(worst, item.name).toBeLessThanOrEqual(1);
		}
	});

	it("draws every style as the reference does", () => {
		for (const item of fixture.renders) {
			const audiogram = new Audiogram(item.levels);
			const rgba = audiogram.render(JSON.stringify(item.params), item.seconds, item.width, item.height);
			audiogram.free();
			expect(rgba.length).toBe(item.width * item.height * 4);
			let worst = 0;
			for (let i = 0; i < item.alpha.length; i++) worst = Math.max(worst, Math.abs(rgba[i * 4 + 3] - item.alpha[i]));
			expect(worst, item.name).toBeLessThanOrEqual(1);
		}
	});

	it("maps each frame to its source time and sizes the raster", () => {
		const times = audiogramSourceTimes({ trimStart: 5, duration: 1 });
		expect(times.length).toBe(30);
		expect(times[0]).toBeCloseTo(5);
		expect(times[29]).toBeCloseTo(5 + 29 / 30);
		// Capped at ten minutes.
		expect(audiogramSourceTimes({ trimStart: 0, duration: 3600 }).length).toBe(600 * 30);
		expect(audiogramGraphicDefinition.sourceSize?.({ width: 3840, height: 2160 })).toEqual({ width: 1920, height: 1080 });
		expect(audiogramGraphicDefinition.timedParams?.({}, 1.5)).toEqual({ __seconds: 1.5 });
		const { styles, defaults } = getAudiogramStyles();
		expect(styles.map((style) => style.id)).toEqual(["bars", "mirror", "line", "circle"]);
		expect(defaults.style).toBe("mirror");
		for (const param of audiogramGraphicDefinition.params) expect(defaults[param.key], param.key).toBe(param.default);
	});
});
