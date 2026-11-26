/**
 * Caption find and replace, highlight by search and auto emoji (row 94)
 * across every caption, through the shared `captionedit` crate (WASM),
 * the same as the desktop: whole words in order, any case, punctuation
 * kept; replaced words keep the matched words' time; auto emoji adds one
 * emoji from our own word list after the first fitting word of each
 * caption. Each change is one undo step.
 */

import { captionEdit, captionFindCount } from "opencut-wasm";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import type { TimelineElement } from "@/timeline/types";
import { roundMediaTime } from "@/wasm";
import { captionElements } from "./keyword-commands";
import type { CaptionWord } from "./types";

export type CaptionEditOp = "replace" | "highlight" | "addEmoji" | "removeEmoji";

interface EditedCaption {
	id: string;
	content: string;
	words: { text: string; start: number; end: number; keyword?: boolean }[];
}

function captionsJson(editor: EditorCore) {
	const captions = captionElements({ editor });
	const json = JSON.stringify(
		captions.map(({ element }) => ({
			id: element.id,
			words: (element.words ?? []).map((word) => ({ text: word.text, start: word.start, end: word.end, keyword: word.keyword === true })),
		})),
	);
	return { captions, json };
}

/** How many places the search matches across the captions. */
export function countCaptionMatches({ editor, query }: { editor: EditorCore; query: string }): number {
	if (!query.trim()) return 0;
	return captionFindCount(captionsJson(editor).json, query);
}

/** Runs a caption edit; returns how many captions changed. */
export function editCaptions({
	editor,
	op,
	query = "",
	replacement = "",
}: {
	editor: EditorCore;
	op: CaptionEditOp;
	query?: string;
	replacement?: string;
}): number {
	const { captions, json } = captionsJson(editor);
	const edited = JSON.parse(captionEdit(op, json, query, replacement)) as EditedCaption[];
	if (edited.length === 0) return 0;
	const byId = new Map(captions.map((caption) => [caption.element.id, caption]));
	const updates = edited.flatMap((caption) => {
		const found = byId.get(caption.id);
		if (!found) return [];
		const words: CaptionWord[] = caption.words.map((word) => ({
			text: word.text,
			start: roundMediaTime({ time: word.start }),
			end: roundMediaTime({ time: word.end }),
			...(word.keyword ? { keyword: true } : {}),
		}));
		return [
			{
				trackId: found.trackId,
				elementId: caption.id,
				patch: { words, params: { ...found.element.params, content: caption.content } } as Partial<TimelineElement>,
			},
		];
	});
	editor.command.execute({ command: new UpdateElementsCommand({ updates }) });
	return updates.length;
}
