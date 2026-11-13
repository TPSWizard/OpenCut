import { describe, expect, it } from "vitest";
import type { ImageElement, TimelineTrack } from "@/timeline/types";
import type { MediaTime } from "@/wasm";
import { getClipTransition, transitionTarget } from "../apply";
import {
	getInTransitionWindow,
	getIncomingTransitionWindow,
	getPreviousAdjacent,
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

	it("plays a clip's own start transition only when nothing sits right before it", () => {
		const fade = { type: "Fade", duration: seconds(1) };
		const a = { ...clip({ id: "a", start: 0, duration: 3, transition: 0.5 }), transitionIn: fade };
		const b = { ...clip({ id: "b", start: 3, duration: 0.6 }), transitionIn: fade };
		const c = { ...clip({ id: "c", start: 5, duration: 2 }), transitionIn: fade };
		const t = track([a, b, c]);
		expect(getInTransitionWindow({ track: t, element: a })).toEqual({ start: seconds(0), end: seconds(1), toElementId: "a" });
		expect(getInTransitionWindow({ track: t, element: b })).toBeNull();
		expect(getPreviousAdjacent({ track: t, element: b })?.id).toBe("a");
		expect(getInTransitionWindow({ track: t, element: c })).toEqual({ start: seconds(5), end: seconds(6), toElementId: "c" });
		// A short clip caps the window at its length.
		const short = { ...clip({ id: "s", start: 9, duration: 0.4 }), transitionIn: fade };
		expect(getInTransitionWindow({ track: track([short]), element: short })?.end).toBe(seconds(9.4));
	});

	it("routes a clip's start to the clip before it when they touch", () => {
		const a = clip({ id: "a", start: 0, duration: 3, transition: 0.5 });
		const b = { ...clip({ id: "b", start: 3, duration: 2 }), transitionIn: { type: "Wipe", duration: seconds(1) } };
		const c = { ...clip({ id: "c", start: 6, duration: 2 }), transitionIn: { type: "Cube", duration: seconds(1) } };
		const t = track([a, b, c]);
		expect(transitionTarget({ track: t, element: b, side: "start" })).toMatchObject({ elementId: "a", field: "transition" });
		expect(getClipTransition({ track: t, element: b, side: "start" })?.type).toBe("Fade");
		expect(transitionTarget({ track: t, element: c, side: "start" })).toMatchObject({ elementId: "c", field: "transitionIn" });
		expect(getClipTransition({ track: t, element: c, side: "start" })?.type).toBe("Cube");
		expect(getClipTransition({ track: t, element: b, side: "end" })).toBeNull();
	});
});
