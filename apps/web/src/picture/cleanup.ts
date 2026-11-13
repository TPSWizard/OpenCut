/**
 * Picture cleanup for cut-out illustrations (deterministic, no models):
 * remove a flat white background by flood fill from the border with a soft
 * edge, take the white back out of the edge pixels (halo removal), feather
 * the edge inwards, tint the ink, crop to the content, and trace the
 * shape's longest skeleton path (where a light pulse can travel). Follows
 * the Shortform Studio's `lib/shortform/imageops.ts`, so traced paths are
 * interchangeable; the desktop editor's `crates/editor/src/picture.rs` is
 * the same arithmetic (a parity test checks both).
 */

export interface Picture {
	width: number;
	height: number;
	/** Straight-alpha RGBA, row by row. */
	rgba: Uint8ClampedArray;
}

/** A polyline along the shape, points as fractions of the picture (0..1), top end first. */
export interface TracedPath {
	id: string;
	points: [number, number][];
}

export interface PictureCleanup {
	removeWhite: boolean;
	decontaminate: boolean;
	/** Edge softening in pixels (0 = none); only ever lowers alpha. */
	feather: number;
	/** Ink colour `#rrggbb`, or null to keep the picture's colours. */
	tint: string | null;
	crop: boolean;
	trace: boolean;
}

export const DEFAULT_PICTURE_CLEANUP: PictureCleanup = {
	removeWhite: true,
	decontaminate: true,
	feather: 1,
	tint: null,
	crop: true,
	trace: true,
};

const lum = (d: Uint8ClampedArray, i: number) => (0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]) / 255;

/** Rust's `f64::round` (half away from zero); the same as Math.round for the non-negative values here. */
const round = (v: number) => Math.sign(v) * Math.round(Math.abs(v));

export function clonePicture(picture: Picture): Picture {
	return { width: picture.width, height: picture.height, rgba: new Uint8ClampedArray(picture.rgba) };
}

export function removeFlatWhite(picture: Picture, hard = 0.93, soft = 0.8): void {
	const { width: w, height: h } = picture;
	if (w === 0 || h === 0) return;
	const d = picture.rgba;
	const seen = new Uint8Array(w * h);
	const stack: number[] = [];
	const push = (p: number) => {
		if (seen[p] || lum(d, p * 4) < soft) return;
		seen[p] = 1;
		stack.push(p);
	};
	for (let x = 0; x < w; x++) {
		push(x);
		push((h - 1) * w + x);
	}
	for (let y = 0; y < h; y++) {
		push(y * w);
		push(y * w + w - 1);
	}
	while (stack.length > 0) {
		const p = stack.pop() as number;
		const l = lum(d, p * 4);
		const a = l >= hard ? 0 : round(255 * (1 - (l - soft) / (hard - soft)));
		d[p * 4 + 3] = Math.min(d[p * 4 + 3], Math.min(255, Math.max(0, a)));
		if (l < hard) continue;
		const x = p % w;
		const y = Math.floor(p / w);
		if (x > 0) push(p - 1);
		if (x + 1 < w) push(p + 1);
		if (y > 0) push(p - w);
		if (y + 1 < h) push(p + w);
	}
}

export function decontaminateWhite(picture: Picture): void {
	const d = picture.rgba;
	for (let i = 0; i < d.length; i += 4) {
		const a = d[i + 3] / 255;
		if (a <= 0 || a >= 1) continue;
		for (let c = 0; c < 3; c++) {
			const ink = (d[i + c] - (1 - a) * 255) / a;
			d[i + c] = Math.min(255, Math.max(0, round(ink)));
		}
	}
}

export function featherAlpha(picture: Picture, radius: number): void {
	const { width: w, height: h } = picture;
	if (radius <= 0 || w === 0 || h === 0) return;
	const alpha = new Float64Array(w * h);
	for (let p = 0; p < w * h; p++) alpha[p] = picture.rgba[p * 4 + 3];
	const blurLine = (get: (i: number) => number, len: number): Float64Array => {
		const out = new Float64Array(len);
		for (let i = 0; i < len; i++) {
			const from = Math.max(0, i - radius);
			const to = Math.min(len - 1, i + radius);
			let sum = 0;
			for (let j = from; j <= to; j++) sum += get(j);
			out[i] = sum / (2 * radius + 1);
		}
		return out;
	};
	const horizontal = new Float64Array(w * h);
	for (let y = 0; y < h; y++) horizontal.set(blurLine((x) => alpha[y * w + x], w), y * w);
	for (let x = 0; x < w; x++) {
		const column = blurLine((y) => horizontal[y * w + x], h);
		for (let y = 0; y < h; y++) {
			const i = (y * w + x) * 4 + 3;
			picture.rgba[i] = Math.min(picture.rgba[i], Math.min(255, Math.max(0, round(column[y]))));
		}
	}
}

