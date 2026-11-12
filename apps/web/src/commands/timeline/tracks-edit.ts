import { Command, createElementSelectionResult, type CommandResult } from "@/commands/base-command";
import type { ElementRef, SceneTracks } from "@/timeline";
import { EditorCore } from "@/core";

/**
 * Replaces the tracks with a precomputed result (grouping, compound clips)
 * as one undo step and selects `selection`. Ripple editing does not close
 * the gaps it leaves: taking clips into a compound is not a deletion.
 */
export class TracksEditCommand extends Command {
	readonly skipsRipple = true;
	private before: SceneTracks | null = null;

	constructor(
		private after: SceneTracks,
		private selection?: ElementRef[],
	) {
		super();
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.before = editor.scenes.getActiveScene().tracks;
		editor.timeline.updateTracks(this.after);
		return this.selection ? createElementSelectionResult(this.selection) : undefined;
	}

	undo(): void {
		if (this.before) EditorCore.getInstance().timeline.updateTracks(this.before);
	}
}
