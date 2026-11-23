import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import type { SceneTracks, TimelineElement } from "@/timeline/types";
import { captionMargin, contentFactor, pictureFactor, type ResizeMode, resizeTracks, type Size, textFactor } from "../plan";

const root = resolve(__dirname, "../../../../..");

interface Case {
	name: string;
	from: [number, number];
	to: [number, number];
	mode: ResizeMode;
	source: [number, number];
	content: number;
	picture: number;
	text: number;
	captionMargin: number;
}

const size = ([width, height]: [number, number]): Size => ({ width, height });

describe("resize project", () => {
	it("works out the factors like the shared cases", () => {
		const { cases } = JSON.parse(readFileSync(resolve(root, "fixtures/resize-project/cases.json"), "utf8")) as { cases: Case[] };
		expect(cases.length).toBeGreaterThan(10);
		for (const item of cases) {
			const [from, to, source] = [size(item.from), size(item.to), size(item.source)];
			expect(contentFactor({ mode: item.mode, from, to }), `${item.name} content`).toBeCloseTo(item.content, 12);
			expect(pictureFactor({ mode: item.mode, from, to, source }), `${item.name} picture`).toBeCloseTo(item.picture, 12);
			expect(textFactor({ mode: item.mode, from, to }), `${item.name} text`).toBeCloseTo(item.text, 12);
			expect(captionMargin(to), `${item.name} margin`).toBe(item.captionMargin);
		}
	});

	it("re-fits clips, text, keyframes and pixel settings, and leaves captions and audio alone", () => {
		const key = (value: number) => ({ id: "k", time: 0, value, segmentToNext: "linear", tangentMode: "auto", rightHandle: { dt: 1, dv: value } });
		const video = {
			id: "v",
			type: "video",
			mediaId: "m",
			params: { "transform.positionX": 100, "transform.positionY": -50, "transform.scaleX": 1, "transform.scaleY": 1, "outline.width": 4 },
			animations: { "transform.positionX": { keys: [key(200)] }, "transform.scaleX": { keys: [key(2)] } },
			masks: [{ id: "mask", type: "rectangle", params: { feather: 10, strokeWidth: 2 } }],
		};
		const text = { id: "t", type: "text", params: { "transform.positionX": 0, "transform.positionY": 300, "transform.scaleX": 1, "transform.scaleY": 1 } };
		const caption = { ...text, id: "c", words: [{ text: "hi", start: 0, end: 1 }] };
		const audio = { id: "a", type: "audio", params: {} };
		const tracks = {
			overlay: [{ id: "o", elements: [text, caption] }],
			main: { id: "main", elements: [video] },
			audio: [{ id: "au", elements: [audio] }],
		} as unknown as SceneTracks;
		const from = { width: 1920, height: 1080 };
		const to = { width: 1080, height: 1920 };
		const out = resizeTracks({ tracks, from, to, mode: "fill", sizeOf: () => ({ width: 1920, height: 1080 }) });
		const k = 1920 / 1080;
		const resizedVideo = out.main.elements[0] as TimelineElement & { masks: { params: { feather: number; strokeWidth: number } }[] };
		expect(resizedVideo.params["transform.positionX"]).toBeCloseTo(100 * k);
		expect(resizedVideo.params["transform.scaleX"]).toBeCloseTo(pictureFactor({ mode: "fill", from, to, source: from }));
		expect(resizedVideo.params["outline.width"]).toBeCloseTo(4 * k);
		const animations = resizedVideo.animations as unknown as Record<string, { keys: { value: number; rightHandle: { dv: number } }[] }>;
		expect(animations["transform.positionX"].keys[0].value).toBeCloseTo(200 * k);
		expect(animations["transform.positionX"].keys[0].rightHandle.dv).toBeCloseTo(200 * k);
		expect(animations["transform.scaleX"].keys[0].value).toBeCloseTo(2 * pictureFactor({ mode: "fill", from, to, source: from }));
		expect(resizedVideo.masks[0].params.feather).toBeCloseTo(10 * k);
		expect(resizedVideo.masks[0].params.strokeWidth).toBeCloseTo(2 * k);
		const resizedText = out.overlay[0].elements[0] as TimelineElement;
		expect(resizedText.params["transform.positionY"]).toBeCloseTo(300 * k);
		expect(resizedText.params["transform.scaleX"]).toBeCloseTo(textFactor({ mode: "fill", from, to }));
		expect(out.overlay[0].elements[1]).toBe(caption);
		expect(out.audio[0].elements[0]).toBe(audio);
		// A full-frame landscape clip covers the portrait canvas: its drawn height is the canvas height.
		const containTo = Math.min(1080 / 1920, 1920 / 1080);
		expect(1080 * containTo * (resizedVideo.params["transform.scaleX"] as number)).toBeCloseTo(1920);
	});
});
