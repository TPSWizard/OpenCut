/**
 * Resize project in the editor: the canvas size changed and every clip,
 * text and caption re-fitted (see `./plan`), in one undo step.
 */

import { BatchCommand } from "@/commands";
import { UpdateProjectSettingsCommand } from "@/commands/project/update-project-settings";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import type { TProjectSettings } from "@/project/types";
import { relayoutCaptions } from "@/shorts/make";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { captionMargin, contentFactor, type ResizeMode, resizeTracks, type Size } from "./plan";

export const MIN_CANVAS_SIDE = 16;
export const MAX_CANVAS_SIDE = 8192;

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

export interface ResizeResult {
	from: Size;
	to: Size;
	mode: ResizeMode;
	/** How much the content grew. */
	content: number;
	/** Elements changed, captions included. */
	changed: number;
	captions: number;
}

/**
 * Changes the canvas to `canvas` and re-fits the active scene's content.
 * `settings` carries the canvas-size mode the settings panel keeps.
 */
export function resizeProject({
	editor,
	canvas,
	mode,
	settings = {},
}: {
	editor: EditorCore;
	canvas: Size;
	mode: ResizeMode;
	settings?: Partial<TProjectSettings>;
}): ResizeResult {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const width = Math.round(canvas.width);
	const height = Math.round(canvas.height);
	if (!(width >= MIN_CANVAS_SIDE && height >= MIN_CANVAS_SIDE && width <= MAX_CANVAS_SIDE && height <= MAX_CANVAS_SIDE)) {
		throw new Error(`The canvas must be ${MIN_CANVAS_SIDE} to ${MAX_CANVAS_SIDE} pixels a side`);
	}
	const from = project.settings.canvasSize;
	const to = { width, height };
	const sizes = new Map(editor.media.getAssets().map((asset) => [asset.id, asset]));
	const original = editor.scenes.getActiveScene().tracks;
	const resized = relayoutCaptions({
		tracks: resizeTracks({
			tracks: original,
			from,
			to,
			mode,
			sizeOf: (mediaId) => {
				const asset = sizes.get(mediaId);
				return asset?.width && asset.height ? { width: asset.width, height: asset.height } : null;
			},
		}),
		canvas: to,
		margin: captionMargin(to),
	});
	const before = new Map(allTracks(original).flatMap((track) => (track.elements as TimelineElement[]).map((element) => [element.id, element])));
	const updates: { trackId: string; elementId: string; patch: Partial<TimelineElement> }[] = [];
	let captions = 0;
	for (const track of allTracks(resized)) {
		for (const element of track.elements as TimelineElement[]) {
			if (before.get(element.id) === element) continue;
			if (element.type === "text" && element.words?.length) captions++;
			const { id: _id, ...patch } = element;
			updates.push({ trackId: track.id, elementId: element.id, patch: patch as Partial<TimelineElement> });
		}
	}
	editor.command.execute({
		command: new BatchCommand([
			new UpdateProjectSettingsCommand({ ...settings, canvasSize: to }),
			...(updates.length ? [new UpdateElementsCommand({ updates })] : []),
		]),
	});
	return { from: { ...from }, to, mode, content: contentFactor({ mode, from, to }), changed: updates.length, captions };
}
