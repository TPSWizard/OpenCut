import { richTextClearRange, richTextStyleRange, richTextStyles } from "opencut-wasm";
import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import type { ParamValues } from "@/params";
import type { TimelineElement } from "@/timeline/types";

/**
 * Rich text: colour, weight, slant and size for parts of one text clip.
 * The `richText` param keeps the styled ranges with the text they were made
 * for; the shared Rust `richtext` crate (WASM) works out each character's
 * style for the clip's current text, moving the styles with any edits, and
 * applies or clears a style on a range, as on the desktop. Checked against
 * `fixtures/rich-text/cases.json`.
 */

export const RICH_TEXT_PARAM = "richText";
export const RICH_SCALE_MIN = 0.25;
export const RICH_SCALE_MAX = 4;

/** One character's own style; a field left out keeps the clip's. */
export interface RichCharStyle {
	color?: string;
	bold?: boolean;
	italic?: boolean;
	scale?: number;
}

export type RichPatch = RichCharStyle;

const resolved = new Map<string, RichCharStyle[] | null>();
const RESOLVED_KEEP = 64;

/** Each character's style for `content`, or null when nothing is styled. */
export function richCharStyles(stored: unknown, content: string): RichCharStyle[] | null {
	if (typeof stored !== "string" || stored.length === 0) return null;
	const key = `${content}\u0000${stored}`;
	if (resolved.has(key)) return resolved.get(key) ?? null;
	const json = richTextStyles(stored, content);
	const styles = json ? (JSON.parse(json) as RichCharStyle[]) : null;
	resolved.set(key, styles);
	if (resolved.size > RESOLVED_KEEP) {
		const oldest = resolved.keys().next().value;
		if (oldest !== undefined) resolved.delete(oldest);
	}
	return styles;
}

/** A text element's character styles from its params. */
export function readRichStyles(params: ParamValues | undefined): RichCharStyle[] | null {
	const content = typeof params?.content === "string" ? params.content : "";
	return richCharStyles(params?.[RICH_TEXT_PARAM], content);
}

/** A position in UTF-16 code units (a text field's selection) as characters. */
export function charIndex(text: string, utf16: number): number {
	return Array.from(text.slice(0, Math.max(0, utf16))).length;
}

/** The `richText` JSON after styling (or clearing, `patch` null) characters `start..end`. */
export function restyle({
	stored,
	content,
	start,
	end,
	patch,
}: {
	stored: unknown;
	content: string;
	start: number;
	end: number;
	patch: RichPatch | null;
}): string {
	const from = typeof stored === "string" ? stored : "";
	const a = Math.max(0, Math.floor(Math.min(start, end)));
	const b = Math.max(0, Math.floor(Math.max(start, end)));
	return patch ? richTextStyleRange(from, content, a, b, JSON.stringify(patch)) : richTextClearRange(from, content, a, b);
}

/** Styles (or clears) characters `start..end` of a text element, in one undo step. */
export function applyRichStyle({
	editor,
	trackId,
	element,
	start,
	end,
	patch,
}: {
	editor: EditorCore;
	trackId: string;
	element: TimelineElement & { params: ParamValues };
	start: number;
	end: number;
	patch: RichPatch | null;
}): string {
	const content = typeof element.params.content === "string" ? element.params.content : "";
	const next = restyle({ stored: element.params[RICH_TEXT_PARAM], content, start, end, patch });
	editor.command.execute({
		command: new UpdateElementsCommand({
			updates: [
				{
					trackId,
					elementId: element.id,
					patch: { params: { ...element.params, [RICH_TEXT_PARAM]: next } } as Partial<TimelineElement>,
				},
			],
		}),
	});
	return next;
}
