import { Command, type CommandResult } from "@/commands/base-command";
import type { SceneTracks } from "@/timeline";
import { EditorCore } from "@/core";
import { canTrackHaveAudio, findTrackInSceneTracks, updateTrackInSceneTracks } from "@/timeline";
import { TRACK_VOLUME_MAX_DB, TRACK_VOLUME_MIN_DB } from "@/timeline/track-mix";

/**
 * Sets a sounding track's mixer settings (those given): solo, volume in dB
 * (kept within -60 to +12) and pan (-1 to 1). One undo step.
 */
export class SetTrackMixCommand extends Command {
	private savedState: SceneTracks | null = null;

	constructor(
		private trackId: string,
		private mix: { solo?: boolean; volume?: number; pan?: number },
	) {
		super();
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const track = findTrackInSceneTracks({ tracks: this.savedState, trackId: this.trackId });
		if (!track || !canTrackHaveAudio(track)) return;
		const finite = (value: number | undefined, min: number, max: number) =>
			value === undefined ? undefined : Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : 0;
		const { solo, volume, pan } = this.mix;
		const updated = updateTrackInSceneTracks({
			tracks: this.savedState,
			trackId: this.trackId,
			update: (current) =>
				canTrackHaveAudio(current)
					? {
							...current,
							...(solo !== undefined ? { solo } : {}),
							...(volume !== undefined ? { volume: finite(volume, TRACK_VOLUME_MIN_DB, TRACK_VOLUME_MAX_DB) } : {}),
							...(pan !== undefined ? { pan: finite(pan, -1, 1) } : {}),
						}
					: current,
		});
		editor.timeline.updateTracks(updated);
	}

	undo(): void {
		if (this.savedState) {
			EditorCore.getInstance().timeline.updateTracks(this.savedState);
		}
	}
}
