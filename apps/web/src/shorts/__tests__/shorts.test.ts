import { describe, expect, it } from "vitest";
import type { SceneTracks, TextElement, TimelineElement } from "@/timeline/types";
import { TICKS_PER_SECOND, type MediaTime } from "@/wasm";
import { coverCanvas, cropElement, cropTracks } from "../crop";
import { formatSeconds } from "../format";
import { localPicks, transcriptLines, type TimedLine } from "../lines";

const T = TICKS_PER_SECOND;
const ticks = (seconds: number) => Math.round(seconds * T) as MediaTime;

describe("transcriptLines", () => {
	it("breaks at sentence ends, pauses and a length cap", () => {
		const word = (text: string, start: number, end: number) => ({ text, start, end });
		const lines = transcriptLines({
			words: [
				word("Hello", 0, 0.4),
				word("there.", 0.5, 0.9),
				word("So", 1.0, 1.2),
				word("um", 1.3, 1.4),
				word("next", 3.0, 3.3), // after a 1.6 s pause
				word("thing", 3.4, 3.7),
			],
		});
		expect(lines.map((line) => line.text)).toEqual(["Hello there.", "So um", "next thing"]);
		expect(lines[2]).toMatchObject({ start: 3.0, end: 3.7 });
		const long = Array.from({ length: 40 }, (_, index) => word(`w${index}`, index * 0.5, index * 0.5 + 0.4));
		const capped = transcriptLines({ words: long, maxSeconds: 5 });
		expect(capped.every((line) => line.end - line.start <= 5)).toBe(true);
		expect(capped.length).toBeGreaterThan(3);
	});
});

describe("localPicks", () => {
	it("picks dense, non-overlapping runs of whole lines that fit", () => {
		// Ten 9.5 s lines; line 4 is dense and opens with a question.
		const lines: TimedLine[] = Array.from({ length: 10 }, (_, index) => ({
			start: index * 10,
			end: index * 10 + 9.5,
			text: index === 4 ? "Did you know this works? It really does work well every single time you try it." : "A slow line.",
		}));
		const picks = localPicks({ lines, options: { count: 2, minSeconds: 15, maxSeconds: 30 } });
		expect(picks).toHaveLength(2);
		expect(picks.some((pick) => pick.first === 4)).toBe(true);
		for (const pick of picks) expect(pick.end - pick.start).toBeGreaterThanOrEqual(15);
		expect(picks[0].last < picks[1].first).toBe(true);
		expect(picks.find((pick) => pick.first === 4)?.title).toBe("Did you know this works?");
		expect(localPicks({ lines, options: { count: 3, minSeconds: 200, maxSeconds: 300 } })).toEqual([]);
	});
});

describe("cropping a short", () => {
	const video = {
		id: "v",
		type: "video",
		name: "talk",
		mediaId: "m",
		startTime: ticks(10),
		duration: ticks(20),
		trimStart: ticks(5),
		trimEnd: ticks(0),
		params: {},
	} as unknown as TimelineElement;

	it("cuts clips at the window and moves them to 0", () => {
		const cropped = cropElement({ element: video, start: ticks(15), end: ticks(25) });
		expect(cropped?.startTime).toBe(0);
		expect(cropped?.duration).toBe(ticks(10));
		expect(cropped?.trimStart).toBe(ticks(10));
		expect(cropped?.trimEnd).toBe(ticks(5));
		const inside = cropElement({ element: video, start: ticks(5), end: ticks(40) });
		expect(inside).toMatchObject({ startTime: ticks(5), duration: ticks(20), trimStart: ticks(5) });
		expect(cropElement({ element: video, start: ticks(30), end: ticks(40) })).toBeNull();
	});

	it("keeps the caption words inside the window", () => {
		const caption = {
			id: "c",
			type: "text",
			name: "c",
			startTime: ticks(14),
			duration: ticks(2),
			trimStart: 0,
			trimEnd: 0,
			params: { content: "one two three" },
			words: [
				{ text: "one", start: ticks(0), end: ticks(0.5) },
				{ text: "two", start: ticks(0.8), end: ticks(1.4) },
				{ text: "three", start: ticks(1.5), end: ticks(2) },
			],
		} as unknown as TextElement;
		const cropped = cropElement({ element: caption, start: ticks(15), end: ticks(25) }) as TextElement;
		expect(cropped.params.content).toBe("two three");
		expect(cropped.startTime).toBe(0);
		expect(cropped.words?.map((word) => [word.start, word.end])).toEqual([
			[0, ticks(0.4)],
			[ticks(0.5), ticks(1)],
		]);
		expect(cropElement({ element: caption, start: ticks(15.9), end: ticks(25) })).toBeNull();
	});

	it("covers the vertical canvas, centred", () => {
		const tracks = { overlay: [], main: { id: "main", type: "video", elements: [video] }, audio: [] } as unknown as SceneTracks;
		const short = coverCanvas({
			tracks: cropTracks({ tracks, start: ticks(15), end: ticks(25) }),
			canvas: { width: 1080, height: 1920 },
			sizeOf: () => ({ width: 1920, height: 1080 }),
		});
		const element = short.main.elements[0] as TimelineElement;
		// Contain fit is 1080/1920 wide; cover is 1920/1080 tall: 3.16× more.
		expect(element.params["transform.scaleX"]).toBeCloseTo((1920 / 1080) / (1080 / 1920), 6);
		expect(element.params["transform.positionX"]).toBe(0);
	});

	it("formats times", () => {
		expect(formatSeconds(65.4)).toBe("1:05");
		expect(formatSeconds(3725)).toBe("1:02:05");
	});
});
