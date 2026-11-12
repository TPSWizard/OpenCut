/**
 * Auto-reframe: a clip shot for one shape (16:9) is scaled to cover a
 * canvas of another (9:16) and panned so the subject stays in view. The
 * samples carry where the subject is (faces, or movement and skin tone when
 * no face is found); this module turns them into a camera path: per shot, a
 * fixed crop when the subject stays put, otherwise a smoothed pan (a
 * constant-velocity Kalman filter run forwards and backwards), cut sharply
 * at shot changes. The desktop editor's `crates/editor/src/reframe.rs` is
 * the same arithmetic; keep the two in step.
 */

export interface Detection {
	x: number;
	y: number;
	width: number;
	height: number;
	score: number;
}

export interface Subject {
	x: number;
	y: number;
}

/** Clip seconds (from the element's start) and the subject found there. */
export interface ReframeSample {
	time: number;
	subject: Subject | null;
}

/** Faces the detector scores lower are ignored (MediaPipe's own default is 0.5). */
export const FACE_MIN_SCORE = 0.35;

export type ReframeCamera = "steady" | "smooth" | "follow";

export const REFRAME_CAMERAS: ReframeCamera[] = ["steady", "smooth", "follow"];

const PROCESS_NOISE: Record<ReframeCamera, number> = { steady: 0, smooth: 2e-3, follow: 3e-2 };
const MEASUREMENT_NOISE = 4e-4;
const STEADY_SHARE = 0.3;
const KEY_TOLERANCE = 0.004;
const CUT_GAP_SECONDS = 0.001;
const HEADROOM = 0.12;

/** The larger boxes (at least 40% of the largest), averaged by area and confidence. */
export function subjectFromDetections({
	detections,
	minScore = 0.5,
}: {
	detections: Detection[];
	minScore?: number;
}): Subject | null {
	const kept = detections.filter((d) => d.score >= minScore && d.width > 0 && d.height > 0);
	const largest = kept.reduce((max, d) => Math.max(max, d.width * d.height), 0);
	if (largest <= 0) return null;
	let sumX = 0;
	let sumY = 0;
	let sumW = 0;
	for (const d of kept) {
		if (d.width * d.height < largest * 0.4) continue;
		const weight = d.width * d.height * d.score;
		sumX += (d.x + d.width / 2) * weight;
		sumY += (d.y + d.height / 2) * weight;
		sumW += weight;
	}
	return { x: clamp(sumX / sumW, 0, 1), y: clamp(sumY / sumW, 0, 1) };
}

export interface ReframeGeometry {
	/** Element scale over the editor's contain fit that covers the canvas. */
	scale: number;
	/** Share of the source's width and height the canvas shows. */
	visible: { x: number; y: number };
	shownWidth: number;
	shownHeight: number;
}

export function reframeGeometry({
	sourceWidth,
	sourceHeight,
	canvasWidth,
	canvasHeight,
}: {
	sourceWidth: number;
	sourceHeight: number;
	canvasWidth: number;
	canvasHeight: number;
}): ReframeGeometry {
	const fitX = canvasWidth / sourceWidth;
	const fitY = canvasHeight / sourceHeight;
	const cover = Math.max(fitX, fitY);
	const source = sourceWidth / sourceHeight;
	const canvas = canvasWidth / canvasHeight;
	return {
		scale: cover / Math.min(fitX, fitY),
		visible: { x: Math.min(1, canvas / source), y: Math.min(1, source / canvas) },
		shownWidth: sourceWidth * cover,
		shownHeight: sourceHeight * cover,
	};
}

export function isNoopGeometry({ geometry }: { geometry: ReframeGeometry }): boolean {
	return geometry.visible.x > 0.999 && geometry.visible.y > 0.999;
}

export interface PathPoint {
	time: number;
	x: number;
	y: number;
}

/** The camera path: shots split at `cuts` (clip seconds), each held or panned on its own. */
export function planPath({
	samples,
	cuts,
	visible,
	camera,
}: {
	samples: ReframeSample[];
	cuts: number[];
	visible: { x: number; y: number };
	camera: ReframeCamera;
}): PathPoint[][] {
	const sorted = samples.filter((s) => Number.isFinite(s.time)).sort((a, b) => a.time - b.time);
	const sortedCuts = cuts.filter(Number.isFinite).sort((a, b) => a - b);
	const shots: ReframeSample[][] = [[]];
	let cutIndex = 0;
	for (const sample of sorted) {
		while (cutIndex < sortedCuts.length && sample.time >= sortedCuts[cutIndex]) {
			cutIndex += 1;
			if (shots[shots.length - 1].length > 0) shots.push([]);
		}
		shots[shots.length - 1].push(sample);
	}
	return shots
		.filter((shot) => shot.length > 0)
		.map((shot) => {
			const times = shot.map((s) => s.time);
			const xs = shot.map((s) => (s.subject ? s.subject.x : null));
			const ys = shot.map((s) => (s.subject ? s.subject.y + HEADROOM * visible.y : null));
			const x = planAxis({ times, values: xs, visible: visible.x, camera });
			const y = planAxis({ times, values: ys, visible: visible.y, camera });
			return times.map((time, i) => ({ time, x: x[i], y: y[i] }));
		});
}

