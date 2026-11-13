/**
 * Translating caption texts on this device: the browser's own Translator
 * (Chrome 138+ on desktop, which manages its language packs) when it can do
 * the pair, else Opus-MT models in a worker (about 110 MB per pair, fetched
 * from Hugging Face once and cached by the browser). Each caption goes in
 * as one sentence: its line breaks were for the original wording, and
 * cutting a sentence at them makes the models translate fragments.
 */

import type { TranslationWorkerMessage, TranslationWorkerResponse } from "@/services/translation/worker";
import { opusRoute } from "./languages";

export type TranslationEngine = "browser" | "opus";
export type TranslationEnginePreference = TranslationEngine | "auto";

export type TranslationProgress =
	| { step: "downloading"; fraction: number; model: string }
	| { step: "translating"; done: number; total: number };

interface BrowserTranslator {
	translate(text: string): Promise<string>;
	destroy?: () => void;
}

interface BrowserTranslatorFactory {
	availability(options: { sourceLanguage: string; targetLanguage: string }): Promise<string>;
	create(options: {
		sourceLanguage: string;
		targetLanguage: string;
		monitor?: (monitor: EventTarget) => void;
	}): Promise<BrowserTranslator>;
}

/** Rejects when `promise` takes longer than `ms` (some browsers expose the API but never answer). */
function withTimeout<T>(promise: Promise<T>, ms: number, what: string): Promise<T> {
	return new Promise((resolve, reject) => {
		const timer = setTimeout(() => reject(new Error(`${what} did not answer`)), ms);
		promise.then(
			(value) => {
				clearTimeout(timer);
				resolve(value);
			},
			(error) => {
				clearTimeout(timer);
				reject(error);
			},
		);
	});
}

function browserTranslator(): BrowserTranslatorFactory | null {
	const factory = (globalThis as { Translator?: BrowserTranslatorFactory }).Translator;
	return factory && typeof factory.create === "function" ? factory : null;
}

/** Whether the browser's built-in translator can do this pair (may still need a download). */
export async function browserCanTranslate({ from, to }: { from: string; to: string }): Promise<boolean> {
	const factory = browserTranslator();
	if (!factory) return false;
	try {
		const availability = await withTimeout(
			factory.availability({ sourceLanguage: from, targetLanguage: to }),
			3000,
			"The browser's translator",
		);
		return availability !== "unavailable";
	} catch {
		return false;
	}
}

/**
 * Opus-MT output with its loops trimmed: a character repeated three or
 * more times in a row ("饭饭饭", "!!!!!") is kept once.
 */
export function tidyTranslation({ text }: { text: string }): string {
	return text.replace(/(.)\1{2,}/gu, "$1").trim();
}

/** A caption as one line of text to translate (line breaks become spaces). */
export function captionSentence({ text }: { text: string }): string {
	return text
		.split("\n")
		.map((line) => line.trim())
		.filter(Boolean)
		.join(" ");
}

let worker: Worker | null = null;
let nextId = 1;

function opusWorker(): Worker {
	worker ??= new Worker(new URL("../services/translation/worker.ts", import.meta.url), { type: "module" });
	return worker;
}

function runOpus({
	modelId,
	texts,
	onProgress,
}: {
	modelId: string;
	texts: string[];
	onProgress?: (progress: TranslationProgress) => void;
}): Promise<string[]> {
	const target = opusWorker();
	const id = nextId++;
	return new Promise((resolve, reject) => {
		const listen = (event: MessageEvent<TranslationWorkerResponse>) => {
			const response = event.data;
			if (response.id !== id) return;
			switch (response.type) {
				case "load-progress":
					onProgress?.({ step: "downloading", fraction: response.progress, model: modelId });
					break;
				case "progress":
					onProgress?.({ step: "translating", done: response.done, total: texts.length });
					break;
				case "complete":
					target.removeEventListener("message", listen);
					resolve(response.translations);
					break;
				case "error":
					target.removeEventListener("message", listen);
					reject(new Error(response.error));
					break;
			}
		};
		target.addEventListener("message", listen);
		target.postMessage({ type: "translate", id, modelId, texts } satisfies TranslationWorkerMessage);
	});
}

export interface TranslatedTexts {
	translations: string[];
	engine: TranslationEngine;
	/** Opus-MT models used, in order (empty for the browser's translator). */
	models: string[];
}

export async function translateTexts({
	texts,
	from,
	to,
	engine = "auto",
	onProgress,
}: {
	texts: string[];
	from: string;
	to: string;
	engine?: TranslationEnginePreference;
	onProgress?: (progress: TranslationProgress) => void;
}): Promise<TranslatedTexts> {
	if (from === to) return { translations: [...texts], engine: "opus", models: [] };
	const work = texts.map((text) => captionSentence({ text }));

	const factory = browserTranslator();
	if (engine !== "opus" && factory && (await browserCanTranslate({ from, to }))) {
		try {
			const translator = await withTimeout(
				factory.create({
					sourceLanguage: from,
					targetLanguage: to,
					monitor: (monitor) =>
						monitor.addEventListener("downloadprogress", (event) =>
							onProgress?.({ step: "downloading", fraction: (event as ProgressEvent).loaded, model: "browser" }),
						),
				}),
				120_000,
				"The browser's translator",
			);
			const out: string[] = [];
			for (const line of work) {
				out.push(line ? await withTimeout(translator.translate(line), 30_000, "The browser's translator") : "");
				onProgress?.({ step: "translating", done: out.length, total: work.length });
			}
			translator.destroy?.();
			return { translations: out, engine: "browser", models: [] };
		} catch (error) {
			if (engine === "browser") throw error;
			// Fall through to Opus-MT.
		}
	} else if (engine === "browser") {
		throw new Error("This browser has no built-in translator for these languages");
	}

	const route = opusRoute({ from, to });
	if (!route) throw new Error(`No translation model goes from "${from}" to "${to}"`);
	let current = work;
	for (const modelId of route) {
		const indices = current.map((line, index) => (line ? index : -1)).filter((index) => index >= 0);
		const translated = await runOpus({ modelId, texts: indices.map((index) => current[index]), onProgress });
		const next = current.map(() => "");
		indices.forEach((index, position) => {
			next[index] = tidyTranslation({ text: translated[position] ?? "" });
		});
		current = next;
	}
	return { translations: current, engine: "opus", models: route };
}
