/**
 * Languages captions can be translated between, and the Opus-MT models
 * (Helsinki-NLP, ONNX conversions by Xenova on Hugging Face, fetched by the
 * browser on first use) that get from one to another: a direct pair where
 * one exists, else through English. The list is the pairs that exist as
 * `Xenova/opus-mt-*` (checked 2026-11-12); see ATTRIBUTION.md for licences.
 */

/** Translated both to and from English. */
const BOTH = ["zh", "de", "fr", "es", "it", "ru", "nl", "cs", "da", "fi", "hu", "id", "sv", "uk", "vi", "ar", "hi", "af"];
/** Only into English (no English-to-X model). */
const TO_ENGLISH_ONLY = ["ja", "ko", "pl", "th", "et", "tr"];
/** Only from English. */
const FROM_ENGLISH_ONLY = ["ro"];
/** Pairs with their own model, skipping English. */
const DIRECT = new Set([
	"de-es",
	"de-fr",
	"es-de",
	"es-fr",
	"es-it",
	"es-ru",
	"fr-de",
	"fr-es",
	"fr-ro",
	"fr-ru",
	"it-es",
	"it-fr",
	"ru-es",
	"ru-fr",
	"ru-uk",
	"uk-ru",
	"nl-fr",
	"fi-de",
	"da-de",
]);

export const TRANSLATION_LANGUAGE_CODES: string[] = ["en", ...BOTH, ...TO_ENGLISH_ONLY, ...FROM_ENGLISH_ONLY];

function hasEnglishTo(code: string): boolean {
	return BOTH.includes(code) || FROM_ENGLISH_ONLY.includes(code);
}

function hasToEnglish(code: string): boolean {
	return BOTH.includes(code) || TO_ENGLISH_ONLY.includes(code);
}

export function opusModelId({ from, to }: { from: string; to: string }): string {
	return from === "tr" && to === "en" ? "Xenova/opus-mt-tc-big-tr-en" : `Xenova/opus-mt-${from}-${to}`;
}

/** The models that translate `from` into `to`, in order (empty when they match); null when no route exists. */
export function opusRoute({ from, to }: { from: string; to: string }): string[] | null {
	if (from === to) return [];
	if (DIRECT.has(`${from}-${to}`)) return [opusModelId({ from, to })];
	if (from === "en") return hasEnglishTo(to) ? [opusModelId({ from, to })] : null;
	if (to === "en") return hasToEnglish(from) ? [opusModelId({ from, to })] : null;
	if (hasToEnglish(from) && hasEnglishTo(to)) {
		return [opusModelId({ from, to: "en" }), opusModelId({ from: "en", to })];
	}
	return null;
}

/** A language's name in the interface language, e.g. "German" or "德语". */
export function languageName({ code, locale }: { code: string; locale: string }): string {
	try {
		return new Intl.DisplayNames([locale], { type: "language" }).of(code) ?? code;
	} catch {
		return code;
	}
}
