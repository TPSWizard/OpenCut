import type { TimelineElement } from "@/timeline";
import type { PlacementTimeSpan } from "./types";

interface TrackWithElements {
	elements: TimelineElement[];
}

function wouldElementOverlap({
	elements,
	startTime,
	endTime,
	excludeElementId,
}: {
	elements: TimelineElement[];
	startTime: number;
	endTime: number;
	excludeElementId?: string;
}): boolean {
	return elements.some((element) => {
		if (excludeElementId && element.id === excludeElementId) {
			return false;
		}

		const elementEnd = element.startTime + element.duration;
		return startTime < elementEnd && endTime > element.startTime;
	});
}

export function canPlaceTimeSpansOnTrack({
	track,
	timeSpans,
}: {
	track: TrackWithElements & { locked?: boolean };
	timeSpans: PlacementTimeSpan[];
}): boolean {
	// Nothing new lands on a locked track.
	if (track.locked) return false;
	return timeSpans.every(({ startTime, duration, excludeElementId }) => {
		return !wouldElementOverlap({
			elements: track.elements,
			startTime,
			endTime: startTime + duration,
			excludeElementId,
		});
	});
}
