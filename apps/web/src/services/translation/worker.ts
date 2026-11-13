import { pipeline, type TranslationPipeline } from "@huggingface/transformers";

export type TranslationWorkerMessage = { type: "translate"; id: number; modelId: string; texts: string[] };

export type TranslationWorkerResponse =
	| { type: "load-progress"; id: number; progress: number }
	| { type: "progress"; id: number; done: number }
	| { type: "complete"; id: number; translations: string[] }
	| { type: "error"; id: number; error: string };

const pipelines = new Map<string, Promise<TranslationPipeline>>();
// `pipeline`'s overloads are too wide for the checker here; this is its translation form.
const createTranslation = pipeline as unknown as (
	task: "translation",
	model: string,
	options: { dtype: string; progress_callback: (info: { status?: string; file?: string; loaded?: number; total?: number }) => void },
) => Promise<TranslationPipeline>;
const BATCH = 8;

function load(modelId: string, id: number): Promise<TranslationPipeline> {
	let loading = pipelines.get(modelId);
	if (!loading) {
		const files = new Map<string, { loaded: number; total: number }>();
		loading = createTranslation("translation", modelId, {
			dtype: "q8",
			progress_callback: (info: { status?: string; file?: string; loaded?: number; total?: number }) => {
				if (info.status !== "progress" || !info.file || !info.total) return;
				files.set(info.file, { loaded: info.loaded ?? 0, total: info.total });
				let loaded = 0;
				let total = 0;
				for (const file of files.values()) {
					loaded += file.loaded;
					total += file.total;
				}
				self.postMessage({ type: "load-progress", id, progress: total > 0 ? loaded / total : 0 } satisfies TranslationWorkerResponse);
			},
		}).catch((error) => {
			pipelines.delete(modelId);
			throw error;
		});
		pipelines.set(modelId, loading);
	}
	return loading;
}

self.onmessage = async (event: MessageEvent<TranslationWorkerMessage>) => {
	const message = event.data;
	if (message.type !== "translate") return;
	const { id, modelId, texts } = message;
	try {
		const translator = await load(modelId, id);
		const out: string[] = [];
		for (let start = 0; start < texts.length; start += BATCH) {
			const batch = texts.slice(start, start + BATCH);
			// Quantised Opus-MT models can loop on short lines; a cap tied to
			// the input's length keeps a loop short (see `tidyTranslation`).
			// Blocking repeats outright made them run on instead of stopping.
			const longest = Math.max(...batch.map((text) => text.length));
			const results = (await translator(batch, {
				max_new_tokens: Math.min(256, 2 * longest + 20),
			} as unknown as Parameters<TranslationPipeline>[1])) as Array<{ translation_text: string }>;
			out.push(...results.map((result) => result.translation_text));
			self.postMessage({ type: "progress", id, done: out.length } satisfies TranslationWorkerResponse);
		}
		self.postMessage({ type: "complete", id, translations: out } satisfies TranslationWorkerResponse);
	} catch (error) {
		self.postMessage({
			type: "error",
			id,
			error: error instanceof Error ? error.message : String(error),
		} satisfies TranslationWorkerResponse);
	}
};
