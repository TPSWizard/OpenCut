/**
 * Caption keywords: the words of a caption that matter most, drawn in the
 * keyword colour and a little larger (`caption.keywordColor`,
 * `caption.keywordScale`). A word is a keyword when its `keyword` flag is
 * set; the local pick below sets them, Claude can too (`/captions/keywords`
 * on the API worker), and any word can be toggled by hand.
 *
 * The local pick, the same as `crates/editor/src/caption_keywords.rs`
 * (both are checked against `fixtures/caption-keywords/cases.json`):
 * each word is stripped of the punctuation around it and scored —
 * anything with a digit first (numbers, prices, percentages), then names
 * (a capital letter not at the start of a sentence), then other words of
 * four or more letters by length; stop words and shorter words never
 * count. A caption of n words keeps its best round(n / 6) words, between
 * one and three, earlier words winning ties; a one-word caption keeps none.
 */

import type { CaptionWord } from "./types";

/** Common English words that never make a keyword; `STOP_WORDS` in the Rust module has the same list. */
export const STOP_WORDS: readonly string[] = [
	"a", "about", "above", "after", "again", "against", "all", "also", "am", "an", "and", "any", "are", "around", "as", "at",
	"be", "because", "been", "before", "being", "below", "between", "both", "but", "by", "can", "could", "did", "do", "does",
	"doing", "done", "down", "during", "each", "even", "ever", "every", "few", "for", "from", "further", "get", "gets", "getting",
	"go", "goes", "going", "gonna", "got", "had", "has", "have", "having", "he", "her", "here", "hers", "herself", "him",
	"himself", "his", "how", "i", "if", "in", "into", "is", "it", "its", "itself", "just", "kind", "know", "like", "lot", "make",
	"many", "maybe", "me", "might", "more", "most", "much", "must", "my", "myself", "need", "never", "no", "nor", "not", "now",
	"of", "off", "okay", "on", "once", "one", "only", "or", "other", "our", "ours", "ourselves", "out", "over", "own", "really",
	"right", "said", "same", "say", "says", "see", "she", "should", "so", "some", "something", "still", "such", "sure", "take",
	"than", "that", "the", "their", "theirs", "them", "themselves", "then", "there", "these", "they", "thing", "things", "think",
	"this", "those", "through", "to", "too", "under", "until", "up", "us", "very", "want", "was", "way", "we", "well", "were",
	"what", "when", "where", "which", "while", "who", "whom", "why", "will", "with", "would", "yeah", "yes", "yet", "you",
	"your", "yours", "yourself", "yourselves",
];

const STOP = new Set(STOP_WORDS);

/** The word without the punctuation around it (“$5,000!” → 5,000). */
export function keywordCore(text: string): string {
	return text.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
}

/** How much a word stands out, or 0 when it can never be a keyword. */
export function keywordScore({ text, previous }: { text: string; previous: string | null }): number {
	const core = keywordCore(text);
	const length = Array.from(core).length;
	if (length === 0) return 0;
	if (/\p{N}/u.test(core)) return 1000 + length;
	if (STOP.has(core.toLowerCase())) return 0;
	const sentenceStart = previous === null || /[.!?]["'”’)]*$/u.test(previous);
	if (!sentenceStart && /^\p{Lu}/u.test(core)) return 500 + length;
	return length >= 4 ? length : 0;
}

/** How many keywords a caption of `count` words keeps. */
export function keywordBudget(count: number): number {
	if (count <= 1) return 0;
	return Math.min(3, Math.max(1, Math.round(count / 6)));
}

/** The indices of the caption's keywords, in order. */
export function pickKeywordIndices({ words }: { words: readonly string[] }): number[] {
	const scored = words
		.map((text, index) => ({ index, score: keywordScore({ text, previous: index > 0 ? words[index - 1] : null }) }))
		.filter((candidate) => candidate.score > 0)
		.sort((a, b) => b.score - a.score || a.index - b.index);
	return scored
		.slice(0, keywordBudget(words.length))
		.map((candidate) => candidate.index)
		.sort((a, b) => a - b);
}

/** The words with `keyword` set on exactly `indices` (others cleared). */
export function withKeywords({ words, indices }: { words: readonly CaptionWord[]; indices: readonly number[] }): CaptionWord[] {
	const wanted = new Set(indices);
	return words.map((word, index) => {
		const { keyword: _old, ...rest } = word;
		return wanted.has(index) ? { ...rest, keyword: true } : rest;
	});
}

/** The indices of the words flagged as keywords. */
export function keywordIndices({ words }: { words: readonly CaptionWord[] | undefined }): number[] {
	const indices: number[] = [];
	(words ?? []).forEach((word, index) => {
		if (word.keyword) indices.push(index);
	});
	return indices;
}
