import { describe, expect, test } from "vitest";
import {
	axisKeys,
	isNoopGeometry,
	planPath,
	planReframe,
	reframeGeometry,
	shotSubjects,
	shrinkToRgb,
	subjectFromDetections,
	type ReframeSample,
} from "../path";

const portrait = reframeGeometry({ sourceWidth: 1920, sourceHeight: 1080, canvasWidth: 1080, canvasHeight: 1920 });
const sample = (time: number, x: number | null): ReframeSample => ({ time, subject: x === null ? null : { x, y: 0.5 } });

describe("reframe path", () => {
	test("geometry of a landscape clip on a portrait canvas", () => {
		expect(portrait.scale).toBeCloseTo((1920 / 1080) ** 2, 9);
		expect(portrait.visible.x).toBeCloseTo(0.31640625, 9);
		expect(portrait.visible.y).toBe(1);
		expect(portrait.shownHeight).toBeCloseTo(1920, 9);
		expect(
			isNoopGeometry({
				geometry: reframeGeometry({ sourceWidth: 1920, sourceHeight: 1080, canvasWidth: 1280, canvasHeight: 720 }),
			}),
		).toBe(true);
	});

	test("largest faces lead", () => {
		const subject = subjectFromDetections({
			detections: [
				{ x: 0.6, y: 0.2, width: 0.2, height: 0.3, score: 0.9 },
				{ x: 0, y: 0, width: 0.05, height: 0.05, score: 0.95 },
				{ x: 0.1, y: 0.1, width: 0.4, height: 0.4, score: 0.2 },
			],
		});
		expect(subject?.x).toBeCloseTo(0.7, 9);
		expect(subject?.y).toBeCloseTo(0.35, 9);
	});

	test("a still subject gets one fixed crop", () => {
		const samples = Array.from({ length: 50 }, (_, i) => sample(i * 0.2, 0.7 + (i % 3) * 0.01));
		const plan = planReframe({ samples, cuts: [], geometry: portrait, camera: "smooth" });
		expect(plan.keysX).toEqual([]);
		expect(plan.keysY).toEqual([]);
		expect(plan.positionX).toBeCloseTo((0.5 - 0.71) * portrait.shownWidth, 6);
	});

	test("a moving subject is followed and stays framed", () => {
		const samples = Array.from({ length: 61 }, (_, i) => sample(i * 0.1, 0.2 + (0.6 * i) / 60));
		const shots = planPath({ samples, cuts: [], visible: portrait.visible, camera: "smooth" });
		const path = shots[0];
		for (let i = 1; i < path.length; i++) expect(path[i].x).toBeGreaterThanOrEqual(path[i - 1].x - 1e-3);
		path.forEach((point, i) => {
			expect(Math.abs(point.x - (samples[i].subject?.x ?? 0))).toBeLessThan(portrait.visible.x / 2);
		});
		const keys = axisKeys({ shots, axis: (p) => p.x });
		expect(keys.length).toBeGreaterThanOrEqual(2);
		expect(keys.length).toBeLessThan(20);
	});

	test("shots cut sharply and gaps are filled", () => {
		const samples = [
			...Array.from({ length: 20 }, (_, i) => sample(i * 0.1, 0.25)),
			...Array.from({ length: 20 }, (_, j) => sample((j + 20) * 0.1, (j + 20) % 4 === 0 ? null : 0.75)),
		];
		const plan = planReframe({ samples, cuts: [2], geometry: portrait, camera: "smooth" });
		const x = (c: number) => (0.5 - c) * portrait.shownWidth;
		expect(plan.shots).toBe(2);
		expect(plan.keysX.length).toBe(3);
		expect(plan.keysX[0]).toEqual([0, x(0.25)]);
		expect(plan.keysX[1][0]).toBeCloseTo(1.999, 9);
		expect(plan.keysX[2][0]).toBeCloseTo(2, 9);
		expect(plan.keysX[2][1]).toBeCloseTo(x(0.75), 9);
	});

	test("matches the desktop reference (crates/editor/src/reframe.rs parity test)", () => {
		const samples: ReframeSample[] = Array.from({ length: 41 }, (_, i) => {
			const t = i * 0.15;
			return { time: t, subject: i % 7 === 3 ? null : { x: 0.3 + 0.4 * (t / 6) + 0.02 * Math.sin(t * 3), y: 0.45 } };
		});
		const plan = planReframe({ samples, cuts: [3], geometry: portrait, camera: "smooth" });
		const rounded = plan.keysX.map(([t, v]) => [Math.round(t * 1000) / 1000, Math.round(v * 100) / 100]);
		expect(rounded).toEqual([
			[0, 652.53],
			[0.6, 522.15],
			[1.65, 333.56],
			[2.85, -33.24],
			[2.999, -33.24],
			[3, 3.59],
			[3.75, -148.13],
			[4.8, -434.52],
			[6, -615.72],
		]);
		expect(plan.scale).toBeCloseTo(3.160494, 6);
	});

	test("the subject is what stands out from the shot (crates/editor/src/reframe.rs walking_frames)", () => {
		const w = 40;
		const h = 24;
		const frames = Array.from({ length: 10 }, (_, i) => {
			const f = new Uint8Array(w * h * 3);
			for (let row = 0; row < h; row++) {
				for (let col = 0; col < w; col++) {
					const p = (row * w + col) * 3;
					f.set(row >= 17 ? [80, 70, 60] : [96, 110, 128], p);
					const left = 4 + i * 3;
					if (col >= left && col < left + 5 && row >= 5 && row < 11) f.set([205, 150, 120], p);
				}
			}
			return f;
		});
		shotSubjects({ frames, width: w, height: h }).forEach((subject, i) => {
			expect(Math.abs((subject?.x ?? -1) - (4 + i * 3 + 2.5) / w)).toBeLessThan(0.03);
		});
		expect(shotSubjects({ frames: [frames[2], frames[2], frames[2], frames[2]], width: w, height: h })[0]).not.toBeNull();
		const deep = new Uint8Array(w * h * 3).fill(30);
		for (let row = 2; row < 8; row++) for (let col = 2; col < 8; col++) deep.set([120, 80, 60], (row * w + col) * 3);
		expect(shotSubjects({ frames: [deep], width: w, height: h })[0]?.x ?? 1).toBeLessThan(0.25);
		expect(shotSubjects({ frames: [new Uint8Array(w * h * 3).fill(30)], width: w, height: h })).toEqual([null]);
	});

	test("frames shrink by box average", () => {
		const rgba = new Uint8ClampedArray([0, 0, 0, 255, 100, 100, 100, 255, 200, 200, 200, 255, 100, 100, 100, 255]);
		const small = shrinkToRgb({ rgba, width: 2, height: 2, factor: 2 });
		expect(Array.from(small.rgb)).toEqual([100, 100, 100]);
		expect([small.width, small.height]).toEqual([1, 1]);
	});
});
