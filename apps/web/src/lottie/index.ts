/**
 * Lottie animations (`.json` or `.lottie` files) as media. They are read,
 * checked and drawn by the shared Rust renderer in `crates/lottie` through
 * `opencut-wasm`, the same code the desktop app runs, so both editors show
 * the same frames. On the timeline a Lottie file is an image element: it
 * loops for as long as the clip lasts and takes the usual transform,
 * effects and masks.
 */

import { LottieAnimation, isLottie } from "opencut-wasm";

/** File names that may hold a Lottie animation. */
export const LOTTIE_EXTENSIONS = [".json", ".lottie"] as const;
/** The `accept` list for file pickers that take Lottie files. */
export const LOTTIE_ACCEPT = ".json,.lottie,application/json";
/** Largest side a frame is drawn at. */
export const MAX_LOTTIE_RASTER = 2048;
/** Smallest side a frame is drawn at, so small files stay sharp when enlarged. */
const MIN_LOTTIE_RASTER = 256;

export interface LottieInfo {
	width: number;
	height: number;
	frameRate: number;
	durationSeconds: number;
}

/** Whether the name ends in `.json` or `.lottie`. */
export function hasLottieName({ name }: { name: string }): boolean {
	const lower = name.toLowerCase();
	return LOTTIE_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

/** Whether the file is a Lottie animation (by name first, then contents). */
export async function isLottieFile({ file }: { file: File }): Promise<boolean> {
	if (!hasLottieName({ name: file.name })) return false;
	try {
		return isLottie(new Uint8Array(await file.arrayBuffer()));
	} catch {
		return false;
	}
}

const animations = new WeakMap<Blob, Promise<LottieAnimation>>();

/** The parsed animation for a file, read once per file. Rejects with the reason it cannot be used. */
export function loadLottie({ file }: { file: Blob }): Promise<LottieAnimation> {
	let animation = animations.get(file);
	if (!animation) {
		animation = file.arrayBuffer().then((buffer) => new LottieAnimation(new Uint8Array(buffer)));
		animations.set(file, animation);
		animation.catch(() => animations.delete(file));
	}
	return animation;
}

export function lottieInfo({ animation }: { animation: LottieAnimation }): LottieInfo {
	return {
		width: animation.width,
		height: animation.height,
		frameRate: animation.frameRate,
		durationSeconds: animation.durationSeconds,
	};
}

/**
 * The pixel size frames are drawn at: the animation fitted inside
 * `bounds` (the canvas it is shown on), kept between 256 and `maxSide`
 * on its longer side, with the animation's shape.
 */
export function lottieRasterSize({
	width,
	height,
	bounds,
	maxSide = MAX_LOTTIE_RASTER,
}: {
	width: number;
	height: number;
	bounds: { width: number; height: number };
	maxSide?: number;
}): { width: number; height: number } {
	const fit = Math.min(bounds.width / width, bounds.height / height);
	const longest = Math.max(width, height) * fit;
	const target = Math.min(Math.max(longest, MIN_LOTTIE_RASTER), maxSide);
	const scale = target / Math.max(width, height);
	return {
		width: Math.max(1, Math.round(width * scale)),
		height: Math.max(1, Math.round(height * scale)),
	};
}

/** One frame as a canvas of the given size. */
export function drawLottieFrame({
	animation,
	frame,
	width,
	height,
}: {
	animation: LottieAnimation;
	frame: number;
	width: number;
	height: number;
}): OffscreenCanvas {
	const rgba = animation.renderRgba(frame, width, height);
	const canvas = new OffscreenCanvas(width, height);
	const context = canvas.getContext("2d");
	if (!context) throw new Error("No 2D canvas for the Lottie frame");
	context.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), width, height), 0, 0);
	return canvas;
}

/**
 * Recently drawn frames, so a paused preview or a repeated frame does not
 * draw again. Each call that misses makes a new canvas object, which is
 * what tells the compositor to upload a new texture.
 */
const recentFrames = new Map<string, OffscreenCanvas>();
const RECENT_FRAMES = 12;

export function lottieFrameAt({
	key,
	animation,
	seconds,
	width,
	height,
}: {
	/** Identifies the animation (its media id). */
	key: string;
	animation: LottieAnimation;
	/** Seconds into the clip; the animation loops. */
	seconds: number;
	width: number;
	height: number;
}): OffscreenCanvas {
	const frame = animation.frameAt(seconds);
	const cacheKey = `${key}:${frame.toFixed(3)}:${width}x${height}`;
	const cached = recentFrames.get(cacheKey);
	if (cached) {
		recentFrames.delete(cacheKey);
		recentFrames.set(cacheKey, cached);
		return cached;
	}
	const canvas = drawLottieFrame({ animation, frame, width, height });
	recentFrames.set(cacheKey, canvas);
	while (recentFrames.size > RECENT_FRAMES) {
		const oldest = recentFrames.keys().next().value;
		if (oldest === undefined) break;
		recentFrames.delete(oldest);
	}
	return canvas;
}

/** The frame halfway through, which shows more than the first one usually does. */
export function lottieThumbnailFrame({ animation }: { animation: LottieAnimation }): number {
	return animation.frameAt(animation.durationSeconds / 2);
}
