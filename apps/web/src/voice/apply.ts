/**
 * Voice effects and enhance voice in the web editor: the clip's media is decoded, each
 * channel run through the shared Rust `voice` crate (WebAssembly, the same
 * code the desktop runs), the channels kept together under full scale,
 * and the result written as a WAV asset the clip then plays (a video clip
 * has its audio separated first), as noise reduction does.
 */

import { applyVoiceEffect, enhanceVoice } from "opencut-wasm";
import type { EditorCore } from "@/core";
import { ReplaceClipAudioCommand } from "@/denoise/apply";
import { encodeWav } from "@/denoise/spectral";
import { createAudioContext } from "@/media/audio";
import { processMediaAssets } from "@/media/processing";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";

/** `(id, label)` of every effect, in menu order; `crates/voice/src/lib.rs` has the same list. */
export const VOICE_EFFECTS = [
	{ id: "deeper", label: "Deeper" },
	{ id: "higher", label: "Higher" },
	{ id: "chipmunk", label: "Chipmunk" },
	{ id: "monster", label: "Monster" },
	{ id: "robot", label: "Robot" },
	{ id: "echo", label: "Echo" },
	{ id: "radio", label: "Radio" },
	{ id: "hall", label: "Hall" },
] as const;

export type VoiceEffectId = (typeof VOICE_EFFECTS)[number]["id"];

export function isVoiceEffect(value: unknown): value is VoiceEffectId {
	return VOICE_EFFECTS.some((effect) => effect.id === value);
}

/** Scales all channels together so the loudest sample is at most `ceiling`; returns the gain. */
export function normalizePeak({ channels, ceiling = 0.98 }: { channels: Float32Array[]; ceiling?: number }): number {
	let peak = 0;
	for (const channel of channels) for (const sample of channel) peak = Math.max(peak, Math.abs(sample));
	if (peak <= ceiling || peak === 0) return 1;
	const gain = ceiling / peak;
	for (const channel of channels) for (let i = 0; i < channel.length; i++) channel[i] *= gain;
	return gain;
}

/** The channels through the effect, normalised together. */
export function voiceChannels({ channels, sampleRate, effect }: { channels: Float32Array[]; sampleRate: number; effect: VoiceEffectId }): Float32Array[] {
	const out = channels.map((channel) => Float32Array.from(applyVoiceEffect(effect, channel, sampleRate)));
	normalizePeak({ channels: out });
	return out;
}

export interface VoiceEffectResult {
	assetId: string;
	assetName: string;
	/** The element now playing the changed voice (the separated audio element for a video clip). */
	elementId: string;
	trackId: string;
	seconds: number;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

export type ClipAudioStep = "decoding" | "processing" | "saving";

/**
 * Decodes the clip's media, runs `process` over its channels and points the
 * clip at the result as a new WAV asset (one undo step); the shared core of
 * voice effects and enhance voice.
 */
async function processClipAudio({
	editor,
	trackId,
	elementId,
	tool,
	suffix,
	process,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	tool: string;
	/** Goes in the new file's name: "<name> (<suffix>).wav". */
	suffix: string;
	process: (input: { channels: Float32Array[]; sampleRate: number }) => Float32Array[];
	onProgress?: (step: ClipAudioStep) => void;
}): Promise<VoiceEffectResult> {
	const project = editor.project.getActive();
	const track = allTracks(editor.scenes.getActiveScene().tracks).find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!track || !element || !(element.type === "video" || (element.type === "audio" && element.sourceType === "upload"))) {
		throw new Error("This needs an audio or video clip");
	}
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) throw new Error("The clip's media file is not available");
	onProgress?.("decoding");
	const context = createAudioContext();
	let decoded: AudioBuffer;
	try {
		decoded = await context.decodeAudioData(await asset.file.arrayBuffer());
	} catch {
		throw new Error("The clip's audio could not be decoded");
	} finally {
		void context.close();
	}
	onProgress?.("processing");
	// Let the step show before the (synchronous) processing starts.
	await new Promise((resolve) => setTimeout(resolve, 0));
	const channels: Float32Array[] = [];
	for (let channel = 0; channel < decoded.numberOfChannels; channel++) channels.push(decoded.getChannelData(channel));
	const changed = process({ channels, sampleRate: decoded.sampleRate });
	onProgress?.("saving");
	const baseName = asset.name.replace(/\.[a-z0-9]+$/i, "");
	const file = new File([encodeWav({ channels: changed, sampleRate: decoded.sampleRate })], `${baseName} (${suffix}).wav`, {
		type: "audio/wav",
	});
	const [processed] = await processMediaAssets({ files: [file] });
	if (!processed) throw new Error("The new audio could not be prepared");
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, origin: { kind: "generated", tool, from: asset.id } },
	});
	if (!saved) throw new Error("The new audio could not be stored");
	const command = new ReplaceClipAudioCommand({ trackId, elementId, mediaId: saved.id });
	editor.command.execute({ command });
	const target = command.getTarget();
	if (!target) throw new Error("The clip could not be pointed at the new audio");
	return { assetId: saved.id, assetName: saved.name, elementId: target.elementId, trackId: target.trackId, seconds: decoded.duration };
}

