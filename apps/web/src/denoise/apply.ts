/**
 * Reducing noise in a clip: its media is decoded, denoised, written as a
 * WAV media asset, and the clip pointed at it. A video clip has its audio
 * separated first (as the Extract audio action does) and the separated
 * element pointed at the denoised file, so the picture is untouched.
 */

import { Command, type CommandResult } from "@/commands/base-command";
import { ToggleSourceAudioSeparationCommand } from "@/commands/timeline/element/toggle-source-audio-separation";
import { EditorCore } from "@/core";
import { createAudioContext } from "@/media/audio";
import { processMediaAssets } from "@/media/processing";
import { isSourceAudioSeparated } from "@/timeline/audio-separation";
import type { SceneTracks, TimelineElement, TimelineTrack } from "@/timeline/types";
import { DENOISE_DEFAULTS, denoiseChannels, encodeWav, type DenoiseOptions } from "./spectral";

export interface DenoiseResult {
	/** The denoised WAV asset. */
	assetId: string;
	assetName: string;
	/** The element now playing the denoised audio (the separated audio element for a video clip). */
	elementId: string;
	trackId: string;
	seconds: number;
	channels: number;
}

function allTracks(tracks: SceneTracks): TimelineTrack[] {
	return [...tracks.overlay, tracks.main, ...tracks.audio];
}

/**
 * Points the clip at the denoised asset, separating a video clip's audio
 * first; one undo step. `getTarget()` names the element that was changed.
 */
export class ReplaceClipAudioCommand extends Command {
	private savedState: SceneTracks | null = null;
	private target: { trackId: string; elementId: string } | null = null;

	constructor(private readonly params: { trackId: string; elementId: string; mediaId: string }) {
		super();
	}

	getTarget(): { trackId: string; elementId: string } | null {
		return this.target;
	}

	execute(): CommandResult | undefined {
		const editor = EditorCore.getInstance();
		this.savedState = editor.scenes.getActiveScene().tracks;
		const before = new Set(allTracks(this.savedState).flatMap((track) => (track.elements as TimelineElement[]).map((e) => e.id)));
		const track = allTracks(this.savedState).find((candidate) => candidate.id === this.params.trackId);
		const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === this.params.elementId);
		if (!track || !element) return undefined;

		let target: { trackId: string; elementId: string } | null = null;
		if (element.type === "audio") {
			target = { trackId: track.id, elementId: element.id };
		} else if (element.type === "video") {
			if (!isSourceAudioSeparated({ element })) {
				new ToggleSourceAudioSeparationCommand({ trackId: track.id, elementId: element.id }).execute();
			}
			const after = editor.scenes.getActiveScene().tracks;
			for (const candidate of after.audio) {
				for (const audio of candidate.elements) {
					const fresh = !before.has(audio.id) || (audio.sourceType === "upload" && audio.mediaId === element.mediaId && audio.startTime === element.startTime);
					if (audio.sourceType === "upload" && fresh && audio.mediaId === element.mediaId) {
						target = { trackId: candidate.id, elementId: audio.id };
					}
				}
			}
		}
		if (!target) return undefined;
		const tracks = editor.scenes.getActiveScene().tracks;
		const update = (candidate: TimelineTrack): TimelineTrack =>
			candidate.id !== target?.trackId
				? candidate
				: ({
						...candidate,
						elements: (candidate.elements as TimelineElement[]).map((e) =>
							e.id === target?.elementId && e.type === "audio" && e.sourceType === "upload" ? { ...e, mediaId: this.params.mediaId } : e,
						),
					} as TimelineTrack);
		editor.timeline.updateTracks({
			overlay: tracks.overlay.map(update) as SceneTracks["overlay"],
			main: update(tracks.main) as SceneTracks["main"],
			audio: tracks.audio.map(update) as SceneTracks["audio"],
		});
		this.target = target;
		return undefined;
	}

	undo(): void {
		if (!this.savedState) return;
		EditorCore.getInstance().timeline.updateTracks(this.savedState);
	}
}

export async function reduceNoiseOnElement({
	editor,
	trackId,
	elementId,
	options = {},
	onProgress,
}: {
	editor: EditorCore;
	trackId: string;
	elementId: string;
	options?: Partial<DenoiseOptions>;
	onProgress?: (step: "decoding" | "denoising" | "saving") => void;
}): Promise<DenoiseResult> {
	const settings = { ...DENOISE_DEFAULTS, ...options };
	const project = editor.project.getActive();
	const tracks = editor.scenes.getActiveScene().tracks;
	const track = allTracks(tracks).find((candidate) => candidate.id === trackId);
	const element = (track?.elements as TimelineElement[] | undefined)?.find((candidate) => candidate.id === elementId);
	if (!track || !element || !(element.type === "video" || (element.type === "audio" && element.sourceType === "upload"))) {
		throw new Error("Noise reduction needs an audio or video clip");
	}
	const asset = editor.media.getAssets().find((candidate) => candidate.id === element.mediaId);
	if (!asset?.file) {
		throw new Error("The clip's media file is not available");
	}
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
	onProgress?.("denoising");
	const channels: Float32Array[] = [];
	for (let channel = 0; channel < decoded.numberOfChannels; channel++) {
		channels.push(decoded.getChannelData(channel));
	}
	const cleaned = denoiseChannels({ channels, options: settings });
	onProgress?.("saving");
	const baseName = asset.name.replace(/\.[a-z0-9]+$/i, "");
	const file = new File([encodeWav({ channels: cleaned, sampleRate: decoded.sampleRate })], `${baseName} (denoised ${settings.reductionDb} dB).wav`, {
		type: "audio/wav",
	});
	const [processed] = await processMediaAssets({ files: [file] });
	if (!processed) {
		throw new Error("The denoised audio could not be prepared");
	}
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, origin: { kind: "generated", tool: "denoise", from: asset.id } },
	});
	if (!saved) {
		throw new Error("The denoised audio could not be stored");
	}
	const command = new ReplaceClipAudioCommand({ trackId, elementId, mediaId: saved.id });
	editor.command.execute({ command });
	const target = command.getTarget();
	if (!target) {
		throw new Error("The clip could not be pointed at the denoised audio");
	}
	return {
		assetId: saved.id,
		assetName: saved.name,
		elementId: target.elementId,
		trackId: target.trackId,
		seconds: decoded.duration,
		channels: decoded.numberOfChannels,
	};
}
