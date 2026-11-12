import { describe, expect, it } from "vitest";
import type { TimelineTrack } from "@/timeline/types";
import {
	collectEditPoints,
	deleteRangePlan,
	elementsCrossing,
	mergeSpans,
	nextEditPoint,
	previousEditPoint,
	rippleDeletePlan,
} from "../edits";

function track({
	id,
	elements,
}: {
	id: string;
	elements: { id: string; start: number; duration: number }[];
}): TimelineTrack {
	return {
		id,
		name: id,
		type: "video",
		hidden: false,
		muted: false,
		isMain: false,
		elements: elements.map((element) => ({
			id: element.id,
			name: element.id,
			type: "image",
			mediaId: "m",
			startTime: element.start,
			duration: element.duration,
			trimStart: 0,
			trimEnd: 0,
			params: {},
		})),
	} as unknown as TimelineTrack;
}

const tracks = [
	track({ id: "v", elements: [{ id: "a", start: 0, duration: 10 }, { id: "b", start: 10, duration: 5 }, { id: "c", start: 20, duration: 5 }] }),
	track({ id: "t", elements: [{ id: "title", start: 12, duration: 3 }] }),
];

describe("edit points", () => {
	it("collects starts and ends and steps between them", () => {
		const points = collectEditPoints({ tracks });
		expect(points).toEqual([0, 10, 12, 15, 20, 25]);
		expect(nextEditPoint({ points, time: 10 })).toBe(12);
		expect(nextEditPoint({ points, time: 25 })).toBeNull();
		expect(previousEditPoint({ points, time: 12 })).toBe(10);
		expect(previousEditPoint({ points, time: 0 })).toBeNull();
		expect(previousEditPoint({ points, time: 11 })).toBe(10);
	});
});

describe("rippleDeletePlan", () => {
	it("removes the elements and closes the gaps on every track", () => {
		const plan = rippleDeletePlan({ tracks, elements: [{ trackId: "v", elementId: "b" }] });
		expect(plan.remove).toEqual([{ trackId: "v", elementId: "b" }]);
		expect(plan.removedTicks).toBe(5);
		expect(plan.moves).toEqual([
			{ trackId: "v", elementId: "c", startTime: 15 },
			// The title starts inside the removed span, so it stays.
		]);
	});

	it("counts overlapping deletions once", () => {
		const two = [
			track({ id: "v", elements: [{ id: "a", start: 0, duration: 10 }, { id: "c", start: 20, duration: 5 }] }),
			track({ id: "w", elements: [{ id: "x", start: 5, duration: 10 }, { id: "y", start: 30, duration: 1 }] }),
		];
		const plan = rippleDeletePlan({
			tracks: two,
			elements: [{ trackId: "v", elementId: "a" }, { trackId: "w", elementId: "x" }],
		});
		expect(mergeSpans({ spans: [[0, 10], [5, 15]] })).toEqual([[0, 15]]);
		expect(plan.removedTicks).toBe(15);
		expect(plan.moves).toEqual([
			{ trackId: "v", elementId: "c", startTime: 5 },
			{ trackId: "w", elementId: "y", startTime: 15 },
		]);
	});
});

describe("deleteRangePlan", () => {
	it("drops what is inside the range and pulls the rest left", () => {
		const split = [
			track({ id: "v", elements: [{ id: "a1", start: 0, duration: 8 }, { id: "a2", start: 8, duration: 2 }, { id: "b", start: 10, duration: 5 }, { id: "c", start: 20, duration: 5 }] }),
		];
		const plan = deleteRangePlan({ tracks: split, from: 8, to: 15 });
		expect(plan.remove).toEqual([{ trackId: "v", elementId: "a2" }, { trackId: "v", elementId: "b" }]);
		expect(plan.moves).toEqual([{ trackId: "v", elementId: "c", startTime: 13 }]);
		expect(elementsCrossing({ tracks, time: 12 })).toEqual([{ trackId: "v", elementId: "b" }]);
		expect(elementsCrossing({ tracks, time: 10 })).toEqual([]);
	});
});
