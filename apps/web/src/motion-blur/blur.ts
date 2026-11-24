import type { FrameRate } from "opencut-wasm";
import type { ParamValues } from "@/params";
import type { Transform } from "@/rendering";
import type { QuadTransformDescriptor } from "@/services/renderer/compositor/types";

/**
 * Motion blur on a clip: the `motionBlur` param, 0 (off) to 100, is how much
 * of a frame the shutter stays open (100 the whole frame, 50 the film look).
 * The renderer resolves the clip's transform half a shutter before and after
 * the frame's time and the shared compositor (WASM) draws the clip at evenly
 * spaced points between the two and averages them, as on the desktop
 * (`editor::motion_blur`, `compositor::motion_blur_samples`). Checked
 * against `fixtures/motion-blur/cases.json`.
 */

export const MOTION_BLUR_PARAM = "motionBlur";
export const MOTION_BLUR_MAX = 100;
/** Amounts below this are off. */
const MOTION_BLUR_OFF = 0.5;

function clampAmount(amount: unknown): number {
	return typeof amount === "number" && Number.isFinite(amount) ? Math.min(MOTION_BLUR_MAX, Math.max(0, amount)) : 0;
}

/** The clip's motion blur amount, 0 to 100. */
export function readMotionBlur(params: ParamValues | undefined): number {
	return clampAmount(params?.[MOTION_BLUR_PARAM]);
}

/** Half the time the shutter is open, in seconds; null when the blur is off. */
export function motionBlurHalfShutter(amount: number, fps: FrameRate): number | null {
	const value = clampAmount(amount);
	if (value < MOTION_BLUR_OFF || !fps.numerator || !fps.denominator) return null;
	return ((value / MOTION_BLUR_MAX) * fps.denominator) / fps.numerator / 2;
}

/** A clip's transform at shutter open and close. */
export interface MotionTransforms {
	from: Transform;
	to: Transform;
}

export function sameTransform(a: Transform, b: Transform): boolean {
	return (
		a.position.x === b.position.x &&
		a.position.y === b.position.y &&
		a.scaleX === b.scaleX &&
		a.scaleY === b.scaleY &&
		a.rotate === b.rotate
	);
}

/**
 * Motion transforms from a transform at a local time (ticks), or null when
 * the blur is off or the clip holds still over the shutter.
 */
export function motionTransforms({
	amount,
	fps,
	ticksPerSecond,
	localTime,
	transformAt,
}: {
	amount: number;
	fps: FrameRate;
	ticksPerSecond: number;
	localTime: number;
	transformAt: (localTime: number) => Transform;
}): MotionTransforms | null {
	const half = motionBlurHalfShutter(amount, fps);
	if (half === null) return null;
	const ticks = Math.round(half * ticksPerSecond);
	if (ticks <= 0) return null;
	const from = transformAt(localTime - ticks);
	const to = transformAt(localTime + ticks);
	return sameTransform(from, to) ? null : { from, to };
}

/**
 * For text, drawn into a canvas-sized texture at its transform: the
 * canvas-sized quad moved by the change from the drawn transform to
 * another (about the text's anchor, the canvas centre plus its position).
 */
export function canvasQuadFor({
	drawn,
	other,
	width,
	height,
}: {
	drawn: Transform;
	other: Transform;
	width: number;
	height: number;
}): QuadTransformDescriptor | null {
	if (drawn.scaleX === 0 || drawn.scaleY === 0) return null;
	const anchor = (t: Transform) => ({ x: width / 2 + t.position.x, y: height / 2 + t.position.y });
	const a = anchor(drawn);
	const b = anchor(other);
	// p' = B + R(other) S(other) S(drawn)^-1 R(-drawn) (p - A), for the centre.
	const rad = (deg: number) => (deg * Math.PI) / 180;
	const dx = width / 2 - a.x;
	const dy = height / 2 - a.y;
	const [c1, s1] = [Math.cos(-rad(drawn.rotate)), Math.sin(-rad(drawn.rotate))];
	let x = (dx * c1 - dy * s1) * (other.scaleX / drawn.scaleX);
	let y = (dx * s1 + dy * c1) * (other.scaleY / drawn.scaleY);
	const [c2, s2] = [Math.cos(rad(other.rotate)), Math.sin(rad(other.rotate))];
	[x, y] = [x * c2 - y * s2, x * s2 + y * c2];
	const kx = Math.abs(other.scaleX / drawn.scaleX);
	const ky = Math.abs(other.scaleY / drawn.scaleY);
	return {
		centerX: b.x + x,
		centerY: b.y + y,
		width: width * kx,
		height: height * ky,
		rotationDegrees: other.rotate - drawn.rotate,
		flipX: false,
		flipY: false,
	};
}
