import type { EditorCore } from "@/core";
import type { TimelineElement, TimelineTrack } from "@/timeline/types";
import type { ElementTransition } from "./types";
import { isVisualElement } from "./window";

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
