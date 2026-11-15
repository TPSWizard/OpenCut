import type { ElementRef, SceneTracks } from "@/timeline";
import {
	buildMoveGroup,
	resolveGroupMove,
	type GroupMoveResult,
} from "@/timeline/group-move";
import { getDisplayTracks } from "@/timeline/group-move/track-placement";
import type { MediaTime } from "@/wasm";

/**
 * Where a clip dragged on the phone timeline lands: the same move planner
 * the desktop timeline uses (grouped clips come along, nothing overlaps,
 * clips only go on tracks that take them), onto an existing track.
 */
export function planPhoneMove({
	tracks,
	ref,
	selected,
	startTime,
	targetTrackId,
}: {
	tracks: SceneTracks;
	ref: ElementRef;
	selected: ElementRef[];
	startTime: MediaTime;
	targetTrackId: string;
}): GroupMoveResult | null {
	const group = buildMoveGroup({
		anchorRef: ref,
		selectedElements: selected,
		tracks,
	});
	if (!group) return null;
	return resolveGroupMove({
		group,
		tracks,
		anchorStartTime: startTime,
		target: { kind: "existingTrack", anchorTargetTrackId: targetTrackId },
	});
}

/** Clip starts and ends on every track, except the clips in `exclude`. */
export function clipEdges({
	tracks,
	exclude,
}: {
	tracks: SceneTracks;
	exclude: Set<string>;
}): number[] {
	const edges = new Set<number>([0]);
	for (const track of getDisplayTracks({ tracks })) {
		for (const element of track.elements) {
			if (exclude.has(element.id)) continue;
			edges.add(element.startTime);
			edges.add(element.startTime + element.duration);
		}
	}
	return [...edges].sort((a, b) => a - b);
}

/**
 * `start` moved so the clip's start or end meets the nearest edge within
 * `threshold` (all in ticks), or unchanged when none is that close.
 */
export function snapStart({
	start,
	duration,
	edges,
	threshold,
}: {
	start: number;
	duration: number;
	edges: number[];
	threshold: number;
}): number {
	let best = start;
	let bestDistance = threshold;
	for (const edge of edges) {
		const fromStart = Math.abs(edge - start);
		if (fromStart <= bestDistance) {
			best = edge;
			bestDistance = fromStart;
		}
		const fromEnd = Math.abs(edge - (start + duration));
		if (fromEnd < bestDistance) {
			best = edge - duration;
			bestDistance = fromEnd;
		}
	}
	return Math.max(0, best);
}