export function inkTint(picture: Picture, hex: string, gain = 1.35): void {
	const channel = (from: number) => {
		const value = Number.parseInt(hex.slice(from, from + 2), 16);
		return Number.isFinite(value) ? value : 255;
	};
	const [r, g, b] = [channel(1), channel(3), channel(5)];
	const d = picture.rgba;
	for (let i = 0; i < d.length; i += 4) {
		const dark = 1 - lum(d, i);
		const ink = Math.min(1, Math.max(0, (dark - 0.02) * gain));
		const a = (d[i + 3] / 255) * ink;
		d[i] = r;
		d[i + 1] = g;
		d[i + 2] = b;
		d[i + 3] = round(a * 255);
	}
}

export function cropToContent(picture: Picture, padFraction = 0.04, alphaMin = 12): Picture {
	const { width: w, height: h, rgba: d } = picture;
	let x0 = w;
	let y0 = h;
	let x1 = 0;
	let y1 = 0;
	let any = false;
	for (let y = 0; y < h; y++) {
		for (let x = 0; x < w; x++) {
			if (d[(y * w + x) * 4 + 3] > alphaMin) {
				any = true;
				x0 = Math.min(x0, x);
				x1 = Math.max(x1, x);
				y0 = Math.min(y0, y);
				y1 = Math.max(y1, y);
			}
		}
	}
	if (!any) return clonePicture(picture);
	const pad = round(Math.max(x1 - x0, y1 - y0) * padFraction);
	x0 = Math.max(0, x0 - pad);
	y0 = Math.max(0, y0 - pad);
	x1 = Math.min(w - 1, x1 + pad);
	y1 = Math.min(h - 1, y1 + pad);
	const cw = x1 - x0 + 1;
	const ch = y1 - y0 + 1;
	const out = new Uint8ClampedArray(cw * ch * 4);
	for (let y = 0; y < ch; y++) out.set(d.subarray(((y0 + y) * w + x0) * 4, ((y0 + y) * w + x0 + cw) * 4), y * cw * 4);
	return { width: cw, height: ch, rgba: out };
}

/** Zhang–Suen thinning of a 0/1 mask, in place. */
export function thin(m: Uint8Array, w: number, h: number): void {
	const at = (x: number, y: number) => (x < 0 || y < 0 || x >= w || y >= h ? 0 : m[y * w + x]);
	let changed = true;
	const del: number[] = [];
	while (changed) {
		changed = false;
		for (let pass = 0; pass < 2; pass++) {
			del.length = 0;
			for (let y = 0; y < h; y++) {
				for (let x = 0; x < w; x++) {
					if (!m[y * w + x]) continue;
					const p = [at(x, y - 1), at(x + 1, y - 1), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1), at(x - 1, y + 1), at(x - 1, y), at(x - 1, y - 1)];
					const b = p.reduce((s, v) => s + v, 0);
					if (b < 2 || b > 6) continue;
					let a = 0;
					for (let k = 0; k < 8; k++) if (p[k] === 0 && p[(k + 1) % 8] === 1) a++;
					if (a !== 1) continue;
					const blocked =
						pass === 0
							? p[0] * p[2] * p[4] !== 0 || p[2] * p[4] * p[6] !== 0
							: p[0] * p[2] * p[6] !== 0 || p[0] * p[4] * p[6] !== 0;
					if (blocked) continue;
					del.push(y * w + x);
				}
			}
			if (del.length > 0) changed = true;
			for (const q of del) m[q] = 0;
		}
	}
}