function planAxis({
	times,
	values,
	visible,
	camera,
}: {
	times: number[];
	values: Array<number | null>;
	visible: number;
	camera: ReframeCamera;
}): number[] {
	const half = visible / 2;
	if (half >= 0.5 - 1e-9) return times.map(() => 0.5);
	const keep = (value: number) => clamp(value, half, 1 - half);
	const filled = fillGaps(values);
	if (!filled) return times.map(() => 0.5);
	const low = Math.min(...filled);
	const high = Math.max(...filled);
	if (camera === "steady" || high - low <= visible * STEADY_SHARE) {
		const centre = high - low <= visible ? (low + high) / 2 : median(filled);
		return times.map(() => keep(centre));
	}
	return smooth({ times, measured: filled, q: PROCESS_NOISE[camera] }).map(keep);
}

function fillGaps(values: Array<number | null>): number[] | null {
	const known: Array<[number, number]> = [];
	values.forEach((value, index) => {
		if (value !== null) known.push([index, value]);
	});
	if (known.length === 0) return null;
	const first = known[0];
	const last = known[known.length - 1];
	const out: number[] = [];
	let segment = 0;
	for (let i = 0; i < values.length; i++) {
		if (i <= first[0]) out.push(first[1]);
		else if (i >= last[0]) out.push(last[1]);
		else {
			while (known[segment + 1][0] < i) segment += 1;
			const a = known[segment];
			const b = known[segment + 1];
			const t = (i - a[0]) / (b[0] - a[0]);
			out.push(a[1] + (b[1] - a[1]) * t);
		}
	}
	return out;
}

function median(values: number[]): number {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted[Math.floor(sorted.length / 2)];
}

/** Constant-velocity Kalman filter, then the Rauch–Tung–Striebel pass backwards. */
function smooth({ times, measured, q }: { times: number[]; measured: number[]; q: number }): number[] {
	const n = measured.length;
	if (n < 2) return [...measured];
	const r = MEASUREMENT_NOISE;
	const xf: Array<[number, number]> = new Array(n);
	const pf: Array<[number, number, number]> = new Array(n);
	const xp: Array<[number, number]> = new Array(n);
	const pp: Array<[number, number, number]> = new Array(n);
	xp[0] = [measured[0], 0];
	pp[0] = [r, 0, 1];
	for (let k = 0; k < n; k++) {
		if (k > 0) {
			const dt = Math.max(1e-3, times[k] - times[k - 1]);
			const [x, v] = xf[k - 1];
			const [a, b, d] = pf[k - 1];
			xp[k] = [x + v * dt, v];
			pp[k] = [
				a + 2 * dt * b + dt * dt * d + (q * dt ** 3) / 3,
				b + dt * d + (q * dt * dt) / 2,
				d + q * dt,
			];
		}
		const [a, b, d] = pp[k];
		const innovation = measured[k] - xp[k][0];
		const s = a + r;
		const k0 = a / s;
		const k1 = b / s;
		xf[k] = [xp[k][0] + k0 * innovation, xp[k][1] + k1 * innovation];
		pf[k] = [(1 - k0) * a, (1 - k0) * b, d - k1 * b];
	}
	const xs = xf.map((x) => [...x] as [number, number]);
	const ps = pf.map((p) => [...p] as [number, number, number]);
	for (let k = n - 2; k >= 0; k--) {
		const dt = Math.max(1e-3, times[k + 1] - times[k]);
		const [a, b, d] = pf[k];
		const fa = a + dt * b;
		const fb = b;
		const fc = b + dt * d;
		const fd = d;
		const [pa, pb, pd] = pp[k + 1];
		const det = pa * pd - pb * pb;
		if (Math.abs(det) < 1e-18) continue;
		const ia = pd / det;
		const ib = -pb / det;
		const id = pa / det;
		const c = [fa * ia + fb * ib, fa * ib + fb * id, fc * ia + fd * ib, fc * ib + fd * id];
		const dx = [xs[k + 1][0] - xp[k + 1][0], xs[k + 1][1] - xp[k + 1][1]];
		xs[k] = [xf[k][0] + c[0] * dx[0] + c[1] * dx[1], xf[k][1] + c[2] * dx[0] + c[3] * dx[1]];
		const dp = [ps[k + 1][0] - pa, ps[k + 1][1] - pb, ps[k + 1][2] - pd];
		const t = [
			c[0] * dp[0] + c[1] * dp[1],
			c[0] * dp[1] + c[1] * dp[2],
			c[2] * dp[0] + c[3] * dp[1],
			c[2] * dp[1] + c[3] * dp[2],
		];
		ps[k] = [a + t[0] * c[0] + t[1] * c[1], b + t[0] * c[2] + t[1] * c[3], d + t[2] * c[2] + t[3] * c[3]];
	}
	return xs.map(([x]) => x);
}

