/**
 * Tempo and beat tracking from onset energy: the signal's log-energy flux
 * (5 ms hops) with its local mean removed is the onset strength; the
 * tempo is the autocorrelation peak of that strength between 60 and 200
 * BPM, weighted towards 120 BPM; the beat phase is the offset that lines
 * the beat grid up with the strongest onsets. The desktop runs the same
 * arithmetic (`crates/editor/src/beats.rs`).
 */

export interface BeatAnalysis {
	/** Estimated tempo, or `null` when nothing periodic was found. */
	bpm: number | null;
	/** Autocorrelation peak over the zero lag, 0 to 1. */
	confidence: number;
	/** Beat times in seconds from the start of the samples. */
	beats: number[];
	seconds: number;
}

export interface BeatOptions {
	minBpm: number;
	maxBpm: number;
	/** The tempo the weighting prefers when the autocorrelation is ambiguous. */
	preferredBpm: number;
}

export const BEAT_DEFAULTS: BeatOptions = { minBpm: 60, maxBpm: 200, preferredBpm: 120 };

const HOP_SECONDS = 0.005;
const WINDOW_HOPS = 4;
const LOCAL_MEAN_SECONDS = 0.5;
const MIN_CONFIDENCE = 0.05;
/** Onsets weaker than this (about half a decibel of flux) are noise, not beats. */
const MIN_ONSET = 0.05;

/** Log-energy flux per hop with the local mean removed, clipped at zero. */
export function onsetStrength({ samples, sampleRate }: { samples: Float32Array; sampleRate: number }): Float64Array {
	const hop = Math.max(1, Math.round(sampleRate * HOP_SECONDS));
	const window = hop * WINDOW_HOPS;
	const frames = Math.max(0, Math.floor((samples.length - window) / hop) + 1);
	const energy = new Float64Array(frames);
	for (let frame = 0; frame < frames; frame++) {
		let sum = 0;
		const start = frame * hop;
		for (let index = start; index < start + window; index++) sum += samples[index] * samples[index];
		energy[frame] = sum / window;
	}
	const flux = new Float64Array(frames);
	for (let frame = 1; frame < frames; frame++) {
		flux[frame] = Math.max(0, Math.log(energy[frame] + 1e-10) - Math.log(energy[frame - 1] + 1e-10));
	}
	const radius = Math.round(LOCAL_MEAN_SECONDS / HOP_SECONDS / 2);
	const onset = new Float64Array(frames);
	let windowSum = 0;
	let left = 0;
	let right = -1;
	for (let frame = 0; frame < frames; frame++) {
		while (right < Math.min(frames - 1, frame + radius)) {
			right += 1;
			windowSum += flux[right];
		}
		while (left < frame - radius) {
			windowSum -= flux[left];
			left += 1;
		}
		const mean = windowSum / (right - left + 1);
		onset[frame] = Math.max(0, flux[frame] - mean);
	}
	return onset;
}

function autocorrelation({ onset, lag }: { onset: Float64Array; lag: number }): number {
	let sum = 0;
	for (let index = 0; index + lag < onset.length; index++) sum += onset[index] * onset[index + lag];
	return sum / Math.max(1, onset.length - lag);
}

/** Finds the tempo and beats of mono samples. */
export function trackBeats({
	samples,
	sampleRate,
	options = {},
}: {
	samples: Float32Array;
	sampleRate: number;
	options?: Partial<BeatOptions>;
}): BeatAnalysis {
	const settings: BeatOptions = { ...BEAT_DEFAULTS, ...options };
	const seconds = samples.length / sampleRate;
	const onset = onsetStrength({ samples, sampleRate });
	const empty: BeatAnalysis = { bpm: null, confidence: 0, beats: [], seconds };
	const zero = autocorrelation({ onset, lag: 0 });
	let strongest = 0;
	for (const value of onset) strongest = Math.max(strongest, value);
	if (onset.length < 2 / HOP_SECONDS || zero <= 0 || strongest < MIN_ONSET) return empty;

	const minLag = Math.max(1, Math.round(60 / settings.maxBpm / HOP_SECONDS));
	const maxLag = Math.min(onset.length - 1, Math.round(60 / settings.minBpm / HOP_SECONDS));
	const preferredLag = 60 / settings.preferredBpm / HOP_SECONDS;
	let bestLag = 0;
	let bestScore = -Infinity;
	let bestRaw = 0;
	const correlations = new Map<number, number>();
	for (let lag = minLag; lag <= maxLag; lag++) {
		const raw = autocorrelation({ onset, lag });
		correlations.set(lag, raw);
		const weight = Math.exp(-0.5 * (Math.log2(lag / preferredLag) / 1.0) ** 2);
		const score = raw * weight;
		if (score > bestScore) {
			bestScore = score;
			bestLag = lag;
			bestRaw = raw;
		}
	}
	const confidence = bestRaw / zero;
	if (bestLag === 0 || confidence < MIN_CONFIDENCE) return empty;

	// Refine the period between hops with a parabola through the neighbours.
	const before = correlations.get(bestLag - 1) ?? bestRaw;
	const after = correlations.get(bestLag + 1) ?? bestRaw;
	const denominator = before - 2 * bestRaw + after;
	const shift = denominator !== 0 ? Math.max(-0.5, Math.min(0.5, (0.5 * (before - after)) / denominator)) : 0;
	const period = bestLag + shift;
	const bpm = 60 / (period * HOP_SECONDS);

	let bestOffset = 0;
	let bestPhase = -Infinity;
	for (let offset = 0; offset < bestLag; offset++) {
		let sum = 0;
		for (let frame = offset; frame < onset.length; frame += bestLag) sum += onset[frame];
		if (sum > bestPhase) {
			bestPhase = sum;
			bestOffset = offset;
		}
	}
	const beats: number[] = [];
	for (let beat = 0; ; beat++) {
		const time = (bestOffset + beat * period) * HOP_SECONDS;
		if (time >= seconds) break;
		beats.push(Number(time.toFixed(4)));
	}
	return { bpm, confidence, beats, seconds };
}
