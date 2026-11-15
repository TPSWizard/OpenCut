import { describe, expect, it } from "vitest";
import type { SceneTracks, TimelineTrack } from "@/timeline/types";
import { mediaTime, type MediaTime } from "@/wasm";
import {
	clampPps,
	formatClock,
	PHONE_MAX_PPS,
	PHONE_MIN_PPS,
	rowAt,
	rulerStep,
	scrollForTime,
	timeForScroll,
	zoomByPinch,
} from "../timeline-math";
import { clipEdges, planPhoneMove, snapStart } from "../move";

describe("phone timeline maths", () => {
	it("turns scrolling into time and back under the centred playhead", () => {
		expect(scrollForTime({ seconds: 2.5, pps: 40 })).toBe(100);
		expect(scrollForTime({ seconds: -1, pps: 40 })).toBe(0);
		expect(timeForScroll({ scrollLeft: 100, pps: 40, duration: 10 })).toBe(2.5);
		expect(timeForScroll({ scrollLeft: 1000, pps: 40, duration: 10 })).toBe(10);
		expect(timeForScroll({ scrollLeft: -5, pps: 40, duration: 10 })).toBe(0);
	});

	it("zooms with a pinch, within limits", () => {
		expect(zoomByPinch({ pps: 50, startDistance: 100, distance: 200 })).toBe(100);
		expect(zoomByPinch({ pps: 50, startDistance: 100, distance: 1 })).toBe(PHONE_MIN_PPS);
		expect(zoomByPinch({ pps: 300, startDistance: 10, distance: 100 })).toBe(PHONE_MAX_PPS);
		expect(zoomByPinch({ pps: 50, startDistance: 0, distance: 100 })).toBe(50);
		expect(clampPps(Number.NaN)).toBeGreaterThan(0);
	});

	it("spaces ruler labels and formats them", () => {
		expect(rulerStep({ pps: 200 })).toBe(0.5);
		expect(rulerStep({ pps: 48 })).toBe(2);
		expect(rulerStep({ pps: 4 })).toBe(30);
		expect(formatClock(65)).toBe("1:05");
		expect(formatClock(3.25, { tenths: true })).toBe("0:03.2");
		expect(formatClock(3725)).toBe("1:02:05");
		expect(formatClock(-3)).toBe("0:00");
	});

	it("finds the row under a finger", () => {
		expect(rowAt({ y: 0, rowHeight: 50, rowCount: 3 })).toBe(0);
		expect(rowAt({ y: 120, rowHeight: 50, rowCount: 3 })).toBe(2);
		expect(rowAt({ y: 150, rowHeight: 50, rowCount: 3 })).toBeNull();
		expect(rowAt({ y: -1, rowHeight: 50, rowCount: 3 })).toBeNull();
	});
});

const t = (ticks: number) => mediaTime({ ticks }) as MediaTime;

function track({
	id,
	type = "video",
	elements,
}: {
	id: string;
	type?: TimelineTrack["type"];
	elements: { id: string; start: number; duration: number; type?: string }[];
}): TimelineTrack {
	return {
		id,
		name: id,
		type,
		hidden: false,
		muted: false,
		elements: elements.map((element) => ({
			id: element.id,
			name: element.id,
			type: element.type ?? (type === "audio" ? "audio" : type === "text" ? "text" : "image"),
			mediaId: "m",
			startTime: t(element.start),
			duration: t(element.duration),
			trimStart: t(0),
			trimEnd: t(0),
			params: {},
		})),
	} as unknown as TimelineTrack;
}

function scene(): SceneTracks {
	return {
		overlay: [track({ id: "titles", type: "text", elements: [{ id: "title", start: 100, duration: 50 }] })],
		main: track({ id: "main", elements: [{ id: "a", start: 0, duration: 100 }, { id: "b", start: 100, duration: 100 }] }),
		audio: [track({ id: "music", type: "audio", elements: [{ id: "song", start: 0, duration: 300 }] })],
	} as SceneTracks;
}

describe("phone clip moves", () => {
	it("collects other clips' edges and snaps a start or an end to them", () => {
		const edges = clipEdges({ tracks: scene(), exclude: new Set(["b"]) });
		expect(edges).toEqual([0, 100, 150, 300]);
		expect(snapStart({ start: 96, duration: 40, edges, threshold: 6 })).toBe(100);
		// The end (106 + 40 = 146) is 4 from 150: the clip moves to end there.
		expect(snapStart({ start: 106, duration: 40, edges, threshold: 6 })).toBe(110);
		expect(snapStart({ start: 120, duration: 10, edges, threshold: 6 })).toBe(120);
		expect(snapStart({ start: 3, duration: 10, edges: [-20], threshold: 30 })).toBe(0);
	});

	it("moves a clip along its track when there is room", () => {
		const plan = planPhoneMove({
			tracks: scene(),
			ref: { trackId: "titles", elementId: "title" },
			selected: [{ trackId: "titles", elementId: "title" }],
			startTime: t(200),
			targetTrackId: "titles",
		});
		expect(plan?.moves).toEqual([
			{ sourceTrackId: "titles", targetTrackId: "titles", elementId: "title", newStartTime: 200 },
		]);
		expect(plan?.targetSelection).toEqual([{ trackId: "titles", elementId: "title" }]);
	});

	it("refuses a track that does not take the clip", () => {
		const plan = planPhoneMove({
			tracks: scene(),
			ref: { trackId: "titles", elementId: "title" },
			selected: [],
			startTime: t(0),
			targetTrackId: "music",
		});
		expect(plan).toBeNull();
	});
});