export async function applyVoiceEffectToElement({
	editor,
	trackId,
	elementId,
	effect,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	effect: VoiceEffectId;
	onProgress?: (step: ClipAudioStep) => void;
}): Promise<VoiceEffectResult> {
	const label = VOICE_EFFECTS.find((candidate) => candidate.id === effect)?.label ?? effect;
	return processClipAudio({
		editor,
		trackId,
		elementId,
		tool: "voice",
		suffix: `${label.toLowerCase()} voice`,
		process: ({ channels, sampleRate }) => voiceChannels({ channels, sampleRate, effect }),
		onProgress,
	});
}

/** The integrated loudness enhance voice brings a clip to; `ENHANCE_TARGET_LUFS` in `crates/voice`. */
export const ENHANCE_TARGET_LUFS = -16;

export interface EnhanceReport {
	/** Integrated loudness before and after, in LUFS (`null` for silence). */
	inputLufs: number | null;
	outputLufs: number | null;
	/** Whether the clip had quiet pauses the background noise was learned from and reduced. */
	noiseReduced: boolean;
}

/** The channels through enhance voice (the shared Rust chain), with its loudness report. */
export function enhanceChannels({ channels, sampleRate }: { channels: Float32Array[]; sampleRate: number }): {
	channels: Float32Array[];
	report: EnhanceReport;
} {
	const count = channels.length;
	const frames = Math.min(...channels.map((channel) => channel.length));
	const interleaved = new Float32Array(frames * count);
	for (let frame = 0; frame < frames; frame++) for (let channel = 0; channel < count; channel++) interleaved[frame * count + channel] = channels[channel][frame];
	const result = enhanceVoice(interleaved, count, sampleRate);
	try {
		const samples = result.samples;
		const out = channels.map(() => new Float32Array(frames));
		for (let frame = 0; frame < frames; frame++) for (let channel = 0; channel < count; channel++) out[channel][frame] = samples[frame * count + channel];
		const finite = (value: number) => (Number.isFinite(value) ? value : null);
		return { channels: out, report: { inputLufs: finite(result.inputLufs), outputLufs: finite(result.outputLufs), noiseReduced: result.noiseReduced } };
	} finally {
		result.free();
	}
}

/**
 * Enhance voice: high-pass at 80 Hz, noise reduction (when the clip has
 * pauses), a 250 Hz cut and 3.5 kHz presence lift, a de-esser, a
 * compressor with a pause expander and loudness to -16 LUFS under a
 * -1 dBFS limiter, as a new WAV asset the clip then plays. One undo step.
 */
export async function enhanceVoiceOfElement({
	editor,
	trackId,
	elementId,
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	onProgress?: (step: ClipAudioStep) => void;
}): Promise<VoiceEffectResult & EnhanceReport> {
	let report: EnhanceReport = { inputLufs: null, outputLufs: null, noiseReduced: false };
	const result = await processClipAudio({
		editor,
		trackId,
		elementId,
		tool: "enhance-voice",
		suffix: "enhanced voice",
		process: (input) => {
			const enhanced = enhanceChannels(input);
			if (enhanced.report.inputLufs === null) throw new Error("The clip's audio is silent; there is no voice to enhance");
			report = enhanced.report;
			return enhanced.channels;
		},
		onProgress,
	});
	return { ...result, ...report };
}
