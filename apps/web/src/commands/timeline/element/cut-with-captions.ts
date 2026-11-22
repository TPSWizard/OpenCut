import { Command, type CommandResult } from "@/commands/base-command";
import { EditorCore } from "@/core";
import type { RippleScope } from "@/silence/detect";
import type { SceneTracks } from "@/timeline";
import { retimeCaptionTracks, type TickRange } from "@/transcript/cuts";
import { RemoveRangesCommand } from "./remove-ranges";

/**
 * Cuts stretches out of one element, as `RemoveRangesCommand` does, with
 * the timed captions retimed to match (words in the stretches go, later
 * words follow the ripple), in one undo step. Transcript editing and
 * filler removal use it.
 */
export class CutWithCaptionsCommand extends Command {
	private savedState: SceneTracks | null = null;
	private remaining: { trackId: string; elementId: string }[] = [];
	private readonly trackId: string;
	private readonly elementId: string;
	private readonly ranges: TickRange[];
	private readonly ripple: RippleScope;

	constructor({ trackId, elementId, ranges, ripple }: { trackId: string; elementId: string; ranges: TickRange[]; ripple: RippleScope }) {
		super();
		this.trackId = trackId;
		this.elementId = elementId;
		this.ranges = ranges;
		this.ripple = ripple;
	}

	/** The pieces of the element left on the timeline after the cut. */
	getRemainingElements(): { trackId: string; elementId: string }[] {
		return this.remaining;
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		editor.timeline.updateTracks(retimeCaptionTracks({ tracks: this.savedState, ranges: this.ranges, ripple: this.ripple }));
		const cut = new RemoveRangesCommand({ trackId: this.trackId, elementId: this.elementId, ranges: this.ranges, ripple: this.ripple });
		const result = cut.execute();
		this.remaining = cut.getRemainingElements();
		return result;
	}

	undo(): void {
		if (!this.savedState) return;
		EditorCore.getInstance().timeline.updateTracks(this.savedState);
	}
}
