/**
 * Loudness measurement after ITU-R BS.1770-4 and EBU R128: K-weighted,
 * gated integrated loudness, loudness range (EBU Tech 3342), true peak
 * (4× oversampled) and the loudest momentary (400 ms) and short-term
 * (3 s) values. `crates/editor/src/loudness.rs` is the same arithmetic.
 *
 * The K-weighting filters are designed at the given sample rate (a
 * high shelf and a high-pass biquad, the standard's 48 kHz coefficients
 * fall out of the design at 48 kHz). Channels are weighted equally
 * (1.0, as for left, right and centre); surround weights are not applied.
 */

export interface LoudnessReport {
	/** Gated integrated loudness in LUFS; `-Infinity` when nothing passes the gate. */
	integratedLufs: number;
	/** Loudness range in LU (95th less 10th percentile of gated short-term values). */
	loudnessRangeLu: number;
	/** The highest true peak in dBTP; `-Infinity` for silence. */
	truePeakDbtp: number;
	/** The loudest 400 ms window in LUFS. */
	maxMomentaryLufs: number;
	/** The loudest 3 s window in LUFS. */
	maxShortTermLufs: number;
	/** Seconds of audio measured. */
	seconds: number;
	sampleRate: number;
	channels: number;
}

export interface LoudnessTarget {
	id: string;
	label: string;
	lufs: number;
	/** The true-peak ceiling the target is paired with, in dBTP. */
	truePeakDbtp: number;
}

/** Common delivery targets; the first is the default. */
export const LOUDNESS_TARGETS: LoudnessTarget[] = [
	{ id: "streaming", label: "Streaming (−14 LUFS, −1 dBTP)", lufs: -14, truePeakDbtp: -1 },
	{ id: "podcast", label: "Podcast (−16 LUFS, −1 dBTP)", lufs: -16, truePeakDbtp: -1 },
	{ id: "broadcast", label: "Broadcast EBU R128 (−23 LUFS, −1 dBTP)", lufs: -23, truePeakDbtp: -1 },
	{ id: "loud", label: "Loud social (−11 LUFS, −1 dBTP)", lufs: -11, truePeakDbtp: -1 },
];

export const DEFAULT_LOUDNESS_TARGET = LOUDNESS_TARGETS[0];

export const ABSOLUTE_GATE_LUFS = -70;
const RELATIVE_GATE_LU = -10;
const RANGE_RELATIVE_GATE_LU = -20;
const MOMENTARY_SECONDS = 0.4;
const SHORT_TERM_SECONDS = 3;
const HOP_SECONDS = 0.1;
const LOUDNESS_OFFSET = -0.691;

interface Biquad {
	b0: number;
	b1: number;
	b2: number;
	a1: number;
	a2: number;
}

/** The two K-weighting stages for a sample rate. */
export function kWeighting({ sampleRate }: { sampleRate: number }): [Biquad, Biquad] {
	// Stage 1: high shelf, +4 dB above about 1.7 kHz.
	const f0 = 1681.974450955533;
	const gainDb = 3.999843853973347;
	const q = 0.7071752369554196;
	const k = Math.tan((Math.PI * f0) / sampleRate);
	const vh = 10 ** (gainDb / 20);
	const vb = vh ** 0.4996667741545416;
	const a0 = 1 + k / q + k * k;
	const shelf: Biquad = {
		b0: (vh + (vb * k) / q + k * k) / a0,
		b1: (2 * (k * k - vh)) / a0,
		b2: (vh - (vb * k) / q + k * k) / a0,
		a1: (2 * (k * k - 1)) / a0,
		a2: (1 - k / q + k * k) / a0,
	};
	// Stage 2: high-pass at about 38 Hz.
	const f1 = 38.13547087602444;
	const q1 = 0.5003270373238773;
	const k1 = Math.tan((Math.PI * f1) / sampleRate);
	const a01 = 1 + k1 / q1 + k1 * k1;
	const highPass: Biquad = {
		b0: 1,
		b1: -2,
		b2: 1,
		a1: (2 * (k1 * k1 - 1)) / a01,
		a2: (1 - k1 / q1 + k1 * k1) / a01,
	};
	return [shelf, highPass];
}

