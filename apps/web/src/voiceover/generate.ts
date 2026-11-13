/**
 * Voice-over: the script is split into lines (`./script`), each line is
 * spoken by Kokoro in a worker, the lines are joined with their pauses into
 * one WAV that is added to the project and placed on an audio track at the
 * playhead, and (optionally) the lines become captions timed to the audio.
 */

import type { EditorCore } from "@/core";
import { encodeWav } from "@/denoise/spectral";
import { processMediaAssets } from "@/media/processing";
import type { TtsWorkerMessage, TtsWorkerResponse } from "@/services/tts/worker";
import { insertCaptionChunksAsTextTrack } from "@/subtitles/insert";
import { buildElementFromMedia } from "@/timeline/element-utils";
import { mediaTimeFromSeconds, mediaTimeToSeconds } from "@/wasm";
import { layoutLines, splitScript } from "./script";

/** Kokoro's voices: American (a) and British (b) English, female (f) and male (m). */
export const KOKORO_VOICES = [
	"af_heart",
	"af_bella",
	"af_nicole",
	"af_sarah",
	"af_sky",
	"am_michael",
	"am_fenrir",
	"am_puck",
	"am_adam",
	"bf_emma",
	"bf_isabella",
	"bf_alice",
	"bm_george",
	"bm_fable",
	"bm_lewis",
	"bm_daniel",
] as const;
export const DEFAULT_KOKORO_VOICE = "af_heart";

export type VoiceoverProgress =
	| { step: "loading"; fraction: number }
	| { step: "speaking"; done: number; total: number }
	| { step: "saving" };

export interface VoiceoverResult {
	mediaId: string;
	elementId: string | null;
	captionTrackId: string | null;
	lines: number;
	seconds: number;
}

let worker: Worker | null = null;
let nextId = 1;

function speak({
	lines,
	voice,
	speed,
	onProgress,
}: {
	lines: string[];
	voice: string;
	speed: number;
	onProgress?: (progress: VoiceoverProgress) => void;
}): Promise<{ audio: Float32Array[]; sampleRate: number }> {
	worker ??= new Worker(new URL("../services/tts/worker.ts", import.meta.url), { type: "module" });
	const target = worker;
	const id = nextId++;
	const audio: Float32Array[] = new Array(lines.length);
	let sampleRate = 24_000;
	return new Promise((resolve, reject) => {
		const listen = (event: MessageEvent<TtsWorkerResponse>) => {
			const response = event.data;
			if (response.id !== id) return;
			switch (response.type) {
				case "load-progress":
					onProgress?.({ step: "loading", fraction: response.progress });
					break;
				case "line":
					audio[response.index] = response.audio;
					sampleRate = response.sampleRate;
					onProgress?.({ step: "speaking", done: response.index + 1, total: lines.length });
					break;
				case "complete":
					target.removeEventListener("message", listen);
					resolve({ audio, sampleRate });
					break;
				case "error":
					target.removeEventListener("message", listen);
					reject(new Error(response.error));
					break;
			}
		};
		target.addEventListener("message", listen);
		target.postMessage({ type: "speak", id, lines, voice, speed } satisfies TtsWorkerMessage);
	});
}

/** The lines' audio joined with their pauses into one mono track. */
export function joinLines({
	audio,
	pauses,
	sampleRate,
}: {
	audio: Float32Array[];
	pauses: number[];
	sampleRate: number;
}): Float32Array {
	const gaps = pauses.map((pause) => Math.round(pause * sampleRate));
	const length = audio.reduce((sum, clip, index) => sum + clip.length + (gaps[index] ?? 0), 0);
	const out = new Float32Array(length);
	let cursor = 0;
	audio.forEach((clip, index) => {
		out.set(clip, cursor);
		cursor += clip.length + (gaps[index] ?? 0);
	});
	return out;
}

export async function generateVoiceover({
	editor,
	script,
	voice = DEFAULT_KOKORO_VOICE,
	speed = 1,
	captions = true,
	at,
	onProgress,
}: {
	editor: EditorCore;
	script: string;
	voice?: string;
	speed?: number;
	captions?: boolean;
	/** Timeline seconds to start at; the playhead by default. */
	at?: number;
	onProgress?: (progress: VoiceoverProgress) => void;
}): Promise<VoiceoverResult> {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const lines = splitScript({ script });
	if (lines.length === 0) throw new Error("The script is empty");
	const { audio, sampleRate } = await speak({ lines: lines.map((line) => line.text), voice, speed, onProgress });
	onProgress?.({ step: "saving" });
	const durations = audio.map((clip) => clip.length / sampleRate);
	const { spans, total } = layoutLines({ lines, durations });
	const joined = joinLines({ audio, pauses: lines.map((line) => line.pauseAfter), sampleRate });
	const first = lines[0].text.slice(0, 32).replace(/[\\/:*?"<>|]/g, "");
	const file = new File([encodeWav({ channels: [joined], sampleRate })], `Voice-over (${voice}) ${first}.wav`, {
		type: "audio/wav",
	});
	const [processed] = await processMediaAssets({ files: [file] });
	if (!processed) throw new Error("The voice-over could not be prepared");
	const saved = await editor.media.addMediaAsset({ projectId: project.metadata.id, asset: processed });
	if (!saved) throw new Error("The voice-over could not be stored");

	const startSeconds = at ?? mediaTimeToSeconds({ time: editor.playback.getCurrentTime() });
	const startTime = mediaTimeFromSeconds({ seconds: startSeconds });
	const element = buildElementFromMedia({
		mediaId: saved.id,
		mediaType: "audio",
		name: saved.name,
		duration: mediaTimeFromSeconds({ seconds: total }),
		startTime,
		buffer: new AudioBuffer({ length: 1, sampleRate: 44_100 }),
	});
	const before = new Set(allElementIds({ editor }));
	editor.timeline.insertElement({ element, placement: { mode: "auto", trackType: "audio" } });
	const elementId = allElementIds({ editor }).find((id) => !before.has(id)) ?? null;

	const captionTrackId = captions
		? insertCaptionChunksAsTextTrack({
				editor,
				captions: lines.map((line, index) => ({
					text: line.text,
					startTime: startSeconds + spans[index][0],
					duration: Math.max(0.2, spans[index][1] - spans[index][0] + line.pauseAfter),
				})),
				captionStyle: { highlight: "none" },
			})
		: null;
	return { mediaId: saved.id, elementId, captionTrackId, lines: lines.length, seconds: total };
}

function allElementIds({ editor }: { editor: EditorCore }): string[] {
	const { tracks } = editor.scenes.getActiveScene();
	return [...tracks.overlay, tracks.main, ...tracks.audio].flatMap((track) => track.elements.map((element) => element.id));
}
