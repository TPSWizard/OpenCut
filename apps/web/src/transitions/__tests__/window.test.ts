import { describe, expect, it } from "vitest";
import type { ImageElement, TimelineTrack } from "@/timeline/types";
import type { MediaTime } from "@/wasm";
import {
	getIncomingTransitionWindow,
	getNextAdjacent,
	getOutgoingTransitionWindow,
	getTransitionProgress,
	getTransitionWindow,
	isAdjacent,
	isInTransitionWindow,
} from "../window";

const SECOND = 120_000;
const seconds = (value: number) => Math.round(value * SECOND) as MediaTime;

function clip({
	id,
	start,
	duration,
	transition,
}: {
	id: string;
	start: number;
	duration: number;
	transition?: number;
}): ImageElement {
	return {
		id,
		name: id,
		type: "image",
		mediaId: "m",
		startTime: seconds(start),
		duration: seconds(duration),
		trimStart: 0 as MediaTime,
		trimEnd: 0 as MediaTime,
		params: {},
		...(transition !== undefined && {
			transition: { type: "Fade", duration: seconds(transition) },
		}),
	};
}

function track(elements: ImageElement[]): TimelineTrack {
	return {
		id: "t",
		name: "Main",
		type: "video",
		elements,
		muted: false,
		isMain: true,
	} as unknown as TimelineTrack;
}

describe("transition windows", () => {
	it("centres a transition on the cut", () => {
		const a = clip({ id: "a", start: 0, duration: 4, transition: 1 });
		const b = clip({ id: "b", start: 4, duration: 4 });
		const window = getTransitionWindow({ element: a, next: b });
		expect(window).toEqual({
			start: seconds(3.5),
			end: seconds(4.5),
			toElementId: "b",
		});
		if (!window) throw new Error("no window");
		expect(isInTransitionWindow({ transitionWindow: window, time: seconds(3.5) })).toBe(true);
		expect(isInTransitionWindow({ transitionWindow: window, time: seconds(4.5) })).toBe(false);
		expect(getTransitionProgress({ transitionWindow: window, time: seconds(4) })).toBe(0.5);
	});

	it("never outgrows either clip", () => {
		const a = clip({ id: "a", start: 0, duration: 4, transition: 3 });
		const b = clip({ id: "b", start: 4, duration: 1 });
		const window = getTransitionWindow({ element: a, next: b });
		expect(window?.end).toBe(seconds(4.5));
		expect(window?.start).toBe(seconds(3.5));
	});

	it("fades out over the end without a neighbour, and a gap is not a cut", () => {
		const a = clip({ id: "a", start: 0, duration: 4, transition: 1 });
		expect(getTransitionWindow({ element: a, next: null })).toEqual({
			start: seconds(3),
			end: seconds(4),
			toElementId: null,
		});
		const far = clip({ id: "c", start: 6, duration: 2 });
		expect(isAdjacent({ element: a, next: far })).toBe(false);
		expect(getTransitionWindow({ element: a, next: far })?.toElementId).toBeNull();
	});

	it("finds neighbours on a track", () => {
		const a = clip({ id: "a", start: 0, duration: 4, transition: 1 });
		const b = clip({ id: "b", start: 4, duration: 2 });
		const c = clip({ id: "c", start: 6, duration: 2 });
		const main = track([b, a, c]);
		expect(getNextAdjacent({ track: main, element: a })?.id).toBe("b");
		expect(getOutgoingTransitionWindow({ track: main, element: a })?.toElementId).toBe("b");
		expect(getIncomingTransitionWindow({ track: main, element: b })?.start).toBe(seconds(3.5));
		expect(getIncomingTransitionWindow({ track: main, element: a })).toBeNull();
		expect(getIncomingTransitionWindow({ track: main, element: c })).toBeNull();
	});

	it("gives no window without a transition or with a zero length", () => {
		expect(getTransitionWindow({ element: clip({ id: "a", start: 0, duration: 4 }), next: null })).toBeNull();
		expect(getTransitionWindow({ element: clip({ id: "a", start: 0, duration: 4, transition: 0 }), next: null })).toBeNull();
	});
});
