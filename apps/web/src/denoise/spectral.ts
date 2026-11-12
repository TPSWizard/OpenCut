/**
 * Spectral noise reduction: a short-time Fourier transform (1024-point
 * Hann frames, quarter-frame hops), a noise profile taken as the 30th
 * percentile of each bin's magnitude over the whole clip (so steady
 * background hiss, hum and room tone are the profile as long as the clip
 * has pauses), and a per-bin gain that subtracts that noise power from
 * the time-smoothed power and never drops below a floor set by the
 * requested reduction. Gains are median-smoothed across neighbouring bins
 * to keep "musical noise" down. The desktop uses FFmpeg's `afftdn`, which
 * works the same way.
 */

export interface DenoiseOptions {
	/** How far the noise floor is pushed down, in dB (6 to 30). */
	reductionDb: number;
	/** How much of the measured noise power is subtracted (1 is exact, 2 twice). */
	overSubtraction: number;
	/** The percentile of each bin's magnitude over the clip taken as its noise (0 to 1). */
	noisePercentile: number;
	/** How much of the previous frame's power carries into the next (0 none, 0.9 slow); steadies the noise estimate. */
	powerSmoothing: number;
}

export const DENOISE_DEFAULTS: DenoiseOptions = { reductionDb: 12, overSubtraction: 3.5, noisePercentile: 0.3, powerSmoothing: 0.85 };

export const FRAME = 1024;
const HOP = FRAME / 4;

/** In-place iterative radix-2 FFT on interleaved real/imaginary arrays. */
export function fft({ real, imag }: { real: Float64Array; imag: Float64Array }): void {
	const n = real.length;
	for (let i = 1, j = 0; i < n; i++) {
		let bit = n >> 1;
		for (; j & bit; bit >>= 1) j ^= bit;
		j ^= bit;
		if (i < j) {
			[real[i], real[j]] = [real[j], real[i]];
			[imag[i], imag[j]] = [imag[j], imag[i]];
		}
	}
	for (let length = 2; length <= n; length <<= 1) {
		const angle = (-2 * Math.PI) / length;
		const wReal = Math.cos(angle);
		const wImag = Math.sin(angle);
		for (let start = 0; start < n; start += length) {
			let curReal = 1;
			let curImag = 0;
			for (let k = 0; k < length / 2; k++) {
				const a = start + k;
				const b = a + length / 2;
				const tReal = real[b] * curReal - imag[b] * curImag;
				const tImag = real[b] * curImag + imag[b] * curReal;
				real[b] = real[a] - tReal;
				imag[b] = imag[a] - tImag;
				real[a] += tReal;
				imag[a] += tImag;
				const nextReal = curReal * wReal - curImag * wImag;
				curImag = curReal * wImag + curImag * wReal;
				curReal = nextReal;
			}
		}
	}
}

/** Inverse FFT in place (conjugate trick), scaled by 1/n. */
export function ifft({ real, imag }: { real: Float64Array; imag: Float64Array }): void {
	for (let i = 0; i < imag.length; i++) imag[i] = -imag[i];
	fft({ real, imag });
	const n = real.length;
	for (let i = 0; i < n; i++) {
		real[i] /= n;
		imag[i] = -imag[i] / n;
	}
}

function hann(): Float64Array {
	const window = new Float64Array(FRAME);
	for (let i = 0; i < FRAME; i++) window[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / FRAME);
	return window;
}

/** Magnitudes per frame (FRAME/2 + 1 bins) and the spectra to rebuild from. */
function analyse({ samples }: { samples: Float32Array }): { frames: number; real: Float64Array[]; imag: Float64Array[]; magnitude: Float64Array[] } {
	const window = hann();
	const frames = Math.max(1, Math.ceil(samples.length / HOP) + 3);
	const real: Float64Array[] = [];
	const imag: Float64Array[] = [];
	const magnitude: Float64Array[] = [];
	for (let frame = 0; frame < frames; frame++) {
		const offset = frame * HOP - 3 * HOP;
		const re = new Float64Array(FRAME);
		const im = new Float64Array(FRAME);
		for (let i = 0; i < FRAME; i++) {
			const index = offset + i;
			re[i] = index >= 0 && index < samples.length ? samples[index] * window[i] : 0;
		}
		fft({ real: re, imag: im });
		const mag = new Float64Array(FRAME / 2 + 1);
		for (let bin = 0; bin <= FRAME / 2; bin++) mag[bin] = Math.hypot(re[bin], im[bin]);
		real.push(re);
		imag.push(im);
		magnitude.push(mag);
	}
	return { frames, real, imag, magnitude };
}

