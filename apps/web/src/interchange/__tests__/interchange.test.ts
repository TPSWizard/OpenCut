import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { interchangeFormatOf, interchangeRead, interchangeWrite } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import { buildElementFromMedia, buildTextElement } from "@/timeline/element-utils";
import { buildEmptyTrack } from "@/timeline/placement/track-factory";
import type { AudioTrack, SceneTracks, TextTrack, TimelineElement, VideoElement, VideoTrack } from "@/timeline/types";
import { mediaTimeFromSeconds } from "@/wasm";
import { type ExchangeAsset, NOTE_TEXT, type TimelineFormat, importTimeline, projectTimeline, readTimeline, writeTimeline } from "../timeline";

const root = resolve(__dirname, "../../../../..");
// biome-ignore lint/suspicious/noExplicitAny: fixture JSON
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/interchange/cases.json"), "utf8")) as any;
const ticks = (seconds: number) => mediaTimeFromSeconds({ seconds });

describe("project interchange", () => {
	it("writes and reads the reference's files through WASM", () => {
		for (const c of fixture.cases) {
			const json = JSON.stringify(c.timeline);
			expect(JSON.parse(interchangeWrite(json, "otio"))).toEqual(c.otio);
			expect(interchangeWrite(json, "fcpxml")).toBe(c.fcpxml);
			expect(interchangeWrite(json, "edl")).toBe(c.edl);
			for (const format of ["otio", "fcpxml", "edl"] as const) {
				const text = interchangeWrite(json, format);
				const back = JSON.parse(interchangeRead(text, `cut.${format}`, c.timeline.rate.num, c.timeline.rate.den));
				expect(back).toEqual(c.back[format]);
			}
		}
		for (const f of fixture.foreign) {
			expect(JSON.parse(interchangeRead(f.text, `file.${f.format}`, f.rate.num, f.rate.den))).toEqual(f.timeline);
			expect(interchangeFormatOf("no-extension", f.text)).toBe(f.format);
		}
		expect(() => interchangeRead("hello", "x.txt", 30, 1)).toThrow();
	});

	const assets: ExchangeAsset[] = [
		{ id: "take", name: "take.mp4", type: "video", duration: 20, width: 1920, height: 1080, hasAudio: true },
		{ id: "logo", name: "logo.png", type: "image", width: 400, height: 400 },
		{ id: "song", name: "song.mp3", type: "audio", duration: 30, hasAudio: true },
	];
	const element = (mediaId: string, type: "video" | "image" | "audio", start: number, length: number, extra: Record<string, unknown> = {}) =>
		({
			...buildElementFromMedia({ mediaId, mediaType: type, name: mediaId, duration: ticks(length), startTime: ticks(start) }),
			id: `${mediaId}-${start}`,
			...extra,
		}) as TimelineElement;
	const project = (): SceneTracks => {
		const main = buildEmptyTrack({ id: "main", type: "video", name: "Main" }) as VideoTrack;
		const logo = buildEmptyTrack({ id: "logo-track", type: "video", name: "Logo" }) as VideoTrack;
		const words = buildEmptyTrack({ id: "words", type: "text", name: "Words" }) as TextTrack;
		const music = buildEmptyTrack({ id: "music", type: "audio", name: "Music" }) as AudioTrack;
		return {
			main: {
				...main,
				elements: [
					element("take", "video", 0, 3, { trimStart: ticks(1), transition: { type: "Fade", duration: ticks(0.5) } }),
					element("take", "video", 3, 2, { trimStart: ticks(10), retime: { rate: 2 }, isSourceAudioEnabled: false }),
				] as VideoElement[],
			},
			overlay: [
				{ ...words, elements: [{ ...buildTextElement({ raw: { name: "Hi" }, startTime: ticks(0) }), id: "t" }] } as TextTrack,
				{ ...logo, elements: [element("logo", "image", 1, 2)] } as VideoTrack,
			],
			audio: [{ ...music, elements: [element("song", "audio", 0, 5)] } as AudioTrack],
		};
	};

	it("maps the project like the desktop editor", () => {
		const timeline = projectTimeline({ name: "P", rate: { num: 30, den: 1 }, width: 1920, height: 1080, tracks: project(), assets });
		expect(timeline.tracks.map((t) => [t.kind, t.name, t.clips.length])).toEqual([
			["video", "Main", 2],
			["video", "Logo", 1],
			["audio", "Music", 1],
		]);
		const [a, b] = timeline.tracks[0].clips;
		expect([a.start, a.duration, a.sourceStart, a.sound, a.transition]).toEqual([0, 90, 30, true, { frames: 15, kind: "dissolve" }]);
		expect([b.start, b.sourceStart, b.speed, b.sound]).toEqual([90, 300, 2, false]);
		expect(timeline.notes).toContain(NOTE_TEXT);
		expect(timeline.media.map((m) => [m.name, m.still])).toEqual([
			["take.mp4", false],
			["logo.png", true],
			["song.mp3", false],
		]);
	});

	it("comes back through every format into an empty timeline, naming missing files", () => {
		const timeline = projectTimeline({ name: "P", rate: { num: 30, den: 1 }, width: 1920, height: 1080, tracks: project(), assets });
		for (const format of ["otio", "fcpxml", "edl"] as TimelineFormat[]) {
			const { text, notes } = writeTimeline(timeline, format);
			if (format === "edl") expect(notes.some((n) => n.includes("one video track"))).toBe(true);
			const back = readTimeline({ text, fileName: `cut.${format}`, rate: timeline.rate });
			const empty = buildEmptyTrack({ id: "m", type: "video", name: "Main" }) as VideoTrack;
			const result = importTimeline({ tracks: { main: empty, overlay: [], audio: [] }, timeline: back, assets });
			expect(result.missing).toEqual([]);
			const main = result.tracks.main.elements as VideoElement[];
			expect(main.map((e) => [e.startTime, e.trimStart, e.duration])).toEqual([
				[0, ticks(1), ticks(3)],
				[ticks(3), ticks(10), ticks(2)],
			]);
			expect(main[0].transition).toEqual({ type: "Fade", duration: ticks(0.5) });
			expect(main[1].retime?.rate).toBe(2);
			expect(main[1].isSourceAudioEnabled).toBe(false);
			expect(result.tracks.overlay.length).toBe(format === "edl" ? 0 : 1);
			expect(result.tracks.audio.flatMap((t) => t.elements).length).toBe(1);
		}
		const { text } = writeTimeline(timeline, "otio");
		const back = readTimeline({ text, fileName: "cut.otio", rate: timeline.rate });
		const result = importTimeline({ tracks: project(), timeline: back, assets: assets.filter((a) => a.id !== "song") });
		expect(result.missing).toEqual(["song.mp3"]);
		expect(result.tracks.overlay.length).toBe(2 + 2);
	});
});
