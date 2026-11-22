/**
 * Setting caption keywords on the timeline: a local pick (`keywords.ts`),
 * Claude's pick through the API worker (`POST /captions/keywords`, the
 * server's key and passcode as for shorts), clearing them, and toggling
 * one word. Each change to the timeline is one undo step.
 */

import { UpdateElementsCommand } from "@/commands/timeline/element/update-elements";
import type { EditorCore } from "@/core";
import { apiUrl } from "@/lib/api";
import type { SceneTracks, TextElement, TimelineTrack } from "@/timeline/types";
import { keywordIndices, pickKeywordIndices, withKeywords } from "./keywords";

export interface CaptionRef {
	trackId: string;
	element: TextElement;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/** Text elements with timed words in the active scene (or just `elementIds`), in timeline order. */
export function captionElements({ editor, elementIds }: { editor: EditorCore; elementIds?: readonly string[] }): CaptionRef[] {
	const wanted = elementIds ? new Set(elementIds) : null;
	const found: CaptionRef[] = [];
	for (const track of allTracks(editor.scenes.getActiveScene().tracks)) {
		if (track.locked) continue;
		for (const element of track.elements as TextElement[]) {
			if (element.type !== "text" || !element.words?.length) continue;
			if (wanted && !wanted.has(element.id)) continue;
			found.push({ trackId: track.id, element });
		}
	}
	return found.sort((a, b) => a.element.startTime - b.element.startTime);
}

export interface KeywordResult {
	/** Captions changed. */
	captions: number;
	/** Keywords set across them. */
	keywords: number;
}

/** Sets each caption's keywords to `picks[i]` (indices into its words) in one undo step. */
export function applyKeywordPicks({
	editor,
	captions,
	picks,
}: {
	editor: EditorCore;
	captions: readonly CaptionRef[];
	picks: readonly (readonly number[])[];
}): KeywordResult {
	let keywords = 0;
	const updates = captions
		.map((caption, index) => {
			const words = caption.element.words ?? [];
			const indices = [...new Set(picks[index] ?? [])]
				.filter((at) => Number.isInteger(at) && at >= 0 && at < words.length)
				.sort((a, b) => a - b);
			keywords += indices.length;
			if (keywordIndices({ words }).join() === indices.join()) return null;
			return { trackId: caption.trackId, elementId: caption.element.id, patch: { words: withKeywords({ words, indices }) } };
		})
		.filter((update) => update !== null);
	if (updates.length > 0) editor.command.execute({ command: new UpdateElementsCommand({ updates }) });
	return { captions: updates.length, keywords };
}

/** The local pick for every caption (or `elementIds`). */
export function pickCaptionKeywords({ editor, elementIds }: { editor: EditorCore; elementIds?: readonly string[] }): KeywordResult {
	const captions = captionElements({ editor, elementIds });
	const picks = captions.map((caption) => pickKeywordIndices({ words: (caption.element.words ?? []).map((word) => word.text) }));
	return applyKeywordPicks({ editor, captions, picks });
}

/** Clears the keywords of every caption (or `elementIds`). */
export function clearCaptionKeywords({ editor, elementIds }: { editor: EditorCore; elementIds?: readonly string[] }): KeywordResult {
	const captions = captionElements({ editor, elementIds });
	return applyKeywordPicks({ editor, captions, picks: captions.map(() => []) });
}

/** Makes one word a keyword or a plain word again. */
export function toggleCaptionKeyword({
	editor,
	trackId,
	element,
	index,
}: {
	editor: EditorCore;
	trackId: string;
	element: TextElement;
	index: number;
}): void {
	const current = keywordIndices({ words: element.words });
	const next = current.includes(index) ? current.filter((at) => at !== index) : [...current, index];
	applyKeywordPicks({ editor, captions: [{ trackId, element }], picks: [next] });
}

/** Claude's keywords for the captions (indices into each caption's words), through the API worker. */
export async function fetchClaudeKeywords({
	captions,
	passcode,
	fetchImpl = fetch,
}: {
	captions: readonly (readonly string[])[];
	passcode: string;
	fetchImpl?: typeof fetch;
}): Promise<{ picks: number[][]; model: string }> {
	const response = await fetchImpl(apiUrl("/captions/keywords"), {
		method: "POST",
		headers: { "Content-Type": "application/json", "X-Share-Key": passcode },
		body: JSON.stringify({ captions: captions.map((words) => ({ words })) }),
	});
	const body = (await response.json().catch(() => null)) as { keywords?: unknown; model?: unknown; error?: unknown } | null;
	if (!response.ok) throw new Error(typeof body?.error === "string" ? body.error : `Picking keywords failed (${response.status})`);
	const keywords = Array.isArray(body?.keywords) ? (body.keywords as unknown[]) : null;
	if (!keywords || keywords.length !== captions.length) throw new Error("The server's answer did not cover every caption");
	const picks = keywords.map((list, index) =>
		Array.isArray(list) ? list.filter((at): at is number => Number.isInteger(at) && at >= 0 && at < captions[index].length) : [],
	);
	return { picks, model: typeof body?.model === "string" ? body.model : "" };
}

/** Claude's pick for every caption (or `elementIds`), applied in one undo step. */
export async function pickCaptionKeywordsWithClaude({
	editor,
	passcode,
	elementIds,
	fetchImpl,
}: {
	editor: EditorCore;
	passcode: string;
	elementIds?: readonly string[];
	fetchImpl?: typeof fetch;
}): Promise<KeywordResult & { model: string }> {
	const captions = captionElements({ editor, elementIds });
	if (captions.length === 0) return { captions: 0, keywords: 0, model: "" };
	const { picks, model } = await fetchClaudeKeywords({
		captions: captions.map((caption) => (caption.element.words ?? []).map((word) => word.text)),
		passcode,
		fetchImpl,
	});
	return { ...applyKeywordPicks({ editor, captions, picks }), model };
}