/** Keys for one axis, `[clip seconds, value]`: few points per shot, a sharp step at each cut. */
export function axisKeys({
	shots,
	axis,
}: {
	shots: PathPoint[][];
	axis: (point: PathPoint) => number;
}): Array<[number, number]> {
	let keys: Array<[number, number]> = [];
	shots.forEach((shot, index) => {
		const points: Array<[number, number]> = shot.map((p) => [p.time, axis(p)]);
		if (points.length === 0) return;
		const [firstTime, firstValue] = points[0];
		const previous = keys[keys.length - 1];
		if (previous) keys.push([Math.max(previous[0], firstTime - CUT_GAP_SECONDS), previous[1]]);
		const start = index === 0 ? Math.min(0, firstTime) : firstTime;
		keys.push([start, firstValue]);
		for (const [time, value] of simplify(points)) {
			if (time > start + 1e-9) keys.push([time, value]);
		}
	});
	const first = keys[0]?.[1] ?? 0;
	if (keys.every((key) => Math.abs(key[1] - first) < 1e-6)) return [];
	const deduped: Array<[number, number]> = [];
	for (const key of keys) {
		const last = deduped[deduped.length - 1];
		if (last && Math.abs(key[0] - last[0]) < 1e-9) last[1] = key[1];
		else deduped.push([key[0], key[1]]);
	}
	keys = deduped;
	return prune(keys);
}

function prune(keys: Array<[number, number]>): Array<[number, number]> {
	const out: Array<[number, number]> = [];
	keys.forEach((key, index) => {
		const previous = out[out.length - 1];
		const next = keys[index + 1];
		if (previous && next) {
			const along = next[0] > previous[0] ? (key[0] - previous[0]) / (next[0] - previous[0]) : 0;
			if (Math.abs(previous[1] + (next[1] - previous[1]) * along - key[1]) < 1e-9) return;
		}
		out.push(key);
	});
	while (out.length >= 2 && Math.abs(out[out.length - 1][1] - out[out.length - 2][1]) < 1e-9) out.pop();
	return out;
}

/** Ramer–Douglas–Peucker on (time, value) with a value tolerance. */
function simplify(points: Array<[number, number]>): Array<[number, number]> {
	if (points.length <= 2) return [...points];
	const keep = points.map(() => false);
	keep[0] = true;
	keep[points.length - 1] = true;
	const stack: Array<[number, number]> = [[0, points.length - 1]];
	while (stack.length > 0) {
		const [a, b] = stack.pop() as [number, number];
		const [ta, va] = points[a];
		const [tb, vb] = points[b];
		let worst = 0;
		let worstIndex = a;
		for (let i = a + 1; i < b; i++) {
			const [t, v] = points[i];
			const along = tb > ta ? (t - ta) / (tb - ta) : 0;
			const error = Math.abs(v - (va + (vb - va) * along));
			if (error > worst) {
				worst = error;
				worstIndex = i;
			}
		}
		if (worst > KEY_TOLERANCE) {
			keep[worstIndex] = true;
			stack.push([a, worstIndex]);
			stack.push([worstIndex, b]);
		}
	}
	return points.filter((_, i) => keep[i]);
}

export interface ReframePlan {
	scale: number;
	positionX: number;
	positionY: number;
	/** `[clip seconds, canvas pixels]`; empty when the axis holds still. */
	keysX: Array<[number, number]>;
	keysY: Array<[number, number]>;
	shots: number;
	samplesWithSubject: number;
}

export function planReframe({
	samples,
	cuts,
	geometry,
	camera,
}: {
	samples: ReframeSample[];
	cuts: number[];
	geometry: ReframeGeometry;
	camera: ReframeCamera;
}): ReframePlan {
	const shots = planPath({ samples, cuts, visible: geometry.visible, camera });
	const first = shots[0]?.[0] ?? { time: 0, x: 0.5, y: 0.5 };
	const toX = (centre: number) => (0.5 - centre) * geometry.shownWidth;
	const toY = (centre: number) => (0.5 - centre) * geometry.shownHeight;
	return {
		scale: geometry.scale,
		positionX: toX(first.x),
		positionY: toY(first.y),
		keysX: axisKeys({ shots, axis: (p) => p.x }).map(([t, v]) => [t, toX(v)]),
		keysY: axisKeys({ shots, axis: (p) => p.y }).map(([t, v]) => [t, toY(v)]),
		shots: shots.length,
		samplesWithSubject: samples.filter((s) => s.subject !== null).length,
	};
}

