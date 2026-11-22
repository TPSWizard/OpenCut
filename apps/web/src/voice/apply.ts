/**
 * Voice effects in the web editor: the clip's media is decoded, each
 * channel run through the shared Rust `voice` crate (WebAssembly, the same
 * code the desktop runs), the channels kept together under full scale,
 * and the result written as a WAV asset the clip then plays (a video clip
 * has its audio separated first), as noise reduction does.
 */

import { applyVoiceEffect } from "opencut-wasm";
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
	onProgress?: (step: "decoding" | "processing" | "saving") => void;
}): Promise<VoiceEffectResult> {
	const project = editor.project.getActive();
	const track = allTracks(editor.scenes.getActiveScene().tracks).find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!track || !element || !(element.type === "video" || (element.type === "audio" && element.sourceType === "upload"))) {
		throw new Error("Voice effects need an audio or video clip");
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
	const changed = voiceChannels({ channels, sampleRate: decoded.sampleRate, effect });
	onProgress?.("saving");
	const label = VOICE_EFFECTS.find((candidate) => candidate.id === effect)?.label ?? effect;
	const baseName = asset.name.replace(/\.[a-z0-9]+$/i, "");
	const file = new File([encodeWav({ channels: changed, sampleRate: decoded.sampleRate })], `${baseName} (${label.toLowerCase()} voice).wav`, {
		type: "audio/wav",
	});
	const [processed] = await processMediaAssets({ files: [file] });
	if (!processed) throw new Error("The changed voice could not be prepared");
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, origin: { kind: "generated", tool: "voice", from: asset.id } },
	});
	if (!saved) throw new Error("The changed voice could not be stored");
	const command = new ReplaceClipAudioCommand({ trackId, elementId, mediaId: saved.id });
	editor.command.execute({ command });
	const target = command.getTarget();
	if (!target) throw new Error("The clip could not be pointed at the changed voice");
	return { assetId: saved.id, assetName: saved.name, elementId: target.elementId, trackId: target.trackId, seconds: decoded.duration };
}
