/**
 * Edit points and ripple plans: the times a cut sits at (every element's
 * start and end), and what a ripple delete moves. Pure functions over the
 * scene's tracks, shared by the keyboard actions and the desktop's
 * `crates/editor/src/edits.rs`.
 */

import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";

export function allTracks({ tracks }: { tracks: SceneTracks }): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** Every element start and end in ticks, plus 0, sorted without repeats. */
export function collectEditPoints({ tracks }: { tracks: TimelineTrack[] }): number[] {
	const points = new Set<number>([0]);
	for (const track of tracks) {
		for (const element of track.elements as TimelineElement[]) {
			points.add(element.startTime);
			points.add(element.startTime + element.duration);
		}
	}
	return [...points].sort((a, b) => a - b);
}

/** The first edit point after `time`, or `null` at the end. */
export function nextEditPoint({ points, time }: { points: number[]; time: number }): number | null {
	for (const point of points) {
		if (point > time) return point;
	}
	return null;
}

/** The last edit point before `time`, or `null` at the start. */
export function previousEditPoint({ points, time }: { points: number[]; time: number }): number | null {
	let found: number | null = null;
	for (const point of points) {
		if (point < time) found = point;
		else break;
	}
	return found;
}

export interface ElementRef {
	trackId: string;
	elementId: string;
}

export interface RipplePlan {
	/** The elements to remove. */
	remove: ElementRef[];
	/** New start times for the elements that close the gaps. */
	moves: { trackId: string; elementId: string; startTime: number }[];
	/** How many ticks the timeline shortens by. */
	removedTicks: number;
}

/** `[start, end)` spans merged where they touch or overlap. */
export function mergeSpans({ spans }: { spans: [number, number][] }): [number, number][] {
	const sorted = spans
		.filter(([start, end]) => end > start)
		.map(([start, end]) => [start, end] as [number, number])
		.sort((a, b) => a[0] - b[0]);
	const merged: [number, number][] = [];
	for (const span of sorted) {
		const last = merged[merged.length - 1];
		if (last && span[0] <= last[1]) {
			last[1] = Math.max(last[1], span[1]);
		} else {
			merged.push(span);
		}
	}
	return merged;
}

/**
 * The ripple delete of `elements`: they go, and every other element that
 * starts at or after a removed span's end moves left by the length of the
 * removed spans before it (spans from several tracks merged, so overlapping
 * deletions count once), on every track.
 */
export function rippleDeletePlan({
	tracks,
	elements,
}: {
	tracks: TimelineTrack[];
	elements: ElementRef[];
}): RipplePlan {
	const removedIds = new Set(elements.map((ref) => ref.elementId));
	const spans: [number, number][] = [];
	for (const track of tracks) {
		for (const element of track.elements as TimelineElement[]) {
			if (removedIds.has(element.id)) {
				spans.push([element.startTime, element.startTime + element.duration]);
			}
		}
	}
	const merged = mergeSpans({ spans });
	const moves: RipplePlan["moves"] = [];
	for (const track of tracks) {
		// Locked tracks stay where they are.
		if (track.locked) continue;
		for (const element of track.elements as TimelineElement[]) {
			if (removedIds.has(element.id)) continue;
			let shift = 0;
			for (const [start, end] of merged) {
				if (end <= element.startTime) shift += end - start;
			}
			if (shift > 0) {
				moves.push({ trackId: track.id, elementId: element.id, startTime: element.startTime - shift });
			}
		}
	}
	return {
		remove: elements,
		moves,
		removedTicks: merged.reduce((sum, [start, end]) => sum + (end - start), 0),
	};
}

/**
 * What removing the stretch `[from, to)` of the timeline takes, once every
 * element crossing `from` or `to` has been split there: the elements inside
 * go, and everything from `to` on moves left by the stretch's length.
 */
export function deleteRangePlan({
	tracks,
	from,
	to,
}: {
	tracks: TimelineTrack[];
	from: number;
	to: number;
}): RipplePlan {
	const remove: ElementRef[] = [];
	const moves: RipplePlan["moves"] = [];
	const length = Math.max(0, to - from);
	for (const track of tracks) {
		if (track.locked) continue;
		for (const element of track.elements as TimelineElement[]) {
			const end = element.startTime + element.duration;
			if (element.startTime >= from && end <= to) {
				remove.push({ trackId: track.id, elementId: element.id });
			} else if (element.startTime >= to && length > 0) {
				moves.push({ trackId: track.id, elementId: element.id, startTime: element.startTime - length });
			}
		}
	}
	return { remove, moves, removedTicks: length };
}

/** The elements whose span strictly crosses `time` (a split there makes two). */
export function elementsCrossing({
	tracks,
	time,
}: {
	tracks: TimelineTrack[];
	time: number;
}): ElementRef[] {
	const refs: ElementRef[] = [];
	for (const track of tracks) {
		if (track.locked) continue;
		for (const element of track.elements as TimelineElement[]) {
			if (element.startTime < time && element.startTime + element.duration > time) {
				refs.push({ trackId: track.id, elementId: element.id });
			}
		}
	}
	return refs;
}

/** Clips on unlocked tracks that overlap `[from, to)` (in either order). */
export function elementsInRange({
	tracks,
	from,
	to,
}: {
	tracks: TimelineTrack[];
	from: number;
	to: number;
}): ElementRef[] {
	const [start, end] = from <= to ? [from, to] : [to, from];
	const refs: ElementRef[] = [];
	for (const track of tracks) {
		if (track.locked) continue;
		for (const element of track.elements as TimelineElement[]) {
			if (element.startTime < end && element.startTime + element.duration > start) {
				refs.push({ trackId: track.id, elementId: element.id });
			}
		}
	}
	return refs;
}
