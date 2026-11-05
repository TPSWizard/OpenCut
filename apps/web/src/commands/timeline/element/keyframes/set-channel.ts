import { EditorCore } from "@/core";
import { Command, type CommandResult } from "@/commands/base-command";
import { setChannel } from "@/animation";
import type { AnimationChannel, AnimationPath } from "@/animation/types";
import { updateElementInSceneTracks } from "@/timeline";
import type { SceneTracks } from "@/timeline";

/**
 * Replaces every key of one scalar channel in a single undo step, as auto
 * ducking does for `volume`. An undefined channel clears the path.
 */
export class SetChannelCommand extends Command {
	private savedState: SceneTracks | null = null;
	private readonly trackId: string;
	private readonly elementId: string;
	private readonly propertyPath: AnimationPath;
	private readonly channel: AnimationChannel | undefined;

	constructor({
		trackId,
		elementId,
		propertyPath,
		channel,
	}: {
		trackId: string;
		elementId: string;
		propertyPath: AnimationPath;
		channel: AnimationChannel | undefined;
	}) {
		super();
		this.trackId = trackId;
		this.elementId = elementId;
		this.propertyPath = propertyPath;
		this.channel = channel;
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const updatedTracks = updateElementInSceneTracks({
			tracks: this.savedState,
			trackId: this.trackId,
			elementId: this.elementId,
			update: (element) => ({
				...element,
				animations: setChannel({
					animations: element.animations,
					propertyPath: this.propertyPath,
					channel: this.channel,
				}),
			}),
		});
		editor.timeline.updateTracks(updatedTracks);
		return undefined;
	}

	undo(): void {
		if (!this.savedState) return;
		EditorCore.getInstance().timeline.updateTracks(this.savedState);
	}
}
