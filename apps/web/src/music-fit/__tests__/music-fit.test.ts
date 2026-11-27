import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { musicFit } from "opencut-wasm";
import { describe, expect, it } from "vitest";
import type { TimelineElement } from "@/timeline";
import { canFitMusic, getMusicFitConstants, musicFitClip, type MusicFitResult } from "../fit";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/music-fit/cases.json"), "utf8")) as {
	musics: Record<string, string>;
	cases: Array<{
		name: string;
		music: string;
		sample_rate: number;
		clip: { start: number; duration: number; trimStart: number; trimEnd: number; fadeIn: number; fadeOut: number };
		target: number;
		kind: string | null;
		jumps: [number, number][];
		pieces: Array<{ start: number; duration: number; trimStart: number; trimEnd: number; fadeIn: number; fadeOut: number; crossfade: number }>;
		error: string | null;
	}>;
};

const MESSAGES: Record<string, string> = {
	badLength: "Pick a length between 1 second and 30 minutes",
	alreadyFits: "The music is already that long",
	noBeat: "No steady beat was found in the music, so there are no bars to cut or loop",
	tooShort: "The music is too short to loop: it needs a few bars",
	tooManyPieces: "That is too long for this music: it would take more than 64 pieces",
};

function pcm(text: string): Float32Array {
	const bytes = Buffer.from(text, "base64");
	const out = new Float32Array(bytes.length / 2);
	for (let i = 0; i < out.length; i++) out[i] = bytes.readInt16LE(i * 2) / 32768;
	return out;
}

describe("music fit", () => {
	it("fits as the reference does, through WASM", () => {
		const musics = Object.fromEntries(Object.entries(fixture.musics).map(([name, text]) => [name, pcm(text)]));
		expect(fixture.cases.length).toBeGreaterThanOrEqual(5);
		for (const item of fixture.cases) {
			let fit: MusicFitResult | null = null;
			let error: string | null = null;
			try {
				fit = JSON.parse(musicFit(musics[item.music], item.sample_rate, JSON.stringify(item.clip), item.target));
			} catch (thrown) {
				error = String(thrown);
			}
			if (item.error) {
				expect(error, item.name).toBe(MESSAGES[item.error]);
				continue;
			}
			expect(error, item.name).toBeNull();
			expect(fit!.kind, item.name).toBe(item.kind);
			expect(fit!.jumps.map((jump) => [jump.from, jump.to]), item.name).toEqual(item.jumps);
			expect(fit!.pieces.length, item.name).toBe(item.pieces.length);
			fit!.pieces.forEach((piece, index) => {
				const want = item.pieces[index];
				expect([piece.start, piece.duration, piece.trimStart, piece.trimEnd, piece.crossfade], item.name).toEqual([
					want.start,
					want.duration,
					want.trimStart,
					want.trimEnd,
					want.crossfade,
				]);
				expect(piece.fadeIn).toBeCloseTo(want.fadeIn, 9);
				expect(piece.fadeOut).toBeCloseTo(want.fadeOut, 9);
			});
		}
	});

	it("takes audio clips without a speed change", () => {
		const { sampleRate, crossfadeTicks, maxPieces } = getMusicFitConstants();
		expect([sampleRate, crossfadeTicks, maxPieces]).toEqual([16000, 12000, 64]);
		const audio = { type: "audio", startTime: 0, duration: 240_000, trimStart: 0, trimEnd: 0, params: { fadeIn: 3, fadeOut: 3 } } as unknown as TimelineElement;
		expect(canFitMusic(audio)).toBe(true);
		expect(canFitMusic({ ...audio, retime: { rate: 2 } } as unknown as TimelineElement)).toBe(false);
		expect(canFitMusic({ ...audio, type: "video" } as unknown as TimelineElement)).toBe(false);
		// Fades longer than the clip are scaled down, as both editors play them.
		expect(musicFitClip(audio)).toMatchObject({ fadeIn: 1, fadeOut: 1 });
	});
});
