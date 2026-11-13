import { describe, expect, test } from "vitest";
import type { TextElement } from "@/timeline/types";
import type { MediaTime } from "@/wasm";
import { translatedCopy } from "../captions";
import { captionSentence, tidyTranslation } from "../engine";
import { TRANSLATION_LANGUAGE_CODES, opusRoute } from "../languages";

describe("caption translation", () => {
	test("routes go direct, through English, or nowhere", () => {
		expect(opusRoute({ from: "en", to: "zh" })).toEqual(["Xenova/opus-mt-en-zh"]);
		expect(opusRoute({ from: "zh", to: "en" })).toEqual(["Xenova/opus-mt-zh-en"]);
		expect(opusRoute({ from: "de", to: "fr" })).toEqual(["Xenova/opus-mt-de-fr"]);
		expect(opusRoute({ from: "de", to: "zh" })).toEqual(["Xenova/opus-mt-de-en", "Xenova/opus-mt-en-zh"]);
		expect(opusRoute({ from: "tr", to: "en" })).toEqual(["Xenova/opus-mt-tc-big-tr-en"]);
		expect(opusRoute({ from: "ja", to: "de" })).toEqual(["Xenova/opus-mt-ja-en", "Xenova/opus-mt-en-de"]);
		expect(opusRoute({ from: "en", to: "ja" })).toBeNull();
		expect(opusRoute({ from: "ro", to: "en" })).toBeNull();
		expect(opusRoute({ from: "fr", to: "fr" })).toEqual([]);
		expect(new Set(TRANSLATION_LANGUAGE_CODES).size).toBe(TRANSLATION_LANGUAGE_CODES.length);
	});

	test("model loops are trimmed", () => {
		expect(tidyTranslation({ text: "今天我们做饭饭饭" })).toBe("今天我们做饭");
		expect(tidyTranslation({ text: "谢谢你看!!!!!" })).toBe("谢谢你看!");
		expect(tidyTranslation({ text: "Willkommen in der Küche." })).toBe("Willkommen in der Küche.");
		expect(tidyTranslation({ text: "Kaffee" })).toBe("Kaffee");
	});

	test("a caption is translated as one sentence", () => {
		expect(captionSentence({ text: "Today we cook rice\n with chicken " })).toBe("Today we cook rice with chicken");
		expect(captionSentence({ text: "One" })).toBe("One");
	});

	test("copies keep timing and look but drop word timings", () => {
		const element = {
			id: "c1",
			name: "Caption 1",
			type: "text",
			startTime: 120_000 as MediaTime,
			duration: 240_000 as MediaTime,
			trimStart: 0 as MediaTime,
			trimEnd: 0 as MediaTime,
			params: { content: "Hello there", fontSize: 48, "caption.highlight": "word" },
			words: [{ text: "Hello", start: 0 as MediaTime, end: 60_000 as MediaTime }],
		} as unknown as TextElement;
		const copy = translatedCopy({ element, text: "你好" }) as unknown as Record<string, unknown>;
		expect(copy.id).toBeUndefined();
		expect(copy.words).toBeUndefined();
		expect(copy.startTime).toBe(120_000);
		expect(copy.params).toMatchObject({ content: "你好", fontSize: 48, "caption.highlight": "none" });
		expect(element.params.content).toBe("Hello there");
	});
});
