import type { MediaTime } from "@/wasm";
import {
	VISUAL_ELEMENT_TYPES,
	type TimelineElement,
	type TimelineTrack,
	type VisualElement,
} from "@/timeline/types";

/**
 * Where a clip's transition plays. Mirrors `crates/editor/src/transition.rs`
 * so both editors agree frame for frame.
 *
 * A transition belongs to the clip it leads out of and is centred on the cut
 * to the next clip on the same track: half of it plays over the end of the
 * outgoing clip, half over the start of the incoming one, so the timeline's
 * durations never change. Each side keeps playing through its half from the
 * source material beyond its trim ("handles"); a source with no handle holds
 * its edge frame. A clip with nothing adjacent after it transitions out to
 * nothing over its own last moments.
 */

/** How far apart two clips may be and still count as one cut: 10 ms. */
export const ADJACENCY_TOLERANCE_TICKS = 1200;

export interface TransitionWindow {
	start: MediaTime;
	/** Exclusive. */
	end: MediaTime;
	/** The incoming clip, or null for a transition out to nothing. */
	toElementId: string | null;
}

export function isVisualElement(
	element: TimelineElement,
): element is VisualElement {
	return (VISUAL_ELEMENT_TYPES as readonly string[]).includes(element.type);
}

export function getElementEnd({ element }: { element: TimelineElement }) {
	return element.startTime + element.duration;
}

export function isAdjacent({
	element,
	next,
}: {
	element: TimelineElement;
	next: TimelineElement;
}): boolean {
	return (
		Math.abs(next.startTime - getElementEnd({ element })) <=
		ADJACENCY_TOLERANCE_TICKS
	);
}

/** The first visible visual element on the track that follows `element` directly. */
export function getNextAdjacent({
	track,
	element,
}: {
	track: TimelineTrack;
	element: TimelineElement;
}): VisualElement | null {
	let best: VisualElement | null = null;
	for (const candidate of track.elements) {
		if (candidate.id === element.id || !isVisualElement(candidate)) continue;
		if (candidate.hidden) continue;
		if (!isAdjacent({ element, next: candidate })) continue;
		if (!best || candidate.startTime < best.startTime) best = candidate;
	}
	return best;
}

/**
 * The window of `element`'s transition, given the clip after it; a `next`
 * that is not adjacent counts as none. Null when the element has no
 * transition or it would be empty.
 */
export function getTransitionWindow({
	element,
	next,
}: {
	element: TimelineElement;
	next: TimelineElement | null;
}): TransitionWindow | null {
	if (!isVisualElement(element) || !element.transition) return null;
	const transition = element.transition;
	if (transition.duration <= 0 || element.duration <= 0) return null;
	const cut = getElementEnd({ element });
	let window: TransitionWindow;
	if (next && next.duration > 0 && isAdjacent({ element, next })) {
		// Never longer than either clip, so the window stays inside both.
		const length = Math.min(
			transition.duration,
			element.duration,
			next.duration,
		);
		const half = Math.trunc(length / 2);
		window = {
			start: (cut - half) as MediaTime,
			end: (cut - half + length) as MediaTime,
			toElementId: next.id,
		};
	} else {
		const length = Math.min(transition.duration, element.duration);
		window = {
			start: (cut - length) as MediaTime,
			end: cut as MediaTime,
			toElementId: null,
		};
	}
	return window.end > window.start ? window : null;
}

/** The window of the transition `element` leads out of, if it has one. */
export function getOutgoingTransitionWindow({
	track,
	element,
}: {
	track: TimelineTrack;
	element: TimelineElement;
}): TransitionWindow | null {
	return getTransitionWindow({
		element,
		next: getNextAdjacent({ track, element }),
	});
}

/** The window of the transition that leads into `element` from the clip before it. */
export function getIncomingTransitionWindow({
	track,
	element,
}: {
	track: TimelineTrack;
	element: TimelineElement;
}): TransitionWindow | null {
	for (const candidate of track.elements) {
		if (candidate.id === element.id || !isVisualElement(candidate)) continue;
		if (candidate.hidden || !candidate.transition) continue;
		if (!isAdjacent({ element: candidate, next: element })) continue;
		const window = getTransitionWindow({
			element: candidate,
			next: getNextAdjacent({ track, element: candidate }),
		});
		if (window?.toElementId === element.id) return window;
	}
	return null;
}

export function isInTransitionWindow({
	transitionWindow,
	time,
}: {
	transitionWindow: TransitionWindow;
	time: number;
}): boolean {
	return time >= transitionWindow.start && time < transitionWindow.end;
}

/** 0 at the start of the window, 1 at its end. */
export function getTransitionProgress({
	transitionWindow,
	time,
}: {
	transitionWindow: TransitionWindow;
	time: number;
}): number {
	const length = transitionWindow.end - transitionWindow.start;
	if (length <= 0) return 1;
	return Math.min(Math.max((time - transitionWindow.start) / length, 0), 1);
}
