import { describe, expect, it } from "vitest";
import {
	PLATFORM_PRESETS,
	checkPlatformExport,
	estimatePlatformFileBytes,
	getPlatformPreset,
	h264HighCodecString,
	platformEncodeSettings,
	recommendedVideoBitrate,
} from "../catalogue";

describe("recommendedVideoBitrate", () => {
	it("follows YouTube's SDR table by the shorter side", () => {
		expect(
			recommendedVideoBitrate({ canvas: { width: 1920, height: 1080 }, fps: 30 }),
		).toBe(8_000_000);
		expect(
			recommendedVideoBitrate({ canvas: { width: 1080, height: 1920 }, fps: 30 }),
		).toBe(8_000_000);
		expect(
			recommendedVideoBitrate({ canvas: { width: 1920, height: 1080 }, fps: 60 }),
		).toBe(12_000_000);
		expect(
			recommendedVideoBitrate({ canvas: { width: 1280, height: 720 }, fps: 30 }),
		).toBe(5_000_000);
		expect(
			recommendedVideoBitrate({ canvas: { width: 3840, height: 2160 }, fps: 60 }),
		).toBe(60_000_000);
		expect(
			recommendedVideoBitrate({ canvas: { width: 640, height: 360 }, fps: 25 }),
		).toBe(1_000_000);
	});
});

describe("h264HighCodecString", () => {
	it("picks the lowest High-profile level that holds the frames", () => {
		expect(
			h264HighCodecString({ canvas: { width: 1920, height: 1080 }, fps: 30 }),
		).toBe("avc1.640028");
		expect(
			h264HighCodecString({ canvas: { width: 1920, height: 1080 }, fps: 60 }),
		).toBe("avc1.64002a");
		expect(
			h264HighCodecString({ canvas: { width: 1280, height: 720 }, fps: 30 }),
		).toBe("avc1.64001f");
		expect(
			h264HighCodecString({ canvas: { width: 3840, height: 2160 }, fps: 30 }),
		).toBe("avc1.640033");
	});
});

describe("platformEncodeSettings", () => {
	it("uses a half-second closed GOP and 384 kbps audio", () => {
		const settings = platformEncodeSettings({
			canvas: { width: 1080, height: 1920 },
			fps: 30,
		});
		expect(settings).toEqual({
			videoBitrate: 8_000_000,
			audioBitrate: 384_000,
			keyFrameIntervalSeconds: 0.5,
			codec: "avc1.640028",
		});
	});
});

describe("checkPlatformExport", () => {
	const shorts = getPlatformPreset({ id: "youtube-shorts" })!;

	it("passes a project that matches the preset", () => {
		expect(
			checkPlatformExport({
				preset: shorts,
				canvas: { width: 1080, height: 1920 },
				fps: 30,
				durationSeconds: 45,
			}),
		).toEqual([]);
	});

	it("offers the preset canvas and frame rate as fixes", () => {
		const issues = checkPlatformExport({
			preset: shorts,
			canvas: { width: 1920, height: 1080 },
			fps: 60,
			durationSeconds: 45,
		});
		expect(issues.map((issue) => issue.kind)).toEqual(["canvas", "fps"]);
		expect(issues[0].fix).toEqual({ canvas: { width: 1080, height: 1920 } });
		expect(issues[0].message).toContain("16:9");
		expect(issues[0].message).toContain("9:16");
		expect(issues[1].fix).toEqual({ fps: 30 });
		expect(issues.every((issue) => issue.severity === "warning")).toBe(true);
	});

	it("flags a timeline longer than the platform allows", () => {
		const issues = checkPlatformExport({
			preset: shorts,
			canvas: { width: 1080, height: 1920 },
			fps: 30,
			durationSeconds: 200,
		});
		expect(issues).toHaveLength(1);
		expect(issues[0]).toMatchObject({ kind: "duration", severity: "error" });
		expect(issues[0].message).toContain("3 min 20 s");
		expect(issues[0].message).toContain("3 min");
	});

	it("flags a file over the platform's size cap", () => {
		const x = getPlatformPreset({ id: "x" })!;
		// 140 s at 5.4 Mbps is about 94 MB: fine. 4K60 pushes it past 512 MB.
		expect(
			checkPlatformExport({
				preset: x,
				canvas: { width: 1280, height: 720 },
				fps: 30,
				durationSeconds: 140,
			}),
		).toEqual([]);
		const issues = checkPlatformExport({
			preset: x,
			canvas: { width: 3840, height: 2160 },
			fps: 60,
			durationSeconds: 140,
		});
		expect(issues.map((issue) => issue.kind)).toEqual(["canvas", "fps", "size"]);
		expect(issues[2].message).toContain("512 MB");
	});

	it("estimates the file from the video and audio bitrates", () => {
		expect(
			estimatePlatformFileBytes({
				canvas: { width: 1920, height: 1080 },
				fps: 30,
				durationSeconds: 10,
				includeAudio: true,
			}),
		).toBe(Math.round(((8_000_000 + 384_000) * 10) / 8));
	});
});

describe("PLATFORM_PRESETS", () => {
	it("keeps every safe area inside the frame", () => {
		for (const preset of PLATFORM_PRESETS) {
			if (!preset.safeArea) continue;
			const { top, bottom, left, right } = preset.safeArea;
			expect(top + bottom).toBeLessThan(1);
			expect(left + right).toBeLessThan(1);
			expect(preset.canvas.height).toBeGreaterThan(preset.canvas.width);
		}
	});
});
