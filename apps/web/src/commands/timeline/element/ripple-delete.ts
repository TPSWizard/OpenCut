import { Command, type CommandResult } from "@/commands/base-command";
import { EditorCore } from "@/core";
import { allTracks, deleteRangePlan, elementsCrossing, rippleDeletePlan, type RipplePlan } from "@/timeline/edits";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { SplitElementsCommand } from "./split-elements";

function applyPlan({ tracks, plan }: { tracks: SceneTracks; plan: RipplePlan }): SceneTracks {
	const removed = new Set(plan.remove.map((ref) => ref.elementId));
	const moved = new Map(plan.moves.map((move) => [move.elementId, move.startTime]));
	const update = (track: TimelineTrack): TimelineTrack =>
		({
			...track,
			elements: (track.elements as TimelineElement[])
				.filter((element) => !removed.has(element.id))
				.map((element) => {
					const startTime = moved.get(element.id);
					return startTime === undefined ? element : { ...element, startTime };
				}),
		}) as TimelineTrack;
	return {
		overlay: tracks.overlay.map(update) as SceneTracks["overlay"],
		main: update(tracks.main) as SceneTracks["main"],
		audio: tracks.audio.map(update) as SceneTracks["audio"],
	};
}

/**
 * Deletes elements and closes the gaps they leave: everything that starts
 * after a removed span moves left by the removed length, on every track.
 * One undo step.
 */
export class RippleDeleteCommand extends Command {
	private savedState: SceneTracks | null = null;
	private plan: RipplePlan | null = null;

	constructor(private readonly options: { elements: { trackId: string; elementId: string }[] }) {
		super();
	}

	getPlan(): RipplePlan | null {
		return this.plan;
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		const tracks = editor.scenes.getActiveScene().tracks;
		this.savedState = tracks;
		this.plan = rippleDeletePlan({ tracks: allTracks({ tracks }), elements: this.options.elements });
		editor.timeline.updateTracks(applyPlan({ tracks, plan: this.plan }));
		return undefined;
	}

	undo(): void {
		if (!this.savedState) return;
		EditorCore.getInstance().timeline.updateTracks(this.savedState);
	}
}

/**
 * Removes the stretch `[from, to)` of the timeline on every track: elements
 * crossing either end are split there, the pieces inside go, and everything
 * from `to` on moves left by the stretch's length. One undo step.
 */
export class DeleteRangeCommand extends Command {
	private savedState: SceneTracks | null = null;
	private plan: RipplePlan | null = null;

	constructor(private readonly options: { from: number; to: number }) {
		super();
	}

	getPlan(): RipplePlan | null {
		return this.plan;
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const { from, to } = this.options;
		if (to <= from) {
			this.plan = { remove: [], moves: [], removedTicks: 0 };
			return undefined;
		}
		// Later cut first, so the earlier one keeps its timeline position.
		for (const time of [to, from]) {
			const crossing = elementsCrossing({ tracks: allTracks({ tracks: editor.scenes.getActiveScene().tracks }), time });
			if (crossing.length > 0) {
				new SplitElementsCommand({ elements: crossing, splitTime: time as never }).execute();
			}
		}
		const tracks = editor.scenes.getActiveScene().tracks;
		this.plan = deleteRangePlan({ tracks: allTracks({ tracks }), from, to });
		editor.timeline.updateTracks(applyPlan({ tracks, plan: this.plan }));
		return undefined;
	}

	undo(): void {
		if (!this.savedState) return;
		EditorCore.getInstance().timeline.updateTracks(this.savedState);
	}
}
