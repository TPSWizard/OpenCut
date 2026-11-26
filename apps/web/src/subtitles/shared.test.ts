import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { parseSubtitleFile } from "./parse";
import { type ExportCue, writeSrt, writeVtt } from "./write";

const root = resolve(__dirname, "../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/subtitles/cases.json"), "utf8")) as {
	cases: {
		name: string;
		format: "srt" | "vtt" | null;
		input: string;
		parsed: { cues: { text: string; start: number; end: number; words?: { text: string; start: number; end: number }[] }[]; skipped: number };
	}[];
};

describe("subtitle import through the shared crate", () => {
	it("reads SRT and WebVTT as the reference (and the desktop)", () => {
		for (const item of fixture.cases) {
			// A sniffed case is read under a name that says neither format.
			const fileName = item.format ? `subs.${item.format}` : "subs.srt";
			const result = parseSubtitleFile({ fileName: item.format ? fileName : "subs.vtt", input: item.input });
			if (item.format === null && !item.input.includes("WEBVTT")) {
				// "subs.vtt" for plain SRT text still finds its cue.
				expect(result.captions.length, item.name).toBeGreaterThan(0);
				continue;
			}
			expect(result.skippedCueCount, item.name).toBe(item.parsed.skipped);
			expect(
				result.captions.map((cue) => ({ text: cue.text, start: cue.startTime, end: cue.startTime + cue.duration, words: cue.words })),
				item.name,
			).toEqual(
				item.parsed.cues.map((cue) => ({
					text: cue.text,
					start: cue.start,
					end: expect.closeTo(cue.end, 9),
					words: cue.words,
				})),
			);
		}
	});

	it("reads back what the editor writes, word timings included", () => {
		const style = { fontFamily: "Inter", fontSize: 15, color: "#ffffff", bold: false, italic: false, textAlign: "center", positionY: 0 } as const;
		const cues: ExportCue[] = [
			{
				text: "hello there world",
				start: 1,
				end: 3,
				words: [
					{ text: "hello", start: 1, end: 1.5 },
					{ text: "there", start: 1.5, end: 2.2 },
					{ text: "world", start: 2.2, end: 3 },
				],
				style,
			},
			{ text: "second & <last>", start: 4, end: 5.5, style },
		];
		const vtt = parseSubtitleFile({ fileName: "a.vtt", input: writeVtt({ cues }) });
		expect(vtt.captions.map((cue) => cue.text)).toEqual(["hello there world", "second & <last>"]);
		expect(vtt.captions[0].words?.map((word) => [word.text, word.start])).toEqual([
			["hello", 1],
			["there", 1.5],
			["world", 2.2],
		]);
		expect(vtt.captions[1].words).toBeUndefined();
		const srt = parseSubtitleFile({ fileName: "a.SRT", input: writeSrt({ cues }) });
		expect(srt.captions.map((cue) => [cue.text, cue.startTime, cue.duration])).toEqual([
			["hello there world", 1, 2],
			// SRT has no escaping: angle brackets read as a formatting tag.
			["second &", 4, 1.5],
		]);
		expect(() => parseSubtitleFile({ fileName: "a.txt", input: "" })).toThrow();
	});
});
