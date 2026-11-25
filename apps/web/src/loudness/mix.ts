/**
 * Whole-mix loudness on the web, through the shared `loudness` crate
 * (WASM), exactly as the desktop does it: an export's rendered mix is
 * brought to a target with true peaks held under a ceiling by a
 * look-ahead limiter (`masterAudioBuffer`), and the preview's mix is
 * metered as it plays (`MixMeter`).
 */

import { LoudnessMeter, masterMix } from "opencut-wasm";
import { LOUDNESS_TARGETS } from "./r128";

/** A whole-mix loudness target for export. */
export interface MixLoudness {
	targetLufs: number;
	ceilingDbtp: number;
}

/** What mastering did; levels are `-Infinity` for silence. */
export interface MixMasterReport {
	inputLufs: number;
	inputTruePeakDbtp: number;
	gainDb: number;
	outputLufs: number;
	outputTruePeakDbtp: number;
	limited: boolean;
	passes: number;
}

/** The delivery target with this id (`streaming`, `podcast`, `broadcast`, `loud`). */
export function mixLoudnessTarget(id: string): MixLoudness | undefined {
	const target = LOUDNESS_TARGETS.find((candidate) => candidate.id === id);
	return target ? { targetLufs: target.lufs, ceilingDbtp: target.truePeakDbtp } : undefined;
}

/**
 * Masters an audio buffer's first two channels in place (a mono buffer as
 * mono) and reports what was done. Silence is left alone.
 */
export function masterAudioBuffer(buffer: AudioBuffer, loudness: MixLoudness): MixMasterReport {
	const left = buffer.getChannelData(0).slice();
	const right = buffer.numberOfChannels > 1 ? buffer.getChannelData(1).slice() : new Float32Array(0);
	const [inputLufs, inputTruePeakDbtp, gainDb, outputLufs, outputTruePeakDbtp, limited, passes] = masterMix(
		left,
		right,
		buffer.sampleRate,
		loudness.targetLufs,
		loudness.ceilingDbtp,
	);
	buffer.copyToChannel(left, 0);
	if (right.length > 0) buffer.copyToChannel(right, 1);
	return { inputLufs, inputTruePeakDbtp, gainDb, outputLufs, outputTruePeakDbtp, limited: limited === 1, passes };
}

/** What the meter reads; `-Infinity` before anything was heard. */
export interface MeterReadings {
	momentaryLufs: number;
	shortTermLufs: number;
	peakDbfs: number;
}

/** A live meter fed with blocks of the playing mix. */
export class MixMeter {
	private meter: LoudnessMeter;

	constructor({ sampleRate, channels }: { sampleRate: number; channels: number }) {
		this.meter = new LoudnessMeter(sampleRate, Math.max(1, Math.min(2, channels)));
	}

	push(left: Float32Array, right?: Float32Array): void {
		this.meter.pushPlanar(left, right ?? new Float32Array(0));
	}

	readings(): MeterReadings {
		const [momentaryLufs, shortTermLufs, peakDbfs] = this.meter.readings();
		return { momentaryLufs, shortTermLufs, peakDbfs };
	}

	reset(): void {
		this.meter.reset();
	}

	dispose(): void {
		this.meter.free();
	}
}

/** A loudness value for a meter: one decimal, or a dash below the -70 LUFS gate. */
export function formatMeterLufs(value: number): string {
	return Number.isFinite(value) && value > -70 ? value.toFixed(1) : "–";
}

/** Where a peak sits on a meter bar over -60..0 dBFS, 0 to 1. */
export function meterFraction(db: number): number {
	return Number.isFinite(db) ? Math.min(1, Math.max(0, (db + 60) / 60)) : 0;
}
