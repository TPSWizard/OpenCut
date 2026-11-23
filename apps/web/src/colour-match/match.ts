/**
 * Colour match: one clip's colours moved onto a reference frame's, by the
 * mean and spread of each CIELAB channel (Reinhard transfer), baked into a
 * look-up table for the existing `lut` effect so both renderers draw it
 * unchanged. `crates/editor/src/colour_match.rs` does the same, and both
 * are checked against `fixtures/colour-match/cases.json`.
 */

import { encodeLutParam, type LutTable } from "@/luts/parse";

const WHITE = [0.95047, 1, 1.08883] as const;
const D = 6 / 29;

function decode(c: number): number {
	return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function encode(c: number): number {
	const v = Math.min(1, Math.max(0, c));
	return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
}

function f(t: number): number {
	return t > D * D * D ? Math.cbrt(t) : t / (3 * D * D) + 4 / 29;
}

function finv(t: number): number {
	return t > D ? t * t * t : 3 * D * D * (t - 4 / 29);
}

/** sRGB in 0..1 to CIELAB (D65). */
export function rgbToLab(r: number, g: number, b: number): [number, number, number] {
	const [lr, lg, lb] = [decode(r), decode(g), decode(b)];
	const x = 0.4124564 * lr + 0.3575761 * lg + 0.1804375 * lb;
	const y = 0.2126729 * lr + 0.7151522 * lg + 0.072175 * lb;
	const z = 0.0193339 * lr + 0.119192 * lg + 0.9503041 * lb;
	const [fx, fy, fz] = [f(x / WHITE[0]), f(y / WHITE[1]), f(z / WHITE[2])];
	return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)];
}

/** CIELAB (D65) to sRGB in 0..1, clamped. */
export function labToRgb(l: number, a: number, b: number): [number, number, number] {
	const fy = (l + 16) / 116;
	const [x, y, z] = [WHITE[0] * finv(fy + a / 500), WHITE[1] * finv(fy), WHITE[2] * finv(fy - b / 200)];
	return [
		encode(3.2404542 * x - 1.5371385 * y - 0.4985314 * z),
		encode(-0.969266 * x + 1.8760108 * y + 0.041556 * z),
		encode(0.0556434 * x - 0.2040259 * y + 1.0572252 * z),
	];
}

/** Mean and spread of each Lab channel. */
export interface ColourStats {
	count: number;
	mean: [number, number, number];
	std: [number, number, number];
}

/** Gathers Lab statistics over pixels from one or more frames. */
export class ColourAccumulator {
	private count = 0;
	private readonly sum = [0, 0, 0];
	private readonly sumSq = [0, 0, 0];

	/** Adds RGBA bytes (canvas `ImageData` order); pixels under half alpha are skipped. */
	addRgba(data: ArrayLike<number>): void {
		for (let index = 0; index + 3 < data.length; index += 4) {
			if (data[index + 3] < 128) continue;
			this.addLab(rgbToLab(data[index] / 255, data[index + 1] / 255, data[index + 2] / 255));
		}
	}

	private addLab(lab: [number, number, number]): void {
		this.count++;
		for (let channel = 0; channel < 3; channel++) {
			this.sum[channel] += lab[channel];
			this.sumSq[channel] += lab[channel] * lab[channel];
		}
	}

	/** The statistics, or null with no pixels. */
	stats(): ColourStats | null {
		if (this.count === 0) return null;
		const n = this.count;
		const mean = this.sum.map((value) => value / n) as [number, number, number];
		const std = this.sumSq.map((value, channel) => Math.sqrt(Math.max(0, value / n - mean[channel] ** 2))) as [number, number, number];
		return { count: n, mean, std };
	}
}

/** How far a channel's spread may be stretched or squeezed. */
export const MIN_SPREAD_RATIO = 0.5;
export const MAX_SPREAD_RATIO = 2;

type Moments = Pick<ColourStats, "mean" | "std">;

/** One colour moved from `source`'s statistics onto `reference`'s. */
export function transferColour(rgb: readonly [number, number, number], source: Moments, reference: Moments): [number, number, number] {
	const lab = rgbToLab(rgb[0], rgb[1], rgb[2]);
	const out = lab.map((value, channel) => {
		const ratio =
			source.std[channel] < 1e-6 ? 1 : Math.min(MAX_SPREAD_RATIO, Math.max(MIN_SPREAD_RATIO, reference.std[channel] / source.std[channel]));
		return reference.mean[channel] + (value - source.mean[channel]) * ratio;
	});
	return labToRgb(out[0], out[1], out[2]);
}

/** Points a side of the generated table (the transfer is smooth). */
export const COLOUR_MATCH_LUT_SIZE = 17;

/** The transfer as a look-up table, red varying fastest. */
export function colourMatchLut(source: Moments, reference: Moments, size = COLOUR_MATCH_LUT_SIZE): LutTable {
	const rgb = new Float32Array(size * size * size * 3);
	const n = Math.max(1, size - 1);
	let index = 0;
	for (let b = 0; b < size; b++) {
		for (let g = 0; g < size; g++) {
			for (let r = 0; r < size; r++) {
				const out = transferColour([r / n, g / n, b / n], source, reference);
				rgb[index++] = out[0];
				rgb[index++] = out[1];
				rgb[index++] = out[2];
			}
		}
	}
	return { size, rgb };
}

/** The name the generated table carries, which marks a colour-match effect. */
export const COLOUR_MATCH_NAME = "Colour match";

/** The `lut` effect parameter for a match. */
export function colourMatchParam(source: Moments, reference: Moments): string {
	return encodeLutParam({ name: COLOUR_MATCH_NAME, table: colourMatchLut(source, reference) });
}

/** Whether a `lut` parameter came from colour match. */
export function isColourMatchParam(param: unknown): boolean {
	return typeof param === "string" && param.startsWith(`${COLOUR_MATCH_NAME}|`);
}
