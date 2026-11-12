import { describe, expect, test } from "vitest";
import type { MediaTime } from "@/wasm";
import { elementsInRange, rippleDeletePlan, deleteRangePlan } from "@/timeline/edits";
import type { TimelineTrack } from "@/timeline/types";
import { chaptersText } from "../chapters";

const T = 120_000;
const at = (seconds: number) => Math.round(seconds * T) as MediaTime;

describe("chapters", () => {
	test("match the desktop's text", () => {
		const bookmarks = [
			{ time: at(75), note: "Setup" },
			{ time: at(12.5), note: "Replaced" },
		];
		expect(chaptersText({ bookmarks, ticksPerSecond: T })).toBe("00:00 Intro\n00:12 Replaced\n01:15 Setup");
		const long = [{ time: at(0) }, { time: at(3725), note: " End " }];
		expect(chaptersText({ bookmarks: long, ticksPerSecond: T })).toBe("0:00:00 Chapter 1\n1:02:05 End");
	});
});

function track(id: string, spans: [string, number, number][], locked = false): TimelineTrack {
	return {
		id,
		name: id,
		type: "text",
		hidden: false,
		...(locked ? { locked: true } : {}),
		elements: spans.map(([elementId, start, length]) => ({
			id: elementId,
			type: "text",
			name: elementId,
			startTime: at(start),
			duration: at(length),
			trimStart: 0,
			trimEnd: 0,
		})),
	} as unknown as TimelineTrack;
}

describe("locked tracks in range edits", () => {
	const tracks = [
		track("open", [
			["a", 0, 2],
			["b", 3, 2],
			["c", 6, 2],
		]),
		track("locked", [["d", 1, 2], ["e", 6, 2]], true),
	];

	test("select range takes overlapping clips on unlocked tracks only", () => {
		expect(elementsInRange({ tracks, from: at(4), to: at(1) }).map((ref) => ref.elementId)).toEqual(["a", "b"]);
		expect(elementsInRange({ tracks, from: at(5), to: at(6) })).toEqual([]);
	});

	test("ripple and range deletes leave locked tracks alone", () => {
		const ripple = rippleDeletePlan({ tracks, elements: [{ trackId: "open", elementId: "a" }] });
		expect(ripple.moves.map((move) => move.elementId)).toEqual(["b", "c"]);
		const range = deleteRangePlan({ tracks, from: at(5), to: at(6.5) });
		expect(range.moves.map((move) => move.elementId)).toEqual([]);
		expect(range.remove).toEqual([]);
		const wide = deleteRangePlan({ tracks, from: at(2.5), to: at(5.5) });
		expect(wide.remove.map((ref) => ref.elementId)).toEqual(["b"]);
		expect(wide.moves.map((move) => move.elementId)).toEqual(["c"]);
	});
});
