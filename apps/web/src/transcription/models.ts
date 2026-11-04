import type {
	TranscriptionModel,
	TranscriptionModelId,
} from "./types";

/**
 * The `_timestamped` exports carry the cross-attention alignment heads that
 * word-level timestamps need; the plain exports do not.
 */
export const TRANSCRIPTION_MODELS: TranscriptionModel[] = [
	{
		id: "whisper-tiny",
		name: "Tiny",
		huggingFaceId: "onnx-community/whisper-tiny_timestamped",
		description: "Fastest, lower accuracy",
	},
	{
		id: "whisper-base",
		name: "Base",
		huggingFaceId: "onnx-community/whisper-base_timestamped",
		description: "Fast, fair accuracy",
	},
	{
		id: "whisper-small",
		name: "Small",
		huggingFaceId: "onnx-community/whisper-small_timestamped",
		description: "Good balance of speed and accuracy",
	},
	{
		id: "whisper-medium",
		name: "Medium",
		huggingFaceId: "onnx-community/whisper-medium_timestamped",
		description: "Higher accuracy, slower",
	},
	{
		id: "whisper-large-v3-turbo",
		name: "Large v3 Turbo",
		huggingFaceId: "onnx-community/whisper-large-v3-turbo_timestamped",
		description: "Best accuracy, requires WebGPU for good performance",
	},
];

export const DEFAULT_TRANSCRIPTION_MODEL: TranscriptionModelId =
	"whisper-small";

export function isTranscriptionModelId(
	value: unknown,
): value is TranscriptionModelId {
	return TRANSCRIPTION_MODELS.some((model) => model.id === value);
}
