import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { jumpCutFind, jumpCutFrame, jumpCutMotion, jumpCutPlan } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import type { SceneTracks, VideoElement } from "@/timeline/types";
import { TICKS_PER_SECOND } from "@/wasm";
import { JUMP_CUT_BRIDGE_PARAM, JUMP_CUT_TRACK_NAME, buildBridgeElement, insertBridges, planJumpCutBridge, projectJumpCuts } from "../cuts";

const root = resolve(__dirname, "../../../../..");
// biome-ignore lint/suspicious/noExplicitAny: fixture JSON
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/jumpcut/jumpcut.json"), "utf8")) as any;

const pat = (x: number, y: number, c: number) =>
	0.5 + 0.2 * Math.sin(0.31 * x + 0.17 * y + 1.3 * c) + 0.15 * Math.sin(0.11 * x - 0.23 * y + 2.1 + 0.7 * c) + 0.1 * Math.cos(0.05 * x + 0.37 * y + c);
const back = (x: number, y: number, c: number) => 0.45 + 0.12 * Math.sin(0.19 * x - 0.13 * y + 0.9 * c) + 0.08 * Math.cos(0.27 * y + 0.4 * c);
const q8 = (value: number) => Math.min(255, Math.max(0, Math.floor(value * 255 + 0.5)));

// biome-ignore lint/suspicious/noExplicitAny: fixture JSON
function picture(c: any, t: number): Uint8Array {
	const { width: w, height: h } = c;
	const out = new Uint8Array(w * h * 4);
	for (let y = 0; y < h; y++) {
		for (let x = 0; x < w; x++) {
			for (let ch = 0; ch < 3; ch++) {
				let value: number;
				if (c.kind === "shift") {
					const zoom = c.zoom ?? 1;
					value = pat((x - t * c.shift[0]) / zoom, (y - t * c.shift[1]) / zoom, ch);
				} else if (c.kind === "head") {
					const cx = c.from[0] + t * (c.to[0] - c.from[0]);
					const cy = c.from[1] + t * (c.to[1] - c.from[1]);
					value = ((x - cx) / c.radii[0]) ** 2 + ((y - cy) / c.radii[1]) ** 2 <= 1 ? 0.15 + pat(x - cx, y - cy, ch) : back(x, y, ch);
				} else {
					value = pat(x, y, ch);
				}
				out[(y * w + x) * 4 + ch] = q8(value);
			}
			out[(y * w + x) * 4 + 3] = 255;
		}
	}
	return out;
}

const ticks = (seconds: number) => Math.round(seconds * TICKS_PER_SECOND);

function piece(id: string, start: number, trim: number, length: number, params: Record<string, unknown> = {}): VideoElement {
	return {
		id,
		type: "video",
		name: id,
		mediaId: "take",
		startTime: ticks(start),
		duration: ticks(length),
		trimStart: ticks(trim),
		trimEnd: ticks(20 - trim - length),
		params: { volume: 1, ...params },
	} as unknown as VideoElement;
}

