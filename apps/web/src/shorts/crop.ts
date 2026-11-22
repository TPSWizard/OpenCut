/**
 * Long video to shorts, step two: one short's timeline. Everything that
 * plays inside the window is kept, cut at its edges with the same
 * arithmetic as splitting a clip (trims, speed ramps, keyframes), and
 * moved so the window starts at 0. Captions keep the words whose middle
 * falls inside. Video and pictures are then scaled to cover the vertical
 * canvas, centred (Auto-reframe in the short's project can follow faces).
 */

import { splitAnimationsAtTime } from "@/animation";
import { getSourceSpanAtClipTime, splitRetimeAtClipTime } from "@/retime";
import { reframeGeometry } from "@/reframe/path";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { isRetimableElement } from "@/timeline/element-utils";
import { type MediaTime, roundMediaTime } from "@/wasm";

/** The part of `element` from `time` on (`side` "right") or before it ("left"); `time` is inside it. */
function cutAt({ element, time, side }: { element: TimelineElement; time: number; side: "left" | "right" }): TimelineElement {
	const relative = time - element.startTime;
	const retime = isRetimableElement(element) ? element.retime : undefined;
	const leftSource = roundMediaTime({ time: getSourceSpanAtClipTime({ clipTime: relative, retime }) });
	const totalSource = roundMediaTime({ time: getSourceSpanAtClipTime({ clipTime: element.duration, retime }) });
	const { leftAnimations, rightAnimations } = splitAnimationsAtTime({
		animations: element.animations,
		splitTime: relative as MediaTime,
		shouldIncludeSplitBoundary: true,
	});
	if (side === "left") {
		return {
			...element,
			duration: relative as MediaTime,
			trimEnd: (element.trimEnd + (totalSource - leftSource)) as MediaTime,
			animations: leftAnimations,
		} as TimelineElement;
	}
	const rightRetime = splitRetimeAtClipTime({ retime, splitClipTime: relative }).right;
	return {
		...element,
		startTime: time as MediaTime,
		duration: (element.duration - relative) as MediaTime,
		trimStart: (element.trimStart + leftSource) as MediaTime,
		animations: rightAnimations,
		...(rightRetime !== undefined ? { retime: rightRetime } : {}),
	} as TimelineElement;
}

/** One element cut to `[start, end)` (ticks) and moved by `-start`, or null when nothing of it is left. */
export function cropElement({ element, start, end }: { element: TimelineElement; start: number; end: number }): TimelineElement | null {
	const elementEnd = element.startTime + element.duration;
	if (elementEnd <= start || element.startTime >= end) return null;
	let cropped = element;
	if (cropped.startTime < start) cropped = cutAt({ element: cropped, time: start, side: "right" });
	if (cropped.startTime + cropped.duration > end) cropped = cutAt({ element: cropped, time: end, side: "left" });
	if (cropped.type === "text" && element.type === "text" && element.words?.length) {
		const newStart = cropped.startTime;
		const words = element.words
			.map((word) => ({ text: word.text, start: element.startTime + word.start, end: element.startTime + word.end }))
			.filter((word) => {
				const middle = (word.start + word.end) / 2;
				return middle >= start && middle < end;
			})
			.map((word) => ({
				text: word.text,
				start: Math.min(Math.max(word.start - newStart, 0), cropped.duration) as MediaTime,
				end: Math.min(Math.max(word.end - newStart, 0), cropped.duration) as MediaTime,
			}));
		if (words.length === 0) return null;
		const content = words.length === element.words.length ? element.params.content : words.map((word) => word.text.trim()).join(" ");
		cropped = { ...cropped, words, params: { ...cropped.params, content } };
	}
	return { ...cropped, startTime: (cropped.startTime - start) as MediaTime } as TimelineElement;
}

/** The scene's tracks cut to `[start, end)` (ticks) and moved to start at 0; empty tracks are kept. */
export function cropTracks({ tracks, start, end }: { tracks: SceneTracks; start: number; end: number }): SceneTracks {
	const cropTrack = <T extends TimelineTrack>(track: T): T =>
		({
			...track,
			elements: (track.elements as TimelineElement[])
				.map((element) => cropElement({ element, start, end }))
				.filter((element): element is TimelineElement => element !== null),
		}) as T;
	return {
		overlay: tracks.overlay.map(cropTrack),
		main: cropTrack(tracks.main),
		audio: tracks.audio.map(cropTrack),
	};
}

/**
 * Video and picture clips scaled to cover the canvas and centred, with
 * any position or scale keyframes dropped. `sizeOf` gives a media item's
 * pixel size.
 */
export function coverCanvas({
	tracks,
	canvas,
	sizeOf,
}: {
	tracks: SceneTracks;
	canvas: { width: number; height: number };
	sizeOf: (mediaId: string) => { width: number; height: number } | null;
}): SceneTracks {
	const cover = (element: TimelineElement): TimelineElement => {
		if (element.type !== "video" && element.type !== "image") return element;
		const size = sizeOf(element.mediaId);
		if (!size || size.width <= 0 || size.height <= 0) return element;
		const { scale } = reframeGeometry({
			sourceWidth: size.width,
			sourceHeight: size.height,
			canvasWidth: canvas.width,
			canvasHeight: canvas.height,
		});
		const animations = element.animations ? { ...element.animations } : undefined;
		if (animations) {
			for (const path of ["transform.scaleX", "transform.scaleY", "transform.positionX", "transform.positionY"]) {
				delete (animations as Record<string, unknown>)[path];
			}
		}
		return {
			...element,
			...(animations ? { animations } : {}),
			params: {
				...element.params,
				"transform.scaleX": scale,
				"transform.scaleY": scale,
				"transform.positionX": 0,
				"transform.positionY": 0,
			},
		} as TimelineElement;
	};
	const coverTrack = <T extends TimelineTrack>(track: T): T =>
		({ ...track, elements: (track.elements as TimelineElement[]).map(cover) }) as T;
	return {
		overlay: tracks.overlay.map(coverTrack),
		main: coverTrack(tracks.main),
		audio: tracks.audio.map(coverTrack),
	};
}
