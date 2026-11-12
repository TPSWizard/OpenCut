/**
 * Filler words ("um", "uh", "ähm", "euh", …) found among timed words, and
 * the stretches to cut them out softly: each filler's span with a little
 * padding, never reaching into the neighbouring words, merged where
 * fillers follow each other. The lists are per language; "auto" uses the
 * fillers that mean nothing in any of the covered languages. The desktop
 * shares the lists and the arithmetic (`crates/editor/src/fillers.rs`).
 */

export interface TimedWordLike {
	text: string;
	/** Seconds on a common clock. */
	start: number;
	end: number;
}

export interface FillerHit extends TimedWordLike {
	/** The normalised form that matched. */
	filler: string;
}

export interface FillerInterval {
	start: number;
	end: number;
}

export interface FillerOptions {
	/** Seconds taken on each side of a filler, held back from the neighbouring words. */
	paddingSeconds: number;
	/** A "filler" longer than this is a word the list happened to match, not a hesitation. */
	maxSeconds: number;
}

export const FILLER_DEFAULTS: FillerOptions = { paddingSeconds: 0.04, maxSeconds: 1.5 };

/** Normalised fillers (lower case, letter runs collapsed) per language. */
export const FILLER_WORDS: Record<string, readonly string[]> = {
	en: ["um", "uh", "er", "erm", "ah", "hm", "mhm", "mm", "uhm", "ehm"],
	de: ["äh", "ähm", "hm", "öhm", "mh", "ehm"],
	fr: ["euh", "heu", "hum", "hm", "ben", "bah"],
	es: ["eh", "em", "este", "mm", "hm", "ehm"],
	pt: ["hum", "hã", "ahn", "ã", "hm", "eh"],
	it: ["ehm", "mh", "hm", "eh", "uhm"],
};

/** Fillers safe in every covered language: those that are no real word in any of them. */
const AUTO_EXCLUDED = new Set(["ben", "bah", "este", "em"]);

export const FILLER_LANGUAGES: { id: string; label: string }[] = [
	{ id: "auto", label: "Any language" },
	{ id: "en", label: "English" },
	{ id: "de", label: "German" },
	{ id: "fr", label: "French" },
	{ id: "es", label: "Spanish" },
	{ id: "pt", label: "Portuguese" },
	{ id: "it", label: "Italian" },
];

/** The filler set for a language code (`auto`, or an unknown code, takes the safe union). */
export function fillerSet({ language }: { language: string }): Set<string> {
	const code = language.toLowerCase().split("-")[0];
	const list = FILLER_WORDS[code];
	if (list) return new Set(list);
	const union = new Set<string>();
	for (const words of Object.values(FILLER_WORDS)) {
		for (const word of words) if (!AUTO_EXCLUDED.has(word)) union.add(word);
	}
	return union;
}

/** Lower case, letters only, runs of a letter collapsed: "Ummm," → "um". */
export function normalizeWord({ text }: { text: string }): string {
	const letters = text
		.normalize("NFKC")
		.toLowerCase()
		.replace(/[^\p{L}]+/gu, "");
	return letters.replace(/(\p{L})\1+/gu, "$1");
}

/** The fillers among the words. */
export function findFillers({
	words,
	language,
	options = {},
}: {
	words: TimedWordLike[];
	language: string;
	options?: Partial<FillerOptions>;
}): FillerHit[] {
	const settings = { ...FILLER_DEFAULTS, ...options };
	const set = fillerSet({ language });
	const hits: FillerHit[] = [];
	for (const word of words) {
		if (word.end - word.start > settings.maxSeconds) continue;
		const filler = normalizeWord({ text: word.text });
		if (filler && set.has(filler)) hits.push({ ...word, filler });
	}
	return hits;
}

/**
 * The stretches to cut: each filler padded, clamped to the words either
 * side, merged where they touch. `words` must be the full word list the
 * fillers were found in, so neighbours are known.
 */
export function fillerIntervals({
	words,
	fillers,
	options = {},
}: {
	words: TimedWordLike[];
	fillers: FillerHit[];
	options?: Partial<FillerOptions>;
}): FillerInterval[] {
	const settings = { ...FILLER_DEFAULTS, ...options };
	const sorted = [...words].sort((a, b) => a.start - b.start);
	const fillerKeys = new Set(fillers.map((hit) => `${hit.start}|${hit.end}`));
	const raw: FillerInterval[] = [];
	for (let index = 0; index < sorted.length; index++) {
		const word = sorted[index];
		if (!fillerKeys.has(`${word.start}|${word.end}`)) continue;
		const previous = sorted[index - 1];
		const next = sorted[index + 1];
		const start = Math.max(word.start - settings.paddingSeconds, previous ? previous.end : 0);
		const end = Math.min(word.end + settings.paddingSeconds, next ? next.start : Number.POSITIVE_INFINITY);
		if (end > start) raw.push({ start, end });
	}
	raw.sort((a, b) => a.start - b.start);
	const merged: FillerInterval[] = [];
	for (const interval of raw) {
		const last = merged[merged.length - 1];
		if (last && interval.start <= last.end + 0.02) {
			last.end = Math.max(last.end, interval.end);
		} else {
			merged.push({ ...interval });
		}
	}
	return merged;
}

/** Total seconds the intervals cover. */
export function totalSeconds({ intervals }: { intervals: FillerInterval[] }): number {
	return intervals.reduce((sum, interval) => sum + (interval.end - interval.start), 0);
}
