import { describe, expect, test } from "bun:test";
import { HeadlessSpecError, parseHeadlessJob } from "./spec.ts";

describe("parseHeadlessJob", () => {
	test("accepts a complete job", () => {
		const job = parseHeadlessJob({
			project: { name: "Demo", fps: 30, canvas: { width: 1920, height: 1080 } },
			media: { a: "./a.mp4" },
			timeline: [
				{ type: "clip", media: "a", at: 0 },
				{ type: "text", text: "Hi", at: 1, duration: 2, params: { fontSize: 40 } },
			],
			script: "return 1;",
			export: { format: "webm", quality: "low", output: "./out" },
		});
		expect(job.project?.canvas).toEqual({ width: 1920, height: 1080 });
		expect(job.timeline).toHaveLength(2);
		expect(job.export?.format).toBe("webm");
	});

	test("rejects clips that reference unknown media", () => {
		expect(() =>
			parseHeadlessJob({
				media: { a: "./a.mp4" },
				timeline: [{ type: "clip", media: "b" }],
			}),
		).toThrow(HeadlessSpecError);
	});

	test("rejects unknown step types and export formats", () => {
		expect(() => parseHeadlessJob({ timeline: [{ type: "sticker" }] })).toThrow(
			/type must be/,
		);
		expect(() =>
			parseHeadlessJob({ export: { output: "x", format: "gif" } }),
		).toThrow(/format/);
	});

	test("reads the export codec, bitrate and hardware choice", () => {
		const job = parseHeadlessJob({
			export: { output: "x.mp4", codec: "av1", videoBitrate: 6_000_000, hardware: true },
		});
		expect(job.export).toMatchObject({ codec: "av1", videoBitrate: 6_000_000, hardware: true });
		expect(() => parseHeadlessJob({ export: { output: "x", codec: "prores" } })).toThrow(/codec/);
	});

	test("rejects empty jobs and non-objects", () => {
		expect(() => parseHeadlessJob({})).toThrow(/empty/);
		expect(() => parseHeadlessJob("nope")).toThrow(HeadlessSpecError);
	});
});