/** Per-bin noise magnitude: a low percentile of each bin over the frames. */
export function noiseProfile({ magnitude, percentile }: { magnitude: Float64Array[]; percentile: number }): Float64Array {
	const bins = magnitude[0]?.length ?? 0;
	const profile = new Float64Array(bins);
	const column = new Float64Array(magnitude.length);
	for (let bin = 0; bin < bins; bin++) {
		for (let frame = 0; frame < magnitude.length; frame++) column[frame] = magnitude[frame][bin];
		column.sort();
		profile[bin] = column[Math.min(column.length - 1, Math.floor(column.length * Math.min(1, Math.max(0, percentile))))];
	}
	return profile;
}

/** Reduces steady noise in one channel; the output has the input's length. */
export function denoiseChannel({
	samples,
	options = {},
}: {
	samples: Float32Array;
	options?: Partial<DenoiseOptions>;
}): Float32Array {
	const settings: DenoiseOptions = { ...DENOISE_DEFAULTS, ...options };
	const floor = 10 ** (-Math.max(0, settings.reductionDb) / 20);
	const over = Math.max(0, settings.overSubtraction);
	const { frames, real, imag, magnitude } = analyse({ samples });
	const profile = noiseProfile({ magnitude, percentile: settings.noisePercentile });
	const bins = FRAME / 2 + 1;
	const window = hann();
	const out = new Float64Array(samples.length);
	const weight = new Float64Array(samples.length);
	const gain = new Float64Array(bins);
	const smoothed = new Float64Array(bins);
	const steady = new Float64Array(bins);
	const keep = Math.min(0.99, Math.max(0, settings.powerSmoothing));
	for (let frame = 0; frame < frames; frame++) {
		const mag = magnitude[frame];
		for (let bin = 0; bin < bins; bin++) {
			const power = mag[bin] * mag[bin];
			steady[bin] = frame === 0 ? power : keep * steady[bin] + (1 - keep) * power;
			const noise = profile[bin] * profile[bin] * over;
			gain[bin] = steady[bin] > 0 ? Math.max(floor, Math.sqrt(Math.max(0, steady[bin] - noise) / steady[bin])) : floor;
		}
		for (let bin = 0; bin < bins; bin++) {
			const a = gain[Math.max(0, bin - 1)];
			const b = gain[bin];
			const c = gain[Math.min(bins - 1, bin + 1)];
			smoothed[bin] = a + b + c - Math.max(a, b, c) - Math.min(a, b, c);
		}
		const re = real[frame];
		const im = imag[frame];
		for (let bin = 0; bin < bins; bin++) {
			re[bin] *= smoothed[bin];
			im[bin] *= smoothed[bin];
			if (bin > 0 && bin < FRAME / 2) {
				re[FRAME - bin] = re[bin];
				im[FRAME - bin] = -im[bin];
			}
		}
		ifft({ real: re, imag: im });
		const offset = frame * HOP - 3 * HOP;
		for (let i = 0; i < FRAME; i++) {
			const index = offset + i;
			if (index < 0 || index >= samples.length) continue;
			out[index] += re[i] * window[i];
			weight[index] += window[i] * window[i];
		}
	}
	const result = new Float32Array(samples.length);
	for (let i = 0; i < samples.length; i++) result[i] = weight[i] > 1e-9 ? out[i] / weight[i] : 0;
	return result;
}

/** Reduces noise in every channel. */
export function denoiseChannels({
	channels,
	options = {},
}: {
	channels: Float32Array[];
	options?: Partial<DenoiseOptions>;
}): Float32Array[] {
	return channels.map((samples) => denoiseChannel({ samples, options }));
}

/** 16-bit PCM WAV of the channels. */
export function encodeWav({ channels, sampleRate }: { channels: Float32Array[]; sampleRate: number }): Blob {
	const count = channels.length;
	const frames = channels[0]?.length ?? 0;
	const data = new ArrayBuffer(44 + frames * count * 2);
	const view = new DataView(data);
	const text = (offset: number, value: string) => {
		for (let i = 0; i < value.length; i++) view.setUint8(offset + i, value.charCodeAt(i));
	};
	text(0, "RIFF");
	view.setUint32(4, 36 + frames * count * 2, true);
	text(8, "WAVE");
	text(12, "fmt ");
	view.setUint32(16, 16, true);
	view.setUint16(20, 1, true);
	view.setUint16(22, count, true);
	view.setUint32(24, sampleRate, true);
	view.setUint32(28, sampleRate * count * 2, true);
	view.setUint16(32, count * 2, true);
	view.setUint16(34, 16, true);
	text(36, "data");
	view.setUint32(40, frames * count * 2, true);
	let offset = 44;
	for (let frame = 0; frame < frames; frame++) {
		for (let channel = 0; channel < count; channel++) {
			const sample = Math.max(-1, Math.min(1, channels[channel][frame]));
			view.setInt16(offset, Math.round(sample * 32767), true);
			offset += 2;
		}
	}
	return new Blob([data], { type: "audio/wav" });
}
