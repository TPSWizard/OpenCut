import { describe, expect, it } from "vitest";
import type { ElementAnimations } from "@/animation/types";
import type { SceneTracks, TimelineElement, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND, type MediaTime } from "@/wasm";
import { bakeAnimatedParams, buildFreezeStill, freezeSourceSeconds, insertFreezeStill, pickFreezeTarget } from "../freeze";

const T = TICKS_PER_SECOND;
const ticks = (seconds: number) => Math.round(seconds * T) as MediaTime;

const video = (overrides: Partial<VideoElement> = {}): VideoElement => ({
	id: "v",
	type: "video",
	name: "talk",
	mediaId: "m",
	startTime: ticks(10),
	duration: ticks(20),
	trimStart: ticks(5),
	trimEnd: ticks(0),
	params: { opacity: 1, "transform.positionX": 40 },
	...overrides,
});

const audio = (id: string, start: number, length: number): TimelineElement =>
	({
		id,
		type: "audio",
		sourceType: "upload",
		name: id,
		mediaId: id,
		startTime: ticks(start),
		duration: ticks(length),
		trimStart: ticks(0),
		trimEnd: ticks(0),
		params: {},
	}) as TimelineElement;

function tracksWith(main: TimelineElement[], extra: TimelineElement[] = [], locked = false): SceneTracks {
	return {
		overlay: [],
		main: { id: "main", type: "video", name: "Main", elements: main, hidden: false, muted: false } as unknown as SceneTracks["main"],
		audio: [{ id: "a", type: "audio", name: "Audio", elements: extra, muted: false, locked } as unknown as SceneTracks["audio"][number]],
	};
}

describe("freeze frame", () => {
	it("maps the playhead to the source frame, with speed changes and reverse", () => {
		expect(freezeSourceSeconds({ element: video(), clipTime: ticks(4) })).toBeCloseTo(9, 6);
		expect(freezeSourceSeconds({ element: video({ retime: { rate: 2 } }), clipTime: ticks(4) })).toBeCloseTo(13, 6);
		const reversed = video({ trimStart: ticks(0), duration: ticks(10), retime: { rate: 1, reverse: true } });
		expect(freezeSourceSeconds({ element: reversed, clipTime: ticks(2) })).toBeCloseTo(8, 6);
	});

	it("picks the selected video under the playhead, else the topmost", () => {
		const a = video({ id: "a" });
		const tracks: SceneTracks = {
			...tracksWith([video({ id: "b" })]),
			overlay: [{ id: "o", type: "video", name: "Top", elements: [a], hidden: false, muted: false } as unknown as SceneTracks["overlay"][number]],
		};
		expect(pickFreezeTarget({ tracks, selected: [], time: ticks(12) })).toEqual({ trackId: "o", elementId: "a" });
		expect(pickFreezeTarget({ tracks, selected: [{ trackId: "main", elementId: "b" }], time: ticks(12) })).toEqual({ trackId: "main", elementId: "b" });
		expect(pickFreezeTarget({ tracks, selected: [], time: ticks(30) })).toBeNull();
		expect(pickFreezeTarget({ tracks, selected: [], time: ticks(10) })).toEqual({ trackId: "o", elementId: "a" });
	});

	it("bakes animated values at the frozen moment into the still", () => {
		const animations: ElementAnimations = {
			opacity: {
				keys: [
					{ id: "k0", time: ticks(0), value: 0, segmentToNext: "linear", tangentMode: "flat" },
					{ id: "k1", time: ticks(4), value: 1, segmentToNext: "linear", tangentMode: "flat" },
				],
			},
		} as unknown as ElementAnimations;
		const element = video({ animations, effects: [{ id: "e", type: "blur", params: { intensity: 5 }, enabled: true }] as VideoElement["effects"] });
		expect(bakeAnimatedParams({ element, localTime: ticks(2) }).opacity).toBeCloseTo(0.5, 6);
		const still = buildFreezeStill({ element, time: ticks(12), freeze: ticks(3), mediaId: "still" });
		expect(still).toMatchObject({ type: "image", mediaId: "still", startTime: ticks(12), duration: ticks(3), name: "talk (freeze)" });
		expect(still.params.opacity).toBeCloseTo(0.5, 6);
		expect(still.params["transform.positionX"]).toBe(40);
		expect(still.effects).toEqual(element.effects);
		expect(still.effects).not.toBe(element.effects);
		expect("animations" in still).toBe(false);
	});

	it("puts the still in and moves later clips along, leaving locked tracks and earlier clips", () => {
		const left = video({ id: "left", duration: ticks(2) });
		const right = video({ id: "right", startTime: ticks(12), duration: ticks(18) });
		const music = audio("music", 0, 40);
		const voice = audio("voice", 12, 5);
		const still = buildFreezeStill({ element: left, time: ticks(12), freeze: ticks(3), mediaId: "s" });
		const out = insertFreezeStill({ tracks: tracksWith([left, right], [music, voice]), trackId: "main", still });
		expect(out.main.elements.map((element) => [element.id, element.startTime / T])).toEqual([
			["left", 10],
			[still.id, 12],
			["right", 15],
		]);
		expect(out.audio[0].elements.map((element) => [element.id, element.startTime / T])).toEqual([
			["music", 0],
			["voice", 15],
		]);
		const locked = insertFreezeStill({ tracks: tracksWith([left, right], [voice], true), trackId: "main", still });
		expect(locked.audio[0].elements[0].startTime).toBe(ticks(12));
	});
});
