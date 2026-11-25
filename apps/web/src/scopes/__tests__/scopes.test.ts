import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { drawScope, isVideoScope, useScopeStore, VIDEO_SCOPES } from "../capture";

const root = resolve(__dirname, "../../../../..");
const fixture = JSON.parse(readFileSync(resolve(root, "fixtures/scopes/cases.json"), "utf8")) as {
	frames: {
		name: string;
		width: number;
		height: number;
		rgba: number[];
		stats: { samples: number; atBlack: number; atWhite: number; blackLevel: number; whiteLevel: number };
		images: Record<string, { width: number; height: number; fnv: string }>;
	}[];
};

function fnv(bytes: Uint8ClampedArray): string {
	let hash = 0xcbf29ce484222325n;
	for (const byte of bytes) {
		hash ^= BigInt(byte);
		hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
	}
	return hash.toString(16).padStart(16, "0");
}

describe("video scopes through WASM", () => {
	it("draws the same pictures as the reference (and the desktop)", () => {
		for (const frame of fixture.frames) {
			for (const scope of VIDEO_SCOPES) {
				const result = drawScope({ scope, rgba: new Uint8Array(frame.rgba), width: frame.width, height: frame.height });
				const want = frame.images[scope];
				expect([result.width, result.height], `${frame.name} ${scope}`).toEqual([want.width, want.height]);
				expect(fnv(result.rgba), `${frame.name} ${scope}`).toBe(want.fnv);
				expect(result.samples).toBe(frame.stats.samples);
				expect(result.atBlack).toBeCloseTo(frame.stats.atBlack, 12);
				expect(result.atWhite).toBeCloseTo(frame.stats.atWhite, 12);
				expect([result.blackLevel, result.whiteLevel]).toEqual([frame.stats.blackLevel, frame.stats.whiteLevel]);
			}
		}
	});

	it("knows its scopes and drops a reading of another one", () => {
		expect(isVideoScope("parade")).toBe(true);
		expect(isVideoScope("oscilloscope")).toBe(false);
		const frame = fixture.frames[0];
		const result = drawScope({ scope: "waveform", rgba: new Uint8Array(frame.rgba), width: frame.width, height: frame.height });
		useScopeStore.setState({ scope: "waveform", result });
		useScopeStore.getState().setScope("waveform");
		expect(useScopeStore.getState().result).toBe(result);
		useScopeStore.getState().setScope("histogram");
		expect(useScopeStore.getState().result).toBeNull();
		useScopeStore.getState().setScope(null);
	});
});
