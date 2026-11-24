import type { ElementAnimations } from "@/animation/types";
import { resolveAnimationPathValueAtTime } from "@/animation/resolve";
import type { ParamValues } from "@/params";

/**
 * Corner pin on a video, picture or sticker clip, from its params:
 * `cornerPin.enabled` and `cornerPin.<corner><X|Y>` for the corners
 * `topLeft`, `topRight`, `bottomRight` and `bottomLeft`, in percent of the
 * clip's own box before its rotation (0, 0 is its top left; 100, 100 its
 * bottom right), so moving, scaling or turning the clip carries the pin.
 * The corners are keyframable. The shared compositor draws the pin
 * (`crates/compositor/src/corner_pin.rs`); the desktop's
 * `editor::read_corner_pin` reads the same params. Checked against
 * `fixtures/corner-pin/cases.json`.
 */

export const CORNER_PIN_MIN = -500;
export const CORNER_PIN_MAX = 600;
export const CORNER_PIN_ENABLED = "cornerPin.enabled";

export type PinCorner = "topLeft" | "topRight" | "bottomRight" | "bottomLeft";
export type Point = [number, number];
/** Top left, top right, bottom right, bottom left, in box units (1 = the whole box). */
export type CornerPin = [Point, Point, Point, Point];

/** The corners in drawing order, with their default places in percent. */
export const PIN_CORNERS: ReadonlyArray<{ corner: PinCorner; label: string; default: Point }> = [
	{ corner: "topLeft", label: "Top left", default: [0, 0] },
	{ corner: "topRight", label: "Top right", default: [100, 0] },
	{ corner: "bottomRight", label: "Bottom right", default: [100, 100] },
	{ corner: "bottomLeft", label: "Bottom left", default: [0, 100] },
];

export function cornerPinKey(corner: PinCorner, axis: "X" | "Y"): string {
	return `cornerPin.${corner}${axis}`;
}

/** Every corner param key, X then Y for each corner in order. */
export const CORNER_PIN_KEYS: readonly string[] = PIN_CORNERS.flatMap(({ corner }) => [
	cornerPinKey(corner, "X"),
	cornerPinKey(corner, "Y"),
]);

function readPercent(value: unknown, fallback: number): number {
	const number = typeof value === "number" && Number.isFinite(value) ? value : fallback;
	return Math.min(CORNER_PIN_MAX, Math.max(CORNER_PIN_MIN, number)) / 100;
}

/**
 * The corners in box units, whether or not the pin is on. `valueOf` reads
 * a key (the clip's params, or their keyframed values at a time).
 */
export function cornerPinCorners(valueOf: (key: string) => unknown): CornerPin {
	return PIN_CORNERS.map(({ corner, default: fallback }) => [
		readPercent(valueOf(cornerPinKey(corner, "X")), fallback[0]),
		readPercent(valueOf(cornerPinKey(corner, "Y")), fallback[1]),
	]) as CornerPin;
}

/** True when the corners make a convex shape in either winding. */
export function isConvexPin(corners: CornerPin): boolean {
	let sign = 0;
	for (let i = 0; i < 4; i++) {
		const [ax, ay] = corners[i];
		const [bx, by] = corners[(i + 1) % 4];
		const [cx, cy] = corners[(i + 2) % 4];
		const cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
		if (!Number.isFinite(cross) || Math.abs(cross) < 1e-9) return false;
		const s = Math.sign(cross);
		if (sign === 0) sign = s;
		else if (s !== sign) return false;
	}
	return true;
}

function isDefaultPin(corners: CornerPin): boolean {
	return corners.every(
		(point, i) =>
			Math.abs(point[0] - PIN_CORNERS[i].default[0] / 100) < 1e-9 &&
			Math.abs(point[1] - PIN_CORNERS[i].default[1] / 100) < 1e-9,
	);
}

/**
 * The pin to draw, or null when it is off, at the default corners, or a
 * shape that cannot be drawn in perspective (crossed, dented or flat).
 */
export function readCornerPin(params: ParamValues | undefined): CornerPin | null {
	if (!params || params[CORNER_PIN_ENABLED] !== true) return null;
	const corners = cornerPinCorners((key) => params[key]);
	return isDefaultPin(corners) || !isConvexPin(corners) ? null : corners;
}

/** The pin at an element-relative time, with the corners' keyframes. */
export function cornerPinAt({
	params,
	animations,
	localTime,
}: {
	params: ParamValues | undefined;
	animations: ElementAnimations | undefined;
	localTime: number;
}): CornerPin | null {
	if (!params || params[CORNER_PIN_ENABLED] !== true) return null;
	const corners = cornerPinCorners((key) => {
		const base = params[key];
		return resolveAnimationPathValueAtTime({
			animations,
			propertyPath: key,
			localTime,
			fallbackValue: typeof base === "number" ? base : Number.NaN,
		});
	});
	return isDefaultPin(corners) || !isConvexPin(corners) ? null : corners;
}

/** A clip's box on the canvas; a negative size (a flip) only mirrors the picture inside it. */
export interface PinBox {
	cx: number;
	cy: number;
	width: number;
	height: number;
	/** Degrees. */
	rotation: number;
}

/** A box point (box units) on the canvas. */
export function boxToCanvas(box: PinBox, [x, y]: Point): Point {
	const angle = (box.rotation * Math.PI) / 180;
	const cos = Math.cos(angle);
	const sin = Math.sin(angle);
	const lx = (x - 0.5) * Math.abs(box.width);
	const ly = (y - 0.5) * Math.abs(box.height);
	return [box.cx + lx * cos - ly * sin, box.cy + lx * sin + ly * cos];
}

/** A canvas point in box units (the inverse of `boxToCanvas`). */
export function canvasToBox(box: PinBox, [x, y]: Point): Point {
	const angle = (box.rotation * Math.PI) / 180;
	const cos = Math.cos(angle);
	const sin = Math.sin(angle);
	const dx = x - box.cx;
	const dy = y - box.cy;
	const lx = dx * cos + dy * sin;
	const ly = -dx * sin + dy * cos;
	return [lx / Math.max(Math.abs(box.width), 1e-9) + 0.5, ly / Math.max(Math.abs(box.height), 1e-9) + 0.5];
}

/** The params that pin the corners (box units), rounded to 0.01 %. */
export function cornerPinParams(corners: CornerPin): ParamValues {
	const values: ParamValues = {};
	PIN_CORNERS.forEach(({ corner }, i) => {
		for (const [axis, value] of [["X", corners[i][0]], ["Y", corners[i][1]]] as const) {
			const percent = Math.min(CORNER_PIN_MAX, Math.max(CORNER_PIN_MIN, value * 100));
			values[cornerPinKey(corner, axis)] = Math.round(percent * 100) / 100;
		}
	});
	return values;
}

/** The params of a pin at the clip's own corners (switched on or off). */
export function resetCornerPinParams(enabled: boolean): ParamValues {
	return {
		[CORNER_PIN_ENABLED]: enabled,
		...cornerPinParams(PIN_CORNERS.map(({ default: d }) => [d[0] / 100, d[1] / 100]) as CornerPin),
	};
}
