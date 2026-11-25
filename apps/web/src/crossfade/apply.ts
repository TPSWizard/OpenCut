/**
 * The one-step audio crossfade on the web: the shared crate's plan
 * (`@/timeline/audio-crossfade`) applied as one undo step, like the
 * desktop's `editor::crossfade_commands`.
 */

import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import {
	CROSSFADE_CURVE_PARAM,
	CROSSFADE_PARAM,
	type CrossfadeCurve,
	planElementCrossfade,
} from "@/timeline/audio-crossfade";
import { findTrackInSceneTracks } from "@/timeline/track-element-update";
import type { TimelineElement } from "@/timeline/types";
import { roundMediaTime } from "@/wasm";

const TICKS = 120_000;

export interface CrossfadeResult {
	/** The crossfade made, seconds. */
	seconds: number;
	/** Trimmed off the two clips together to make room, seconds (the clips after moved this much left). */
	trimmedSeconds: number;
	/** The incoming clip. */
	nextElementId: string;
}

function trackOf(editor: EditorCore, trackId: string) {
	const track = findTrackInSceneTracks({ tracks: editor.scenes.getActiveScene().tracks, trackId });
	if (!track) throw new Error("The track is no longer on the timeline");
	if (track.locked) throw new Error("The track is locked");
	return track;
}

function run(editor: EditorCore, command: UpdateElementsCommand) {
	// The command moves the clips after itself; the editor's ripple must not move them again.
	const rippleMode = editor.command.isRippleEnabled;
	editor.command.isRippleEnabled = false;
	try {
		editor.command.execute({ command });
	} finally {
		editor.command.isRippleEnabled = rippleMode;
	}
}

/**
 * Crossfades a clip into the next audio or video clip on its track over
 * `seconds` (at most 10): trims the two where they have too little spare
 * audio, moves the clips after left to close the gap, and stores the
 * crossfade on the clip. Throws when there is no next clip, a clip has a
 * speed change, or they are too short.
 */
export function crossfadeElement({
	editor,
	trackId,
	elementId,
	seconds,
	curve,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	seconds: number;
	curve: CrossfadeCurve;
}): CrossfadeResult {
	const track = trackOf(editor, trackId);
	const elements = track.elements as TimelineElement[];
	const index = elements.findIndex((element) => element.id === elementId);
	if (index < 0) throw new Error("The clip is no longer on the timeline");
	const plan = planElementCrossfade(elements, index, seconds);
	const from = elements[index];
	const to = elements[plan.to];
	const edit = (e: { start: number; duration: number; trimStart: number; trimEnd: number }) => ({
		startTime: roundMediaTime({ time: e.start }),
		duration: roundMediaTime({ time: e.duration }),
		trimStart: roundMediaTime({ time: e.trimStart }),
		trimEnd: roundMediaTime({ time: e.trimEnd }),
	});
	const updates: { trackId: string; elementId: string; patch: Partial<TimelineElement> }[] = [
		{
			trackId,
			elementId: from.id,
			patch: {
				...edit(plan.fromEdit),
				params: { ...from.params, [CROSSFADE_PARAM]: plan.length / TICKS, [CROSSFADE_CURVE_PARAM]: curve },
			},
		},
		{ trackId, elementId: to.id, patch: edit(plan.toEdit) },
		...plan.moved.map((other) => ({
			trackId,
			elementId: elements[other].id,
			patch: { startTime: roundMediaTime({ time: elements[other].startTime - plan.shift }) },
		})),
	];
	run(editor, new UpdateElementsCommand({ updates }));
	return { seconds: plan.length / TICKS, trimmedSeconds: plan.shift / TICKS, nextElementId: to.id };
}

/** Removes a clip's crossfade into the next one (its trims stay). */
export function removeCrossfade({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): void {
	const track = trackOf(editor, trackId);
	const element = (track.elements as TimelineElement[]).find((candidate) => candidate.id === elementId);
	if (!element) throw new Error("The clip is no longer on the timeline");
	run(
		editor,
		new UpdateElementsCommand({ updates: [{ trackId, elementId, patch: { params: { ...element.params, [CROSSFADE_PARAM]: 0 } } }] }),
	);
}

/** The clip that follows `elementId` directly on the track and would be crossfaded into, if any. */
export function nextCrossfadeClip(elements: readonly TimelineElement[], elementId: string): TimelineElement | undefined {
	const element = elements.find((candidate) => candidate.id === elementId);
	if (!element) return undefined;
	const end = element.startTime + element.duration;
	return elements
		.filter(
			(candidate) =>
				candidate.id !== elementId &&
				(candidate.type === "audio" || candidate.type === "video") &&
				!("hidden" in candidate && candidate.hidden) &&
				candidate.duration > 0 &&
				Math.abs(candidate.startTime - end) <= 1_200,
		)
		.sort((a, b) => a.startTime - b.startTime)[0];
}
