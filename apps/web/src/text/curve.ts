import { bendText, bendTextAlongPath, bentTextSize, textPathPoints, textPathSize } from "opencut-wasm";
import { resolveAnimationPathValueAtTime } from "@/animation/resolve";
import type { ElementAnimations } from "@/animation/types";
import type { ParamValues } from "@/params";

/**
 * Curved text: the `curve` text param, -100 (a smile: the ends rise) to
 * 100 (an arch: the ends drop); 0 is flat. The flat text (glyphs, box,
 * highlights, lines) is drawn off-screen and bent along a circular arc by
 * the shared Rust `textwarp` crate through WASM, as the desktop renderer
 * bends its raster, so both editors draw the same curve. At 100 the text's
 * width wraps 350 degrees. Checked against `fixtures/curved-text/cases.json`.
 */

export const CURVE_MIN = -100;
export const CURVE_MAX = 100;
/** Curves closer to 0 than this are drawn flat (`textwarp::MIN_CURVE`). */
export const CURVE_FLAT = 0.5;
/** The most device pixels a bent text raster may have on a side. */
const MAX_SIDE = 8192;

export function readCurve(value: unknown): number {
	return typeof value === "number" && Number.isFinite(value) ? Math.min(CURVE_MAX, Math.max(CURVE_MIN, value)) : 0;
}

/** The curve at an element-relative time, with its keyframes. */
export function curveAt({
	params,
	animations,
	localTime,
}: {
	params: ParamValues | undefined;
	animations: ElementAnimations | undefined;
	localTime: number;
}): number {
	const base = readCurve(params?.curve);
	return readCurve(
		resolveAnimationPathValueAtTime({ animations, propertyPath: "curve", localTime, fallbackValue: base }),
	);
}

export function isCurved(curve: number): boolean {
	return Math.abs(curve) >= CURVE_FLAT;
}

export interface Rect {
	left: number;
	top: number;
	width: number;
	height: number;
}

/**
 * Text on a path (row 91): the control points (flattened x, y pairs, in
 * units of the text's width around its anchor) of the text's `textPath`
 * shape, or null for none. Laid along it by `textwarp::warp_path`, the
 * same as the desktop; it takes the place of the curve.
 */
export function textPathAt(params: ParamValues | undefined): Float64Array | null {
	const preset = typeof params?.textPath === "string" ? params.textPath : "none";
	if (preset === "none") return null;
	const amount = typeof params?.textPathAmount === "number" ? params.textPathAmount : 50;
	const custom = typeof params?.textPathPoints === "string" ? params.textPathPoints : "";
	const points = textPathPoints(preset, amount, custom);
	return points.length >= 8 ? points : null;
}

/** The rect of `rect` bent by `curve` (or laid along `path`), centred on the same point. */
export function bentRect(rect: Rect, curve: number, path?: Float64Array | null): Rect {
	if (path) {
		const [width, height] = textPathSize(path, Math.max(1, Math.round(rect.width)), Math.max(1, Math.round(rect.height)));
		const cx = rect.left + rect.width / 2;
		const cy = rect.top + rect.height / 2;
		return { left: cx - width / 2, top: cy - height / 2, width, height };
	}
	if (!isCurved(curve)) return rect;
	const [width, height] = bentTextSize(curve, Math.max(1, Math.round(rect.width)), Math.max(1, Math.round(rect.height)));
	const cx = rect.left + rect.width / 2;
	const cy = rect.top + rect.height / 2;
	return { left: cx - width / 2, top: cy - height / 2, width, height };
}

type Context2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

function makeCanvas(width: number, height: number): OffscreenCanvas | HTMLCanvasElement {
	if (typeof OffscreenCanvas !== "undefined") return new OffscreenCanvas(width, height);
	const canvas = document.createElement("canvas");
	canvas.width = width;
	canvas.height = height;
	return canvas;
}

/**
 * Draws text bent by `curve` into `ctx` (already moved to the text's
 * anchor, scaled and turned). `rect` is the flat text's extent around the
 * anchor in text units; `drawFlat` draws the flat text there. The flat text
 * is rasterised at the device pixel size the context shows it at, bent
 * about the rect's centre, and drawn back centred on it.
 */
export function drawCurvedText({
	ctx,
	curve,
	path = null,
	rect,
	drawFlat,
}: {
	ctx: Context2D;
	curve: number;
	/** Text on a path: lays the text along it instead of the arc. */
	path?: Float64Array | null;
	rect: Rect;
	drawFlat: (ctx: Context2D) => void;
}): void {
	const matrix = ctx.getTransform();
	const deviceScale = Math.max(Math.hypot(matrix.a, matrix.b), Math.hypot(matrix.c, matrix.d));
	const pad = 2;
	let scale = Math.min(8, Math.max(0.05, deviceScale || 1));
	const longest = Math.max(rect.width, rect.height) + 2 * pad;
	if (longest * scale > MAX_SIDE) scale = MAX_SIDE / longest;
	const width = Math.max(1, Math.ceil((rect.width + 2 * pad) * scale));
	const height = Math.max(1, Math.ceil((rect.height + 2 * pad) * scale));
	const cx = rect.left + rect.width / 2;
	const cy = rect.top + rect.height / 2;

	const flat = makeCanvas(width, height);
	const flatCtx = flat.getContext("2d") as Context2D | null;
	if (!flatCtx) return;
	flatCtx.setTransform(scale, 0, 0, scale, width / 2 - cx * scale, height / 2 - cy * scale);
	drawFlat(flatCtx);
	if (path) {
		const flatPixels = flatCtx.getImageData(0, 0, width, height);
		const [pathWidth, pathHeight] = textPathSize(path, width, height);
		const laid = bendTextAlongPath(path, width, height, new Uint8Array(flatPixels.data.buffer));
		if (laid.length === pathWidth * pathHeight * 4) {
			const out = makeCanvas(pathWidth, pathHeight);
			const outCtx = out.getContext("2d") as Context2D | null;
			if (!outCtx) return;
			const data = new Uint8ClampedArray(laid.length);
			data.set(laid);
			outCtx.putImageData(new ImageData(data, pathWidth, pathHeight), 0, 0);
			ctx.drawImage(out, cx - pathWidth / 2 / scale, cy - pathHeight / 2 / scale, pathWidth / scale, pathHeight / scale);
			return;
		}
	}
	if (!isCurved(curve)) {
		ctx.drawImage(flat, cx - width / 2 / scale, cy - height / 2 / scale, width / scale, height / scale);
		return;
	}
	const flatPixels = flatCtx.getImageData(0, 0, width, height);
	const [bentWidth, bentHeight] = bentTextSize(curve, width, height);
	const bent = bendText(curve, width, height, new Uint8Array(flatPixels.data.buffer));
	if (bent.length !== bentWidth * bentHeight * 4) {
		ctx.drawImage(flat, cx - width / 2 / scale, cy - height / 2 / scale, width / scale, height / scale);
		return;
	}
	const out = makeCanvas(bentWidth, bentHeight);
	const outCtx = out.getContext("2d") as Context2D | null;
	if (!outCtx) return;
	const data = new Uint8ClampedArray(bent.length);
	data.set(bent);
	outCtx.putImageData(new ImageData(data, bentWidth, bentHeight), 0, 0);
	ctx.drawImage(out, cx - bentWidth / 2 / scale, cy - bentHeight / 2 / scale, bentWidth / scale, bentHeight / scale);
}