function filterInPlace({ samples, stages }: { samples: Float64Array; stages: Biquad[] }): void {
	for (const stage of stages) {
		let x1 = 0;
		let x2 = 0;
		let y1 = 0;
		let y2 = 0;
		for (let index = 0; index < samples.length; index++) {
			const x0 = samples[index];
			const y0 = stage.b0 * x0 + stage.b1 * x1 + stage.b2 * x2 - stage.a1 * y1 - stage.a2 * y2;
			x2 = x1;
			x1 = x0;
			y2 = y1;
			y1 = y0;
			samples[index] = y0;
		}
	}
}

function lufs(meanSquare: number): number {
	return meanSquare > 0 ? LOUDNESS_OFFSET + 10 * Math.log10(meanSquare) : -Infinity;
}

function meanOfPowers(values: number[]): number {
	if (values.length === 0) return -Infinity;
	let sum = 0;
	for (const value of values) sum += 10 ** ((value - LOUDNESS_OFFSET) / 10);
	return lufs(sum / values.length);
}

/**
 * The loudness of each window of `windowSeconds`, every `HOP_SECONDS`,
 * from the summed K-weighted mean squares of the channels.
 */
function windowLoudness({
	weighted,
	sampleRate,
	windowSeconds,
}: {
	weighted: Float64Array[];
	sampleRate: number;
	windowSeconds: number;
}): number[] {
	const length = weighted[0]?.length ?? 0;
	const window = Math.round(windowSeconds * sampleRate);
	const hop = Math.max(1, Math.round(HOP_SECONDS * sampleRate));
	if (window <= 0 || length < window) return [];
	// Running sums of squares per channel for O(n).
	const prefix = weighted.map((channel) => {
		const sums = new Float64Array(channel.length + 1);
		for (let index = 0; index < channel.length; index++) {
			sums[index + 1] = sums[index] + channel[index] * channel[index];
		}
		return sums;
	});
	const values: number[] = [];
	for (let start = 0; start + window <= length; start += hop) {
		let meanSquare = 0;
		for (const sums of prefix) {
			meanSquare += (sums[start + window] - sums[start]) / window;
		}
		values.push(lufs(meanSquare));
	}
	return values;
}

/** Gated integrated loudness of momentary values (absolute, then relative gate). */
export function gatedLoudness({ momentary }: { momentary: number[] }): number {
	const aboveAbsolute = momentary.filter((value) => value > ABSOLUTE_GATE_LUFS);
	if (aboveAbsolute.length === 0) return -Infinity;
	const relative = meanOfPowers(aboveAbsolute) + RELATIVE_GATE_LU;
	const gated = aboveAbsolute.filter((value) => value > relative);
	return meanOfPowers(gated);
}

/** Loudness range of short-term values (EBU Tech 3342). */
export function loudnessRange({ shortTerm }: { shortTerm: number[] }): number {
	const aboveAbsolute = shortTerm.filter((value) => value > ABSOLUTE_GATE_LUFS);
	if (aboveAbsolute.length === 0) return 0;
	const relative = meanOfPowers(aboveAbsolute) + RANGE_RELATIVE_GATE_LU;
	const gated = aboveAbsolute.filter((value) => value > relative).sort((a, b) => a - b);
	if (gated.length < 2) return 0;
	const at = (fraction: number) => gated[Math.min(gated.length - 1, Math.max(0, Math.round((gated.length - 1) * fraction)))];
	return Math.max(0, at(0.95) - at(0.1));
}

/**
 * Polyphase coefficients for `factor`× oversampling: a windowed-sinc
 * low-pass of `taps` taps per phase (Blackman window), scaled so a
 * constant passes unchanged.
 */
