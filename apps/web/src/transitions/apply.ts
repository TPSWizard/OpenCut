import type { EditorCore } from "@/core";
import type { TimelineElement, TimelineTrack } from "@/timeline/types";
import type { ElementTransition } from "./types";
import { getPreviousAdjacent, isVisualElement } from "./window";

export type TransitionSide = "start" | "end";

/**
 * Where a clip's start or end transition lives: its end is its own
 * `transition`; its start is the previous clip's `transition` when a clip
 * sits directly before it (one transition per cut), else its own
 * `transitionIn` (in from nothing).
 */
export function transitionTarget({
	track,
	element,
	side,
}: {
	track: TimelineTrack;
	element: TimelineElement;
	side: TransitionSide;
}): { elementId: string; field: "transition" | "transitionIn"; sharedWith: TimelineElement | null } {
	if (side === "end") return { elementId: element.id, field: "transition", sharedWith: null };
	const previous = getPreviousAdjacent({ track, element });
	return previous
		? { elementId: previous.id, field: "transition", sharedWith: previous }
		: { elementId: element.id, field: "transitionIn", sharedWith: null };
}

/** The transition at a clip's start or end (see `transitionTarget`). */
export function getClipTransition({
	track,
	element,
	side,
}: {
	track: TimelineTrack;
	element: TimelineElement;
	side: TransitionSide;
}): ElementTransition | null {
	const target = transitionTarget({ track, element, side });
	const owner = track.elements.find((candidate) => candidate.id === target.elementId);
	if (!owner || !isVisualElement(owner)) return null;
	return owner[target.field] ?? null;
}

/** Sets (or, with null, removes) the transition at a clip's start or end. */
export function setClipTransition({
	editor,
	track,
	element,
	side,
	transition,
}: {
	editor: EditorCore;
	track: TimelineTrack;
	element: TimelineElement;
	side: TransitionSide;
	transition: ElementTransition | null;
}) {
	const target = transitionTarget({ track, element, side });
	editor.timeline.updateElements({
		updates: [
			{
				trackId: track.id,
				elementId: target.elementId,
				patch: { [target.field]: transition ?? undefined } as Partial<TimelineElement>,
			},
		],
	});
}

/** Sets (or, with null, removes) the transition a clip leads out of. */
export function setElementTransition({
	editor,
	trackId,
	elementId,
	transition,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	transition: ElementTransition | null;
}) {
	editor.timeline.updateElements({
		updates: [
			{
				trackId,
				elementId,
				patch: { transition: transition ?? undefined } as Partial<TimelineElement>,
			},
		],
	});
}

/** The selected visual clips with their tracks, in selection order. */
export function getSelectedVisualClips({
	editor,
}: {
	editor: EditorCore;
}): { track: TimelineTrack; element: TimelineElement }[] {
	const { tracks } = editor.scenes.getActiveScene();
	const all = [...tracks.overlay, tracks.main, ...tracks.audio];
	const found: { track: TimelineTrack; element: TimelineElement }[] = [];
	for (const ref of editor.selection.getSelectedElements()) {
		const track = all.find((candidate) => candidate.id === ref.trackId);
		const element = track?.elements.find((candidate) => candidate.id === ref.elementId);
		if (track && element && isVisualElement(element)) {
			found.push({ track, element });
		}
	}
	return found;
}
