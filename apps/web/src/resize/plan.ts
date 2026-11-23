/**
 * Resize project: a new canvas size with every clip, text and caption
 * re-fitted. The old picture is scaled about the centre by the content
 * factor (fit: the whole old frame shows; fill: the new frame is covered,
 * edges cropped; keep: the same pixel sizes), so positions, keyframes and
 * pixel settings follow it; each element's own scale allows for how its
 * base size follows the canvas (pictures fit the canvas, text follows its
 * height). Captions are laid out again at the bottom of the new canvas.
 * `crates/editor/src/resize.rs` does the same; both are checked against
 * `fixtures/resize-project/cases.json`.
 */

import type { ChannelData, ElementAnimations, ScalarAnimationKey } from "@/animation/types";
import type { Mask } from "@/masks/types";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";

export type ResizeMode = "fit" | "fill" | "keep";
export const RESIZE_MODES: readonly ResizeMode[] = ["fit", "fill", "keep"];

export interface Size {
	width: number;
	height: number;
}

/** How much the content grows. */
export function contentFactor({ mode, from, to }: { mode: ResizeMode; from: Size; to: Size }): number {
	const rw = to.width / from.width;
	const rh = to.height / from.height;
	return mode === "fit" ? Math.min(rw, rh) : mode === "fill" ? Math.max(rw, rh) : 1;
}

function containScale(canvas: Size, source: Size): number {
	return Math.min(canvas.width / source.width, canvas.height / source.height);
}

/** The scale factor of a video, picture, sticker or shape of `source` size. */
export function pictureFactor({ mode, from, to, source }: { mode: ResizeMode; from: Size; to: Size; source: Size }): number {
	return (contentFactor({ mode, from, to }) * containScale(from, source)) / containScale(to, source);
}

/** The scale factor of a text element (its font size follows the canvas height). */
export function textFactor({ mode, from, to }: { mode: ResizeMode; from: Size; to: Size }): number {
	return (contentFactor({ mode, from, to }) * from.height) / to.height;
}

/** Captions' bottom margin as a share of the height: clear of the platforms' buttons on a tall canvas. */
export function captionMargin(to: Size): number {
	return to.height > to.width ? 0.25 : 0.05;
}

/** Params in canvas pixels, scaled with the content. */
export const PIXEL_PARAMS = ["outline.width", "shadow.offsetX", "shadow.offsetY", "shadow.blur"] as const;
const POSITION_PATHS = ["transform.positionX", "transform.positionY"] as const;
const SCALE_PATHS = ["transform.scaleX", "transform.scaleY"] as const;

/** Square stand-in for stickers and shapes without a known size. */
const SQUARE: Size = { width: 1, height: 1 };

function scaleChannel(channel: ChannelData | undefined, factor: number): ChannelData | undefined {
	if (!channel || factor === 1) return channel;
	if ("keys" in channel && Array.isArray(channel.keys)) {
		const keys = (channel.keys as ScalarAnimationKey[]).map((key) =>
			typeof key.value !== "number"
				? key
				: {
						...key,
						value: key.value * factor,
						...(key.leftHandle ? { leftHandle: { ...key.leftHandle, dv: key.leftHandle.dv * factor } } : {}),
						...(key.rightHandle ? { rightHandle: { ...key.rightHandle, dv: key.rightHandle.dv * factor } } : {}),
					},
		);
		return { ...channel, keys } as ChannelData;
	}
	return channel;
}

function scaleParams(params: Record<string, unknown>, factors: Record<string, number>): Record<string, unknown> {
	const next = { ...params };
	for (const [key, factor] of Object.entries(factors)) {
		if (typeof next[key] === "number") next[key] = (next[key] as number) * factor;
	}
	return next;
}

export interface ResizeInput {
	tracks: SceneTracks;
	from: Size;
	to: Size;
	mode: ResizeMode;
	/** A media item's size, or null when unknown. */
	sizeOf: (mediaId: string) => Size | null;
}

/**
 * The tracks re-fitted for the new canvas (captions excepted: they are laid
 * out again by the caller). Elements that do not change are returned as is.
 */
export function resizeTracks({ tracks, from, to, mode, sizeOf }: ResizeInput): SceneTracks {
	const k = contentFactor({ mode, from, to });
	const resizeElement = (element: TimelineElement): TimelineElement => {
		if (element.type === "compound") {
			const compound = element as TimelineElement & { tracks?: TimelineTrack[] };
			return compound.tracks ? ({ ...compound, tracks: compound.tracks.map(resizeTrack) } as TimelineElement) : element;
		}
		let own: number;
		if (element.type === "video" || element.type === "image") {
			const size = sizeOf(element.mediaId);
			if (!size?.width || !size.height) return element;
			own = pictureFactor({ mode, from, to, source: size });
		} else if (element.type === "sticker") {
			const sticker = element as TimelineElement & { intrinsicWidth?: number; intrinsicHeight?: number };
			const source = sticker.intrinsicWidth && sticker.intrinsicHeight ? { width: sticker.intrinsicWidth, height: sticker.intrinsicHeight } : SQUARE;
			own = pictureFactor({ mode, from, to, source });
		} else if (element.type === "graphic") {
			own = pictureFactor({ mode, from, to, source: SQUARE });
		} else if (element.type === "text") {
			if (element.words?.length) return element;
			own = textFactor({ mode, from, to });
		} else {
			return element;
		}
		const factors: Record<string, number> = {};
		for (const path of POSITION_PATHS) factors[path] = k;
		for (const path of SCALE_PATHS) factors[path] = own;
		for (const key of PIXEL_PARAMS) factors[key] = k;
		const params = scaleParams(element.params as Record<string, unknown>, factors);
		let animations: ElementAnimations | undefined = element.animations;
		if (animations) {
			animations = { ...animations };
			for (const [path, factor] of Object.entries(factors)) {
				const key = path as keyof ElementAnimations;
				if (animations[key]) animations[key] = scaleChannel(animations[key], factor);
			}
		}
		const masks = (element as { masks?: Mask[] }).masks;
		const scaledMasks =
			masks && k !== 1
				? masks.map(
						(mask) =>
							({
								...mask,
								params: { ...mask.params, feather: (mask.params.feather ?? 0) * k, strokeWidth: (mask.params.strokeWidth ?? 0) * k },
							}) as Mask,
					)
				: masks;
		return {
			...element,
			params,
			...(animations ? { animations } : {}),
			...(scaledMasks ? { masks: scaledMasks } : {}),
		} as TimelineElement;
	};
	const resizeTrack = <T extends TimelineTrack>(track: T): T =>
		({ ...track, elements: (track.elements as TimelineElement[]).map(resizeElement) }) as T;
	return { overlay: tracks.overlay.map(resizeTrack), main: resizeTrack(tracks.main), audio: tracks.audio.map(resizeTrack) };
}
