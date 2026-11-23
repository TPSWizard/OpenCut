/**
 * Audio effects on a clip (row 72): a three-band equaliser, a compressor
 * and a limiter, kept as `audio.*` params and applied when the clip's audio
 * is prepared, for playback and export alike, by the shared Rust code
 * (`voice::apply_audio_fx` through `opencut-wasm`'s `applyAudioFx`). The
 * desktop runs the same code when it prepares a clip's audio.
 */

import { applyAudioFx, audioFxPresets } from "opencut-wasm";

export interface AudioFx {
	low: number;
	mid: number;
	high: number;
	/** `[threshold dB, ratio]` when the compressor is on. */
	compressor: [number, number] | null;
	/** Ceiling dB when the limiter is on. */
	limiter: number | null;
}

export const EQ_RANGE_DB = 12;
export const DEFAULT_THRESHOLD_DB = -24;
export const DEFAULT_RATIO = 4;
export const DEFAULT_CEILING_DB = -1;

export const AUDIO_FX_PARAMS = [
	"audio.eq.low",
	"audio.eq.mid",
	"audio.eq.high",
	"audio.comp.enabled",
	"audio.comp.threshold",
	"audio.comp.ratio",
	"audio.limit.enabled",
	"audio.limit.ceiling",
] as const;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** A clip's effects from its params (the same reading as `voice::AudioFx::from_params`). */
export function audioFxOf(params: Record<string, unknown> | undefined): AudioFx {
	const read = (key: string) => {
		const value = params?.[key];
		return typeof value === "number" && Number.isFinite(value) ? value : undefined;
	};
	const band = (key: string) => clamp(read(key) ?? 0, -EQ_RANGE_DB, EQ_RANGE_DB);
	return {
		low: band("audio.eq.low"),
		mid: band("audio.eq.mid"),
		high: band("audio.eq.high"),
		compressor:
			params?.["audio.comp.enabled"] === true
				? [clamp(read("audio.comp.threshold") ?? DEFAULT_THRESHOLD_DB, -60, 0), clamp(read("audio.comp.ratio") ?? DEFAULT_RATIO, 1, 20)]
				: null,
		limiter: params?.["audio.limit.enabled"] === true ? clamp(read("audio.limit.ceiling") ?? DEFAULT_CEILING_DB, -12, 0) : null,
	};
}

export function isIdentityFx(fx: AudioFx): boolean {
	return fx.low === 0 && fx.mid === 0 && fx.high === 0 && !fx.compressor && !fx.limiter;
}

/** A stable text of the settings, for cache keys. */
export function fxKey(fx: AudioFx): string {
	return isIdentityFx(fx) ? "" : JSON.stringify(fx);
}

/** The effects as params, to write all of them at once (a preset). */
export function fxParams(fx: AudioFx): Record<string, number | boolean> {
	return {
		"audio.eq.low": fx.low,
		"audio.eq.mid": fx.mid,
		"audio.eq.high": fx.high,
		"audio.comp.enabled": fx.compressor !== null,
		"audio.comp.threshold": fx.compressor?.[0] ?? DEFAULT_THRESHOLD_DB,
		"audio.comp.ratio": fx.compressor?.[1] ?? DEFAULT_RATIO,
		"audio.limit.enabled": fx.limiter !== null,
		"audio.limit.ceiling": fx.limiter ?? DEFAULT_CEILING_DB,
	};
}

export interface AudioFxPreset {
	id: string;
	label: string;
	fx: AudioFx;
}

let presets: AudioFxPreset[] | null = null;

/** The presets, from the shared Rust list. */
export function audioFxPresetList(): AudioFxPreset[] {
	if (!presets) {
		const rows = JSON.parse(audioFxPresets()) as {
			id: string;
			label: string;
			low: number;
			mid: number;
			high: number;
			threshold: number | null;
			ratio: number | null;
			ceiling: number | null;
		}[];
		presets = rows.map((row) => ({
			id: row.id,
			label: row.label,
			fx: {
				low: row.low,
				mid: row.mid,
				high: row.high,
				compressor: row.threshold !== null && row.ratio !== null ? [row.threshold, row.ratio] : null,
				limiter: row.ceiling,
			},
		}));
	}
	return presets;
}

/** The preset the effects match, if any. */
export function matchingPreset(fx: AudioFx): string | null {
	return audioFxPresetList().find((preset) => JSON.stringify(preset.fx) === JSON.stringify(fx))?.id ?? null;
}

/** Interleaved samples through the effects (a new array). */
export function applyFxInterleaved({
	samples,
	channels,
	sampleRate,
	fx,
}: {
	samples: Float32Array;
	channels: number;
	sampleRate: number;
	fx: AudioFx;
}): Float32Array {
	if (isIdentityFx(fx)) return samples;
	return applyAudioFx(
		samples,
		channels,
		sampleRate,
		fx.low,
		fx.mid,
		fx.high,
		fx.compressor?.[0] ?? Number.NaN,
		fx.compressor?.[1] ?? Number.NaN,
		fx.limiter ?? Number.NaN,
	);
}

/** An audio buffer through the effects (a new buffer; the same one when there are none). */
export function applyFxToBuffer({ buffer, fx }: { buffer: AudioBuffer; fx: AudioFx }): AudioBuffer {
	if (isIdentityFx(fx) || buffer.length === 0) return buffer;
	const channels = buffer.numberOfChannels;
	const interleaved = new Float32Array(buffer.length * channels);
	for (let c = 0; c < channels; c++) {
		const data = buffer.getChannelData(c);
		for (let i = 0; i < buffer.length; i++) interleaved[i * channels + c] = data[i];
	}
	const processed = applyFxInterleaved({ samples: interleaved, channels, sampleRate: buffer.sampleRate, fx });
	const out = new AudioBuffer({ numberOfChannels: channels, length: buffer.length, sampleRate: buffer.sampleRate });
	for (let c = 0; c < channels; c++) {
		const data = out.getChannelData(c);
		for (let i = 0; i < buffer.length; i++) data[i] = processed[i * channels + c];
	}
	return out;
}
