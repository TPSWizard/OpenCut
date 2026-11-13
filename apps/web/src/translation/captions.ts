/**
 * Caption translation: the text elements of a caption track are translated
 * and copied onto a new text track straight above it, each copy keeping
 * the original's timing, position and look. Word timings belong to the
 * original language, so copies drop them (and their word highlight).
 */

import { AddTrackCommand, BatchCommand, InsertElementCommand } from "@/commands";
import type { EditorCore } from "@/core";
import type { CreateTimelineElement, TextElement, TimelineTrack } from "@/timeline/types";
import { translateTexts, type TranslatedTexts, type TranslationEnginePreference, type TranslationProgress } from "./engine";

export interface CaptionTrackInfo {
	track: TimelineTrack;
	captions: number;
}

function captionElements({ track }: { track: TimelineTrack }): TextElement[] {
	if (track.type !== "text") return [];
	return (track.elements as TextElement[])
		.filter((element) => element.type === "text" && String(element.params.content ?? "").trim() !== "")
		.sort((a, b) => a.startTime - b.startTime);
}

/** Text tracks with captions on them, top first. */
export function listCaptionTracks({ editor }: { editor: EditorCore }): CaptionTrackInfo[] {
	const { tracks } = editor.scenes.getActiveScene();
	return tracks.overlay
		.map((track) => ({ track, captions: captionElements({ track }).length }))
		.filter((info) => info.captions > 0);
}

/** A copy of a caption with new text, ready to insert. */
export function translatedCopy({ element, text }: { element: TextElement; text: string }): CreateTimelineElement {
	const { id: _id, words: _words, ...rest } = element;
	const params: Record<string, unknown> = { ...element.params, content: text };
	if (element.words && element.words.length > 0) params["caption.highlight"] = "none";
	return { ...rest, params } as CreateTimelineElement;
}

export interface TranslatedTrack extends TranslatedTexts {
	trackId: string;
	captions: number;
}

/** Translates a caption track into a new track above it (one undo step). */
export async function translateCaptionTrack({
	editor,
	trackId,
	from,
	to,
	engine = "auto",
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	from: string;
	to: string;
	engine?: TranslationEnginePreference;
	onProgress?: (progress: TranslationProgress) => void;
}): Promise<TranslatedTrack> {
	const overlay = editor.scenes.getActiveScene().tracks.overlay;
	const index = overlay.findIndex((track) => track.id === trackId);
	if (index < 0) throw new Error("Pick a caption track to translate");
	const elements = captionElements({ track: overlay[index] });
	if (elements.length === 0) throw new Error("That track has no captions");
	const result = await translateTexts({
		texts: elements.map((element) => String(element.params.content ?? "")),
		from,
		to,
		engine,
		onProgress,
	});
	const add = new AddTrackCommand({ type: "text", index });
	const newTrackId = add.getTrackId();
	const inserts = elements.map(
		(element, position) =>
			new InsertElementCommand({
				placement: { mode: "explicit", trackId: newTrackId },
				element: translatedCopy({ element, text: result.translations[position] ?? "" }),
			}),
	);
	editor.command.execute({ command: new BatchCommand([add, ...inserts]) });
	return { ...result, trackId: newTrackId, captions: elements.length };
}