export function oversamplingPhases({ factor, taps }: { factor: number; taps: number }): Float64Array[] {
	const total = taps * factor;
	const centre = (total - 1) / 2;
	const kernel = new Float64Array(total);
	for (let index = 0; index < total; index++) {
		const x = (index - centre) / factor;
		const sinc = x === 0 ? 1 : Math.sin(Math.PI * x) / (Math.PI * x);
		const window = 0.42 - 0.5 * Math.cos((2 * Math.PI * index) / (total - 1)) + 0.08 * Math.cos((4 * Math.PI * index) / (total - 1));
		kernel[index] = sinc * window;
	}
	const phases: Float64Array[] = [];
	for (let phase = 0; phase < factor; phase++) {
		const coefficients = new Float64Array(taps);
		let sum = 0;
		for (let tap = 0; tap < taps; tap++) {
			coefficients[tap] = kernel[tap * factor + phase];
			sum += coefficients[tap];
		}
		for (let tap = 0; tap < taps; tap++) coefficients[tap] /= sum;
		phases.push(coefficients);
	}
	return phases;
}

/** The highest absolute sample after oversampling, in dBTP. */
export function truePeak({ channels, sampleRate }: { channels: Float32Array[]; sampleRate: number }): number {
	const factor = sampleRate >= 96_000 ? 2 : 4;
	const phases = oversamplingPhases({ factor, taps: 12 });
	let peak = 0;
	for (const channel of channels) {
		for (let index = 0; index < channel.length; index++) {
			peak = Math.max(peak, Math.abs(channel[index]));
			for (const coefficients of phases) {
				let value = 0;
				for (let tap = 0; tap < coefficients.length; tap++) {
					const at = index - tap + Math.floor(coefficients.length / 2);
					if (at >= 0 && at < channel.length) value += coefficients[tap] * channel[at];
				}
				peak = Math.max(peak, Math.abs(value));
			}
		}
	}
	return peak > 0 ? 20 * Math.log10(peak) : -Infinity;
}

/** Measures `channels` (one Float32Array per channel, equal lengths). */
export function measureLoudness({
	channels,
	sampleRate,
}: {
	channels: Float32Array[];
	sampleRate: number;
}): LoudnessReport {
	const length = channels[0]?.length ?? 0;
	const stages = kWeighting({ sampleRate });
	const weighted = channels.map((channel) => {
		const copy = new Float64Array(length);
		for (let index = 0; index < length; index++) copy[index] = channel[index];
		filterInPlace({ samples: copy, stages });
		return copy;
	});
	const momentary = windowLoudness({ weighted, sampleRate, windowSeconds: MOMENTARY_SECONDS });
	const shortTerm = windowLoudness({ weighted, sampleRate, windowSeconds: SHORT_TERM_SECONDS });
	const max = (values: number[]) => (values.length > 0 ? Math.max(...values) : -Infinity);
	return {
		integratedLufs: gatedLoudness({ momentary }),
		loudnessRangeLu: loudnessRange({ shortTerm }),
		truePeakDbtp: truePeak({ channels, sampleRate }),
		maxMomentaryLufs: max(momentary),
		maxShortTermLufs: max(shortTerm),
		seconds: length / sampleRate,
		sampleRate,
		channels: channels.length,
	};
}

/**
 * The gain in dB that brings the measured audio to `targetLufs`, held
 * back so the true peak stays under `truePeakDbtp`. `null` when nothing
 * was measured (silence).
 */
export function normalizationGain({
	report,
	targetLufs,
	truePeakDbtp,
}: {
	report: LoudnessReport;
	targetLufs: number;
	truePeakDbtp: number;
}): { gainDb: number; peakLimited: boolean } | null {
	if (!Number.isFinite(report.integratedLufs)) return null;
	const wanted = targetLufs - report.integratedLufs;
	const headroom = Number.isFinite(report.truePeakDbtp) ? truePeakDbtp - report.truePeakDbtp : Infinity;
	if (wanted > headroom) {
		return { gainDb: headroom, peakLimited: true };
	}
	return { gainDb: wanted, peakLimited: false };
}

export function formatLufs(value: number): string {
	return Number.isFinite(value) ? `${value.toFixed(1)} LUFS` : "silent";
}
