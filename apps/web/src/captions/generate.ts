import type { EditorCore } from "@/core";
import { decodeAudioToFloat32 } from "@/media/audio";
import { extractTimelineAudio } from "@/media/mediabunny";
import { transcriptionService } from "@/services/transcription/service";
import { insertCaptionChunksAsTextTrack } from "@/subtitles/insert";
import { DEFAULT_TRANSCRIPTION_SAMPLE_RATE } from "@/transcription/audio";
import { buildCaptionChunks } from "@/transcription/caption";
import { DEFAULT_WORDS_PER_CAPTION } from "@/transcription/caption-defaults";
import { DEFAULT_TRANSCRIPTION_MODEL } from "@/transcription/models";
import type {
	CaptionChunk,
	TranscriptionLanguage,
	TranscriptionModelId,
	TranscriptionProgress,
	TranscriptionResult,
} from "@/transcription/types";
import { GENERATED_CAPTION_HIGHLIGHT } from "./params";
import type { CaptionStyle } from "./types";

export type GenerateCaptionsStep =
	| { step: "extracting-audio" }
	| { step: "preparing-audio" }
	| { step: "loading-model"; progress: number }
	| { step: "transcribing" }
	| { step: "building-captions" };

/**
 * Transcribes the active scene's audio in the browser and cuts the result
 * into caption chunks, with word timings when the model gives them. The
 * caller decides whether to insert them (see `insertGeneratedCaptions`).
 */
export async function generateCaptions({
	editor,
	language = "auto",
	modelId = DEFAULT_TRANSCRIPTION_MODEL,
	wordsPerCaption = DEFAULT_WORDS_PER_CAPTION,
	onStep,
}: {
	editor: EditorCore;
	language?: TranscriptionLanguage;
	modelId?: TranscriptionModelId;
	wordsPerCaption?: number;
	onStep?: (step: GenerateCaptionsStep) => void;
}): Promise<{ result: TranscriptionResult; captions: CaptionChunk[] }> {
	onStep?.({ step: "extracting-audio" });
	const audioBlob = await extractTimelineAudio({
		tracks: editor.scenes.getActiveScene().tracks,
		mediaAssets: editor.media.getAssets(),
		totalDuration: editor.timeline.getTotalDuration(),
	});

	onStep?.({ step: "preparing-audio" });
	const { samples } = await decodeAudioToFloat32({
		audioBlob,
		sampleRate: DEFAULT_TRANSCRIPTION_SAMPLE_RATE,
	});

	const result = await transcriptionService.transcribe({
		audioData: samples,
		language: language === "auto" ? undefined : language,
		modelId,
		onProgress: (progress: TranscriptionProgress) => {
			if (progress.status === "loading-model") {
				onStep?.({ step: "loading-model", progress: progress.progress });
			} else if (progress.status === "transcribing") {
				onStep?.({ step: "transcribing" });
			}
		},
	});

	onStep?.({ step: "building-captions" });
	const captions = buildCaptionChunks({
		segments: result.segments,
		words: result.words,
		wordsPerChunk: wordsPerCaption,
	});
	return { result, captions };
}

/** Inserts caption chunks as a new text track; returns its id, or null when there is nothing to insert. */
export function insertGeneratedCaptions({
	editor,
	captions,
	style,
}: {
	editor: EditorCore;
	captions: CaptionChunk[];
	style?: Partial<CaptionStyle>;
}): string | null {
	return insertCaptionChunksAsTextTrack({
		editor,
		captions,
		captionStyle: { highlight: GENERATED_CAPTION_HIGHLIGHT, ...style },
	});
}