function bfsFarthest(m: Uint8Array, w: number, h: number, start: number): { far: number; prev: Int32Array } {
	const prev = new Int32Array(w * h).fill(-2);
	prev[start] = -1;
	const queue = [start];
	let far = start;
	for (let head = 0; head < queue.length; head++) {
		const p = queue[head];
		far = p;
		const x = p % w;
		const y = Math.floor(p / w);
		for (let dy = -1; dy <= 1; dy++) {
			for (let dx = -1; dx <= 1; dx++) {
				if (!dx && !dy) continue;
				const nx = x + dx;
				const ny = y + dy;
				if (nx < 0 || ny < 0 || nx >= w || ny >= h) continue;
				const n = ny * w + nx;
				if (m[n] && prev[n] === -2) {
					prev[n] = p;
					queue.push(n);
				}
			}
		}
	}
	return { far, prev };
}

function simplify(points: [number, number][], tolerance: number): [number, number][] {
	if (points.length <= 2) return points;
	const a = points[0];
	const b = points[points.length - 1];
	let worst = 0;
	let at = 0;
	for (let i = 1; i < points.length - 1; i++) {
		const p = points[i];
		const num = Math.abs((b[1] - a[1]) * p[0] - (b[0] - a[0]) * p[1] + b[0] * a[1] - b[1] * a[0]);
		const den = Math.hypot(b[1] - a[1], b[0] - a[0]) || 1;
		if (num / den > worst) {
			worst = num / den;
			at = i;
		}
	}
	if (worst <= tolerance) return [a, b];
	return [...simplify(points.slice(0, at + 1), tolerance).slice(0, -1), ...simplify(points.slice(at), tolerance)];
}

export function traceMainPath(picture: Picture, id = "main"): TracedPath | null {
	const { width: pw, height: ph } = picture;
	if (pw === 0 || ph === 0) return null;
	const s = Math.min(1, 220 / Math.max(pw, ph));
	const w = Math.max(2, round(pw * s));
	const h = Math.max(2, round(ph * s));
	const m = new Uint8Array(w * h);
	for (let y = 0; y < h; y++) {
		for (let x = 0; x < w; x++) {
			const ix = Math.min(pw - 1, Math.floor((x + 0.5) / s));
			const iy = Math.min(ph - 1, Math.floor((y + 0.5) / s));
			m[y * w + x] = picture.rgba[(iy * pw + ix) * 4 + 3] > 96 ? 1 : 0;
		}
	}
	const sx = pw / w;
	const sy = ph / h;
	thin(m, w, h);
	let seed = -1;
	let best = 0;
	let bestLen = 0;
	const visited = new Uint8Array(w * h);
	for (let p = 0; p < m.length; p++) {
		if (!m[p] || visited[p]) continue;
		const { far, prev } = bfsFarthest(m, w, h, p);
		prev.forEach((v, i) => {
			if (v !== -2) visited[i] = 1;
		});
		const second = bfsFarthest(m, w, h, far);
		let len = 0;
		for (let c = second.far; c !== -1; c = second.prev[c]) len++;
		if (len > bestLen) {
			bestLen = len;
			seed = far;
			best = second.far;
		}
	}
	if (seed < 0 || bestLen < 4) return null;
	const { prev } = bfsFarthest(m, w, h, seed);
	const raw: [number, number][] = [];
	for (let c = best; c >= 0; c = prev[c]) raw.push([((c % w) + 0.5) * sx, (Math.floor(c / w) + 0.5) * sy]);
	if (raw.length < 2) return null;
	if (raw[0][1] > raw[raw.length - 1][1]) raw.reverse();
	const simple = simplify(raw, Math.max(pw, ph) * 0.006);
	const r3 = (v: number) => round(v * 1000) / 1000;
	return { id, points: simple.slice(0, 64).map(([x, y]) => [r3(x / pw), r3(y / ph)] as [number, number]) };
}

export function cleanPicture(
	picture: Picture,
	options: PictureCleanup = DEFAULT_PICTURE_CLEANUP,
): { picture: Picture; path: TracedPath | null } {
	let out = clonePicture(picture);
	if (options.removeWhite) removeFlatWhite(out);
	if (options.decontaminate) decontaminateWhite(out);
	featherAlpha(out, options.feather);
	if (options.tint && /^#[0-9a-f]{6}$/i.test(options.tint)) inkTint(out, options.tint);
	if (options.crop) out = cropToContent(out);
	return { picture: out, path: options.trace ? traceMainPath(out) : null };
}