describe("smooth jump cuts", () => {
	it("finds cuts, plans bridges and makes frames as the reference does, through WASM", () => {
		for (const timeline of fixture.timelines) {
			const tracks: string[] = [];
			const media: string[] = [];
			const flat: number[] = [];
			for (const clip of timeline.clips) {
				if (!tracks.includes(clip.track)) tracks.push(clip.track);
				if (!media.includes(clip.media)) media.push(clip.media);
				flat.push(tracks.indexOf(clip.track), media.indexOf(clip.media), clip.start, clip.end, clip.sourceStart, clip.sourceEnd);
			}
			const found = jumpCutFind(new Float64Array(flat), timeline.fps);
			expect(found.length / 4, timeline.name).toBe(timeline.cuts.length);
			timeline.cuts.forEach((cut: { before: number; after: number; time: number; sourceGap: number }, index: number) => {
				expect(found[index * 4]).toBe(cut.before);
				expect(found[index * 4 + 1]).toBe(cut.after);
				expect(found[index * 4 + 2]).toBeCloseTo(cut.time, 12);
				expect(found[index * 4 + 3]).toBeCloseTo(cut.sourceGap, 12);
			});
		}
		for (const c of fixture.plans) {
			const plan = jumpCutPlan(c.cut, c.beforeStart, c.afterEnd, c.fps, c.frames);
			if (c.plan === null) {
				expect(plan.length).toBe(0);
				continue;
			}
			expect([plan[0], plan[1], plan[2], plan[3], plan[4]]).toEqual(
				[c.plan.start, c.plan.frames, c.plan.duration, c.plan.fromTime, c.plan.toTime].map((value: number) => expect.closeTo(value, 12)),
			);
			expect(Array.from(plan.slice(5))).toEqual(c.plan.alphas.map((value: number) => expect.closeTo(value, 12)));
		}
		for (const c of fixture.morphs) {
			const p = picture(c, 0);
			const q = picture(c, 1);
			const motion = jumpCutMotion(p, q, c.width, c.height);
			expect([motion[0], motion[1], motion[2]], c.name).toEqual([c.work.width, c.work.height, c.work.scale]);
			const n = motion[0] * motion[1];
			for (const [x, y, u, v] of c.flow) {
				expect(Math.abs(motion[4 + y * motion[0] + x] - u), `${c.name} u ${x},${y}`).toBeLessThan(1e-6);
				expect(Math.abs(motion[4 + n + y * motion[0] + x] - v), `${c.name} v ${x},${y}`).toBeLessThan(1e-6);
			}
			for (const frame of c.frames) {
				const out = jumpCutFrame(p, q, c.width, c.height, motion, frame.alpha);
				for (const [x, y, ...want] of frame.samples) {
					for (let ch = 0; ch < 4; ch++) {
						expect(Math.abs(out[(y * c.width + x) * 4 + ch] - want[ch]), `${c.name} ${x},${y}`).toBeLessThanOrEqual(1);
					}
				}
			}
		}
	});

	it("bridges each jump cut on the track above without touching the clips", () => {
		const clips = [
			piece("a", 0, 0, 2, { "transform.scaleX": 1.5 }),
			piece("b", 2, 2.8, 1.5),
			piece("c", 3.5, 4.3, 1.5),
			piece("d", 5, 7, 3),
		];
		const tracks = {
			overlay: [{ id: "text", type: "text", name: "Text", elements: [], hidden: false }],
			main: { id: "main", type: "video", name: "Main", elements: clips, hidden: false, muted: false },
			audio: [],
		} as unknown as SceneTracks;
		const cuts = projectJumpCuts({ tracks, fps: 30 });
		expect(cuts.map((cut) => cut.time / TICKS_PER_SECOND)).toEqual([2, 5]);
		expect(cuts[0].sourceGap).toBeCloseTo(0.8, 9);
		const job = planJumpCutBridge({ tracks, cut: cuts[0], fps: 30, frames: 6 })!;
		expect(job.frames).toBe(6);
		expect(job.start).toBeCloseTo(1.9, 9);
		expect(job.fromSource).toBeCloseTo(2 - 4 / 30, 6);
		expect(job.toSource).toBeCloseTo(2.9, 6);
		const second = planJumpCutBridge({ tracks, cut: cuts[1], fps: 30, frames: 6 })!;
		const bridgeA = buildBridgeElement({ tracks, job, mediaId: "bridge-a" })!;
		expect(bridgeA.params).toMatchObject({ "transform.scaleX": 1.5, [JUMP_CUT_BRIDGE_PARAM]: true });
		expect(bridgeA.params.volume).toBeUndefined();
		const bridgeB = buildBridgeElement({ tracks, job: second, mediaId: "bridge-b" })!;
		const { tracks: after, placed } = insertBridges({
			tracks,
			bridges: [
				{ trackId: "main", element: bridgeA },
				{ trackId: "main", element: bridgeB },
			],
		});
		// One new video track, directly above the main track (below the text track), holds both.
		expect(after.overlay.map((track) => track.name)).toEqual(["Text", JUMP_CUT_TRACK_NAME]);
		expect(after.overlay[1].elements).toHaveLength(2);
		expect(placed.every((place) => place.trackId === after.overlay[1].id)).toBe(true);
		expect(after.main).toBe(tracks.main);
		expect(projectJumpCuts({ tracks: after, fps: 30 }).every((cut) => cut.smoothed)).toBe(true);
	});

	it("leaves out short clips and cuts to another file", () => {
		const other = { ...piece("o", 1.1, 0, 1), mediaId: "other" } as VideoElement;
		const tracks = {
			overlay: [],
			main: { id: "main", type: "video", name: "Main", elements: [piece("x", 0, 0, 0.1), piece("y", 0.1, 1, 1), other], hidden: false, muted: false },
			audio: [],
		} as unknown as SceneTracks;
		const cuts = projectJumpCuts({ tracks, fps: 30 });
		expect(cuts).toHaveLength(1);
		expect(planJumpCutBridge({ tracks, cut: cuts[0], fps: 30, frames: 6 })!.frames).toBe(5);
	});
});
