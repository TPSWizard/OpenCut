/**
 * Kokoro text-to-speech in a worker (kokoro-js on Transformers.js; the model
 * and voices come from Hugging Face on first use). kokoro-js phonemises with
 * eSpeak NG, which is GPL-3.0; see ATTRIBUTION.md.
 */

import { KokoroTTS } from "kokoro-js";

export const KOKORO_MODEL = "onnx-community/Kokoro-82M-v1.0-ONNX";

export type TtsWorkerMessage = { type: "speak"; id: number; lines: string[]; voice: string; speed: number };

export type TtsWorkerResponse =
	| { type: "load-progress"; id: number; progress: number }
	| { type: "line"; id: number; index: number; audio: Float32Array; sampleRate: number }
	| { type: "complete"; id: number }
	| { type: "error"; id: number; error: string };

let model: Promise<KokoroTTS> | null = null;

function load(id: number): Promise<KokoroTTS> {
	if (!model) {
		const files = new Map<string, { loaded: number; total: number }>();
		model = KokoroTTS.from_pretrained(KOKORO_MODEL, {
			dtype: "q8",
			device: "wasm",
			progress_callback: (info: { status?: string; file?: string; loaded?: number; total?: number }) => {
				if (info.status !== "progress" || !info.file || !info.total) return;
				files.set(info.file, { loaded: info.loaded ?? 0, total: info.total });
				let loaded = 0;
				let total = 0;
				for (const file of files.values()) {
					loaded += file.loaded;
					total += file.total;
				}
				self.postMessage({ type: "load-progress", id, progress: total > 0 ? loaded / total : 0 } satisfies TtsWorkerResponse);
			},
		} as Parameters<typeof KokoroTTS.from_pretrained>[1]).catch((error) => {
			model = null;
			throw error;
		});
	}
	return model;
}

self.onmessage = async (event: MessageEvent<TtsWorkerMessage>) => {
	const message = event.data;
	if (message.type !== "speak") return;
	const { id, lines, voice, speed } = message;
	try {
		const tts = await load(id);
		for (let index = 0; index < lines.length; index++) {
			const raw = await tts.generate(lines[index], {
				voice: voice as Parameters<KokoroTTS["generate"]>[1] extends infer O ? O extends { voice?: infer V } ? V : never : never,
				speed,
			});
			const audio = new Float32Array(raw.audio);
			self.postMessage(
				{ type: "line", id, index, audio, sampleRate: raw.sampling_rate } satisfies TtsWorkerResponse,
				{ transfer: [audio.buffer] },
			);
		}
		self.postMessage({ type: "complete", id } satisfies TtsWorkerResponse);
	} catch (error) {
		self.postMessage({ type: "error", id, error: error instanceof Error ? error.message : String(error) } satisfies TtsWorkerResponse);
	}
};
