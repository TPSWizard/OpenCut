import { describe, expect, it } from "vitest";
import {
	assColour,
	assPlacement,
	type ExportCue,
	type ExportCueStyle,
	subtitleFileName,
	writeAss,
	writeSrt,
	writeVtt,
} from "./write";

const style: ExportCueStyle = {
	fontFamily: "Inter",
	fontSize: 15,
	color: "#ffffff",
	bold: true,
	italic: false,
	textAlign: "center",
	positionY: 400,
};

const cues: ExportCue[] = [
	{
		text: "The quick brown fox",
		start: 0.5,
		end: 2.25,
		words: [
			{ text: "The", start: 0.5, end: 0.8 },
			{ text: "quick", start: 0.9, end: 1.2 },
			{ text: "brown", start: 1.2, end: 1.6 },
			{ text: "fox", start: 1.6, end: 2.0 },
		],
		style,
	},
	{
		text: "jumps over\nthe lazy dog & <cat>",
		start: 3661.004,
		end: 3663,
		style: { ...style, positionY: -400, textAlign: "left", color: "#ff000080" },
	},
];

describe("writeSrt", () => {
	it("numbers the cues and writes comma milliseconds", () => {
		expect(writeSrt({ cues })).toBe(
			[
				"1",
				"00:00:00,500 --> 00:00:02,250",
				"The quick brown fox",
				"",
				"2",
				"01:01:01,004 --> 01:01:03,000",
				"jumps over",
				"the lazy dog & <cat>",
				"",
			].join("\n"),
		);
		expect(writeSrt({ cues: [] })).toBe("");
	});
});

describe("writeVtt", () => {
	it("puts an inline timestamp before each later word and escapes markup", () => {
		expect(writeVtt({ cues })).toBe(
			[
				"WEBVTT",
				"",
				"1",
				"00:00:00.500 --> 00:00:02.250",
				"The <00:00:00.900>quick <00:00:01.200>brown <00:00:01.600>fox",
				"",
				"2",
				"01:01:01.004 --> 01:01:03.000",
				"jumps over",
				"the lazy dog &amp; &lt;cat&gt;",
				"",
			].join("\n"),
		);
	});
});

describe("writeAss", () => {
	it("writes a style per look and karaoke tags with pauses as empty syllables", () => {
		const text = writeAss({ cues, canvasSize: { width: 1080, height: 1920 }, title: "Fox" });
		expect(text).toContain("Title: Fox\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920");
		// 15 app units are 15 × 1920 / 90 = 320 px; bottom-centre with a margin above the edge.
		expect(text).toContain("Style: Default,Inter,320,&H00FFFFFF,&H00FFFF00,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,20,20,368,1");
		// The second cue is top-left with a translucent red (alpha 0x80 → ASS 0x7F).
		expect(text).toContain("Style: Style2,Inter,320,&H7F0000FF,");
		expect(text).toMatch(/Style: Style2,.*,7,20,20,368,1/);
		expect(text).toContain(
			"Dialogue: 0,0:00:00.50,0:00:02.25,Default,,0,0,0,,{\\k30}The {\\k10}{\\k30}quick {\\k40}brown {\\k40}fox",
		);
		expect(text).toContain("Dialogue: 0,1:01:01.00,1:01:03.00,Style2,,0,0,0,,jumps over\\Nthe lazy dog & <cat>");
	});

	it("keeps a default style for an empty scene", () => {
		const text = writeAss({ cues: [], canvasSize: { width: 1920, height: 1080 } });
		expect(text).toContain("Style: Default,Arial,48,");
		expect(text.trim().endsWith("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text")).toBe(true);
	});
});

describe("assColour and assPlacement", () => {
	it("converts css colours to &HAABBGGRR", () => {
		expect(assColour("#112233")).toBe("&H00332211");
		expect(assColour("#fff")).toBe("&H00FFFFFF");
		expect(assColour("#00000000")).toBe("&HFF000000");
		expect(assColour("red")).toBe("&H00FFFFFF");
	});

	it("maps the vertical thirds to numpad alignments", () => {
		expect(assPlacement({ style, canvasHeight: 1080 }).alignment).toBe(2);
		expect(assPlacement({ style: { ...style, positionY: 0 }, canvasHeight: 1080 })).toEqual({ alignment: 5, marginV: 0 });
		expect(assPlacement({ style: { ...style, positionY: -300, textAlign: "right" }, canvasHeight: 1080 }).alignment).toBe(9);
	});
});

describe("subtitleFileName", () => {
	it("keeps letters, digits, spaces and dashes", () => {
		expect(subtitleFileName({ name: "My promo: v2/final", format: "vtt" })).toBe("My promo_ v2_final.vtt");
		expect(subtitleFileName({ name: "  ", format: "srt" })).toBe("captions.srt");
	});
});
