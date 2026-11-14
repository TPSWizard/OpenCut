import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
	CODECS_BY_FORMAT,
	CODEC_LABELS,
	DEFAULT_CODEC,
	MAX_BITRATE_MBPS,
	MIN_BITRATE_MBPS,
	encodableCodecs,
	mediabunnyCodec,
	parseBitrateMbps,
	parseVideoCodec,
	resolveVideoCodec,
	type ExportVideoCodec,
} from "../codecs";
import type { ExportFormat } from "../index";

const cases = JSON.parse(
	readFileSync(join(__dirname, "../../../../../fixtures/export-codecs/cases.json"), "utf8"),
) as {
	formats: Record<ExportFormat, { default: ExportVideoCodec; codecs: ExportVideoCodec[] }>;
	labels: Record<ExportVideoCodec, string>;
	resolve: Array<{ format: ExportFormat; codec: ExportVideoCodec | null; expect: ExportVideoCodec }>;
	parseCodec: Array<{ text: string; expect: ExportVideoCodec | null }>;
	bitrateMbps: { min: number; max: number; cases: Array<{ text: string; expect: number | null }> };
};

describe("export codecs (shared fixture)", () => {
	it("matches the containers, defaults and labels", () => {
		for (const format of ["mp4", "webm"] as const) {
			expect(CODECS_BY_FORMAT[format]).toEqual(cases.formats[format].codecs);
			expect(DEFAULT_CODEC[format]).toBe(cases.formats[format].default);
		}
		expect(CODEC_LABELS).toEqual(cases.labels);
	});

	it("falls back to the container's default", () => {
		for (const item of cases.resolve) {
			expect(resolveVideoCodec({ format: item.format, codec: item.codec }), JSON.stringify(item)).toBe(item.expect);
		}
	});

	it("reads codec names", () => {
		for (const item of cases.parseCodec) expect(parseVideoCodec(item.text), item.text).toBe(item.expect);
	});

	it("reads bitrates in Mbps", () => {
		expect(MIN_BITRATE_MBPS).toBe(cases.bitrateMbps.min);
		expect(MAX_BITRATE_MBPS).toBe(cases.bitrateMbps.max);
		for (const item of cases.bitrateMbps.cases) expect(parseBitrateMbps(item.text), item.text).toBe(item.expect);
	});
});

describe("encodableCodecs", () => {
	it("asks the browser for each codec the container takes", async () => {
		const asked: Array<{ codec: string; hardware: unknown }> = [];
		const codecs = await encodableCodecs({
			format: "mp4",
			width: 1920,
			height: 1080,
			hardware: true,
			canEncode: async (codec, options) => {
				asked.push({ codec, hardware: options?.hardwareAcceleration });
				if (codec === "hevc") throw new Error("no encoder");
				return codec !== "av1";
			},
		});
		expect(codecs).toEqual(["h264"]);
		expect(asked).toEqual([
			{ codec: "avc", hardware: "prefer-hardware" },
			{ codec: "hevc", hardware: "prefer-hardware" },
			{ codec: "av1", hardware: "prefer-hardware" },
		]);
		expect(mediabunnyCodec("vp9")).toBe("vp9");
	});
});
