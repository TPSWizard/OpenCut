/**
 * Puts clips into a split-screen layout on the timeline: each one's
 * transform keyframes cleared, its transform set to cover its cell, and
 * its masks replaced by a rectangle cropped to the cell, in one undo step
 * (`split_screen_commands` in the Rust editor does the same).
 */

import { BatchCommand } from "@/commands";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import { SetChannelCommand } from "@/commands/timeline/element/keyframes/set-channel";
import type { EditorCore } from "@/core";
import { buildDefaultMaskInstance } from "@/masks";
import type { Mask } from "@/masks/types";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { planSplitScreen, splitLayout, type SplitPlacement } from "./layout";

const TRANSFORM_PATHS = ["transform.positionX", "transform.positionY", "transform.scaleX", "transform.scaleY", "transform.rotate"] as const;

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

export interface SplitTarget {
	trackId: string;
	elementId: string;
}

/** Arranges `targets` (in cell order) into the layout; returns the placements. */
export function applySplitScreen({
	editor,
	layoutId,
	targets,
	gap = 0,
}: {
	editor: EditorCore;
	layoutId: string;
	targets: readonly SplitTarget[];
	gap?: number;
}): SplitPlacement[] {
	const layout = splitLayout(layoutId);
	if (!layout) throw new Error(`Unknown layout ${layoutId}`);
	const canvas = editor.project.getActive()?.settings.canvasSize;
	if (!canvas) throw new Error("No project is open");
	const tracks = allTracks(editor.scenes.getActiveScene().tracks);
	const clips = targets.map(({ trackId, elementId }) => {
		const track = tracks.find((candidate) => candidate.id === trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
		if (!track || !element || (element.type !== "video" && element.type !== "image")) {
			throw new Error("Split screen works on video and picture clips");
		}
		if (track.locked) throw new Error("A selected clip is on a locked track");
		const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
		if (!asset?.width || !asset.height) throw new Error(`The size of ${element.name} is not known`);
		return { trackId, element, width: asset.width, height: asset.height };
	});
	const placements = planSplitScreen({
		layout,
		canvasWidth: canvas.width,
		canvasHeight: canvas.height,
		sources: clips.map((clip) => ({ width: clip.width, height: clip.height })),
		gap,
	});
	const updates = clips.map((clip, index) => {
		const placement = placements[index];
		const base = buildDefaultMaskInstance({ maskType: "rectangle" });
		const mask = {
			...base,
			params: { ...base.params, centerX: 0, centerY: 0, rotation: 0, width: placement.maskWidth, height: placement.maskHeight },
		} as Mask;
		return {
			trackId: clip.trackId,
			elementId: clip.element.id,
			patch: {
				params: {
					...clip.element.params,
					"transform.positionX": placement.positionX,
					"transform.positionY": placement.positionY,
					"transform.scaleX": placement.scale,
					"transform.scaleY": placement.scale,
					"transform.rotate": 0,
				},
				masks: [mask],
			} as Partial<TimelineElement>,
		};
	});
	editor.command.execute({
		command: new BatchCommand([
			new UpdateElementsCommand({ updates }),
			...clips.flatMap((clip) =>
				TRANSFORM_PATHS.map(
					(propertyPath) => new SetChannelCommand({ trackId: clip.trackId, elementId: clip.element.id, propertyPath, channel: undefined }),
				),
			),
		]),
	});
	return placements;
}
