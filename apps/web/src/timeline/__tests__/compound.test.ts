import { describe, expect, test } from "vitest";
import type { MediaTime } from "@/wasm";
import type { ElementRef, SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import {
	breakApartCompound,
	clipElement,
	compoundChildCount,
	createCompound,
	expandCompounds,
	groupElements,
	groupRefs,
	ungroupElements,
} from "../compound";

const T = 120_000;
const at = (seconds: number) => Math.round(seconds * T) as MediaTime;

function video(id: string, start: number, length: number, extra: Record<string, unknown> = {}): TimelineElement {
	return {
		id,
		type: "video",
		name: id,
		mediaId: "m",
		startTime: at(start),
		duration: at(length),
		trimStart: at(0),
		trimEnd: at(10 - length),
		sourceDuration: at(10),
		params: {},
		...extra,
	} as unknown as TimelineElement;
}

function text(id: string, start: number, length: number): TimelineElement {
	return { id, type: "text", name: id, startTime: at(start), duration: at(length), trimStart: at(0), trimEnd: at(0), params: {} } as unknown as TimelineElement;
}

function track(id: string, type: TimelineTrack["type"], elements: TimelineElement[]): TimelineTrack {
	return { id, name: id, type, elements, hidden: false, muted: false } as unknown as TimelineTrack;
}

function scene(): SceneTracks {
	return {
		overlay: [track("titles", "text", [text("title", 3, 2)])],
		main: track("main", "video", [video("clip", 2, 4)]) as SceneTracks["main"],
		audio: [track("music", "audio", [{ ...video("song", 2, 4), type: "audio" } as unknown as TimelineElement])] as SceneTracks["audio"],
	};
}

const ids = (tracks: SceneTracks) =>
	[...tracks.overlay, tracks.main, ...tracks.audio].flatMap((track) => track.elements.map((element) => element.id));

describe("clipping", () => {
	test("keeps source frames in place, like the desktop", () => {
		const clip = video("v", 0, 4, { trimStart: at(1) });
		const cut = clipElement({ element: clip, cutStart: at(1), cutEnd: at(0.5) });
		expect([cut.trimStart, cut.duration]).toEqual([at(2), at(2.5)]);
		const fast = clipElement({ element: { ...clip, retime: { rate: 2 } } as TimelineElement, cutStart: at(1), cutEnd: 0 });
		expect(fast.trimStart).toBe(at(3));
		const back = clipElement({ element: { ...clip, retime: { rate: 1, reverse: true } } as TimelineElement, cutStart: 0, cutEnd: at(1) });
		expect(back.trimStart).toBe(at(2));
	});
});

describe("compound clips", () => {
	test("create, expand, trim and break apart", () => {
		const before = scene();
		const refs: ElementRef[] = [
			{ trackId: "main", elementId: "clip" },
			{ trackId: "titles", elementId: "title" },
			{ trackId: "music", elementId: "song" },
		];
		const { tracks, ref } = createCompound({ tracks: before, refs, compoundId: "c1" });
		expect(ref).toEqual({ trackId: "main", elementId: "c1" });
		expect(ids(tracks)).toEqual(["c1"]);
		const compound = tracks.main.elements[0];
		expect([compound.startTime, compound.duration, compound.sourceDuration]).toEqual([at(2), at(4), at(4)]);
		expect(compoundChildCount(compound as TimelineElement)).toBe(3);
		expect(JSON.parse(JSON.stringify(compound)).type).toBe("compound");

		// Expanded: the same clips at the same times, ids prefixed.
		const expanded = expandCompounds(tracks);
		const placed = [...expanded.overlay, expanded.main, ...expanded.audio].flatMap((t) => t.elements as TimelineElement[]);
		const byId = new Map(placed.map((element) => [element.id, element]));
		expect(byId.get("c1~clip")?.startTime).toBe(at(2));
		expect(byId.get("c1~title")?.startTime).toBe(at(3));
		expect(byId.get("c1~song")?.startTime).toBe(at(2));
		expect(expanded.audio.some((t) => t.elements.some((element) => element.id === "c1~song"))).toBe(true);
		expect(expanded.overlay.some((t) => t.elements.some((element) => element.id === "c1~title"))).toBe(true);

		// Trimming the compound's start by 2 s cuts into its children.
		const trimmed: SceneTracks = {
			...tracks,
			main: { ...tracks.main, elements: [{ ...compound, startTime: at(4), duration: at(2), trimStart: at(2) }] } as SceneTracks["main"],
		};
		const cut = new Map(
			[...expandCompounds(trimmed).overlay, expandCompounds(trimmed).main].flatMap((t) => (t.elements as TimelineElement[]).map((element) => [element.id, element])),
		);
		const clip = cut.get("c1~clip");
		expect([clip?.startTime, clip?.trimStart, clip?.duration]).toEqual([at(4), at(2), at(2)]);
		expect([cut.get("c1~title")?.startTime, cut.get("c1~title")?.duration]).toEqual([at(4), at(1)]);

		const apart = breakApartCompound({ tracks: trimmed, ref: { trackId: "main", elementId: "c1" } });
		expect(apart.refs).toHaveLength(3);
		expect(ids(apart.tracks).sort()).toEqual(["clip", "song", "title"]);
		const restored = apart.tracks.main.elements.find((element) => element.id === "clip") as TimelineElement;
		expect([restored.startTime, restored.trimStart, restored.duration]).toEqual([at(4), at(2), at(2)]);
		const groups = new Set(
			[...apart.tracks.overlay, apart.tracks.main, ...apart.tracks.audio].flatMap((t) => (t.elements as TimelineElement[]).map((element) => element.groupId)),
		);
		expect(groups.size).toBe(1);
	});

	test("a compound with no free video track goes on a new one", () => {
		const tracks = scene();
		const { tracks: next, ref } = createCompound({ tracks, refs: [{ trackId: "titles", elementId: "title" }], compoundId: "c2" });
		expect(next.overlay[0].type).toBe("video");
		expect(ref.trackId).toBe(next.overlay[0].id);
	});
});

describe("groups", () => {
	test("select together, ungroup as one", () => {
		const tracks = scene();
		const grouped = groupElements({
			tracks,
			refs: [
				{ trackId: "main", elementId: "clip" },
				{ trackId: "titles", elementId: "title" },
			],
			groupId: "g1",
		});
		expect(groupRefs({ tracks: grouped, ref: { trackId: "main", elementId: "clip" } }).map((ref) => ref.elementId).sort()).toEqual(["clip", "title"]);
		expect(groupRefs({ tracks: grouped, ref: { trackId: "music", elementId: "song" } })).toEqual([{ trackId: "music", elementId: "song" }]);
		expect(() => groupElements({ tracks, refs: [{ trackId: "main", elementId: "clip" }], groupId: "g2" })).toThrow();
		const ungrouped = ungroupElements({ tracks: grouped, refs: [{ trackId: "titles", elementId: "title" }] });
		expect((ungrouped.main.elements[0] as TimelineElement).groupId).toBeUndefined();
	});
});