/** Width of the small frames `shotSubjects` works on (height follows the source). */
export const SUBJECT_FRAME_WIDTH = 80;

function luma(r: number, g: number, b: number): number {
	return 0.299 * r + 0.587 * g + 0.114 * b;
}

/** A chrominance range that holds across skin tones (lighting, and brown or wooden backgrounds, can still fool it). */
function isSkin(r: number, g: number, b: number): boolean {
	const y = 0.299 * r + 0.587 * g + 0.114 * b;
	const cb = 128 - 0.168736 * r - 0.331264 * g + 0.5 * b;
	const cr = 128 + 0.5 * r - 0.418688 * g - 0.081312 * b;
	return y > 40 && cb >= 77 && cb <= 127 && cr >= 133 && cr <= 173;
}

/**
 * Subject guesses for one shot without faces, from small RGB frames of equal
 * size: each pixel's median brightness over the shot is the background, and
 * the subject is the centre of what differs from it (skin-coloured pixels
 * count three times). A shot where nothing stands out falls back to the
 * centre of the skin-coloured pixels; `null` where neither finds enough.
 * The desktop editor's `shot_subjects` is the same.
 */
export function shotSubjects({
	frames,
	width,
	height,
}: {
	frames: Uint8Array[];
	width: number;
	height: number;
}): Array<Subject | null> {
	const pixels = width * height;
	if (pixels === 0 || frames.some((f) => f.length < pixels * 3)) return frames.map(() => null);
	let background: Float64Array | null = null;
	if (frames.length >= 3) {
		background = new Float64Array(pixels);
		const column = new Float64Array(frames.length);
		for (let p = 0; p < pixels; p++) {
			frames.forEach((frame, i) => {
				column[i] = luma(frame[p * 3], frame[p * 3 + 1], frame[p * 3 + 2]);
			});
			const sorted = Array.from(column).sort((a, b) => a - b);
			background[p] = sorted[Math.floor(sorted.length / 2)];
		}
	}
	return frames.map((frame) => {
		let fx = 0;
		let fy = 0;
		let fw = 0;
		let sx = 0;
		let sy = 0;
		let sw = 0;
		for (let p = 0; p < pixels; p++) {
			const r = frame[p * 3];
			const g = frame[p * 3 + 1];
			const b = frame[p * 3 + 2];
			const skin = isSkin(r, g, b);
			const col = (p % width) + 0.5;
			const row = Math.floor(p / width) + 0.5;
			if (background && Math.abs(luma(r, g, b) - background[p]) > 25) {
				const weight = skin ? 3 : 1;
				fx += col * weight;
				fy += row * weight;
				fw += weight;
			}
			if (skin) {
				sx += col;
				sy += row;
				sw += 1;
			}
		}
		if (fw >= pixels * 0.004) return { x: fx / fw / width, y: fy / fw / height };
		if (sw >= pixels * 0.005) return { x: sx / sw / width, y: sy / sw / height };
		return null;
	});
}

/** An RGBA frame shrunk by a whole factor (box average) to tightly packed RGB. */
export function shrinkToRgb({
	rgba,
	width,
	height,
	factor,
}: {
	rgba: Uint8ClampedArray | Uint8Array;
	width: number;
	height: number;
	factor: number;
}): { rgb: Uint8Array; width: number; height: number } {
	const outWidth = Math.max(1, Math.floor(width / factor));
	const outHeight = Math.max(1, Math.floor(height / factor));
	const rgb = new Uint8Array(outWidth * outHeight * 3);
	const area = factor * factor;
	for (let row = 0; row < outHeight; row++) {
		for (let col = 0; col < outWidth; col++) {
			let r = 0;
			let g = 0;
			let b = 0;
			for (let dy = 0; dy < factor; dy++) {
				for (let dx = 0; dx < factor; dx++) {
					const i = ((row * factor + dy) * width + col * factor + dx) * 4;
					r += rgba[i];
					g += rgba[i + 1];
					b += rgba[i + 2];
				}
			}
			const o = (row * outWidth + col) * 3;
			rgb[o] = Math.round(r / area);
			rgb[o + 1] = Math.round(g / area);
			rgb[o + 2] = Math.round(b / area);
		}
	}
	return { rgb, width: outWidth, height: outHeight };
}

function clamp(value: number, low: number, high: number): number {
	return Math.min(high, Math.max(low, value));
}
