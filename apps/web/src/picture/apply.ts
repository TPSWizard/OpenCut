/**
 * Cleans up an image clip's picture (`./cleanup`): the result is stored as
 * a new PNG in the project and the clip shows it, in one undo step. The
 * clip remembers its original (`picture.source`), so cleaning again starts
 * from the original and "Restore original" puts it back; the traced path
 * is kept as JSON in `picture.path` (the studio's `TracedPath` shape).
 */

import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { processMediaAssets } from "@/media/processing";
import type { ImageElement, TimelineElement } from "@/timeline/types";
import { DEFAULT_PICTURE_CLEANUP, cleanPicture, type PictureCleanup, type TracedPath } from "./cleanup";

export const PICTURE_SOURCE_PARAM = "picture.source";
export const PICTURE_PATH_PARAM = "picture.path";

export interface CleanupResult {
	mediaId: string;
	width: number;
	height: number;
	path: TracedPath | null;
}

function findImage({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): ImageElement {
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = [...tracks.overlay, tracks.main, ...tracks.audio].find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!element || element.type !== "image") throw new Error("Picture cleanup needs an image clip");
	return element;
}

/** The traced path stored on a clip, if any. */
export function storedPath({ element }: { element: ImageElement }): TracedPath | null {
	const raw = element.params[PICTURE_PATH_PARAM];
	if (typeof raw !== "string" || !raw) return null;
	try {
		const parsed = JSON.parse(raw) as TracedPath;
		return Array.isArray(parsed.points) ? parsed : null;
	} catch {
		return null;
	}
}

async function decode(file: File): Promise<{ width: number; height: number; rgba: Uint8ClampedArray }> {
	const bitmap = await createImageBitmap(file);
	const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
	const context = canvas.getContext("2d");
	if (!context) throw new Error("No 2D canvas");
	context.drawImage(bitmap, 0, 0);
	bitmap.close();
	const image = context.getImageData(0, 0, canvas.width, canvas.height);
	return { width: canvas.width, height: canvas.height, rgba: image.data };
}

export async function cleanUpPicture({
	editor,
	trackId,
	elementId,
	options = {},
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<PictureCleanup>;
}): Promise<CleanupResult> {
	const settings: PictureCleanup = { ...DEFAULT_PICTURE_CLEANUP, ...options };
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const element = findImage({ editor, trackId, elementId });
	const assets = editor.media.getAssets();
	const sourceId = String(element.params[PICTURE_SOURCE_PARAM] ?? "") || element.mediaId;
	const source = assets.find((asset) => asset.id === sourceId) ?? assets.find((asset) => asset.id === element.mediaId);
	if (!source?.file) throw new Error("The picture's file is not available");

	const { picture, path } = cleanPicture(await decode(source.file), settings);
	const canvas = new OffscreenCanvas(picture.width, picture.height);
	const context = canvas.getContext("2d");
	if (!context) throw new Error("No 2D canvas");
	context.putImageData(new ImageData(new Uint8ClampedArray(picture.rgba), picture.width, picture.height), 0, 0);
	const blob = await canvas.convertToBlob({ type: "image/png" });
	const base = source.name.replace(/\.[^.]+$/, "");
	const [processed] = await processMediaAssets({
		files: [new File([blob], `${base} (cleaned).png`, { type: "image/png" })],
	});
	if (!processed) throw new Error("The cleaned picture could not be prepared");
	const saved = await editor.media.addMediaAsset({ projectId: project.metadata.id, asset: processed });
	if (!saved) throw new Error("The cleaned picture could not be stored");

	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [
				{
					trackId,
					elementId,
					patch: {
						mediaId: saved.id,
						params: {
							...element.params,
							[PICTURE_SOURCE_PARAM]: source.id,
							[PICTURE_PATH_PARAM]: path ? JSON.stringify(path) : "",
						},
					} as Partial<TimelineElement>,
				},
			],
		}),
	});
	return { mediaId: saved.id, width: picture.width, height: picture.height, path };
}

/** Shows the original picture again (the cleaned PNG stays in the media). */
export function restoreOriginalPicture({ editor, trackId, elementId }: { editor: EditorCore; trackId: string; elementId: string }): boolean {
	const element = findImage({ editor, trackId, elementId });
	const sourceId = String(element.params[PICTURE_SOURCE_PARAM] ?? "");
	if (!sourceId || !editor.media.getAssets().some((asset) => asset.id === sourceId)) return false;
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [
				{
					trackId,
					elementId,
					patch: {
						mediaId: sourceId,
						params: { ...element.params, [PICTURE_SOURCE_PARAM]: "", [PICTURE_PATH_PARAM]: "" },
					} as Partial<TimelineElement>,
				},
			],
		}),
	});
	return true;
}
