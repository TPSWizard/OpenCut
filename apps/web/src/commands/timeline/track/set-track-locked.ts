import { Command, type CommandResult } from "@/commands/base-command";
import type { SceneTracks } from "@/timeline";
import { EditorCore } from "@/core";
import { findTrackInSceneTracks, updateTrackInSceneTracks } from "@/timeline";

/** Locks or unlocks a track; locking also drops its clips from the selection. */
export class SetTrackLockedCommand extends Command {
	private savedState: SceneTracks | null = null;

	constructor(
		private trackId: string,
		private locked: boolean,
	) {
		super();
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const track = findTrackInSceneTracks({ tracks: this.savedState, trackId: this.trackId });
		if (!track) return;
		const updatedTracks = updateTrackInSceneTracks({
			tracks: this.savedState,
			trackId: this.trackId,
			update: (current) => {
				const next = { ...current };
				if (this.locked) next.locked = true;
				else delete next.locked;
				return next;
			},
		});
		editor.timeline.updateTracks(updatedTracks);
		if (this.locked) {
			const selected = editor.selection.getSelectedElements();
			const kept = selected.filter((ref) => ref.trackId !== this.trackId);
			if (kept.length !== selected.length) editor.selection.setSelectedElements({ elements: kept });
		}
	}

	undo(): void {
		if (this.savedState) {
			EditorCore.getInstance().timeline.updateTracks(this.savedState);
		}
	}
}
