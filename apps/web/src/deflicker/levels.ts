import { DeflickerLevels, deflickerWindow } from "opencut-wasm";
import type { ParamValues } from "@/params";

/**
 * Deflicker on a video clip: `deflicker.strength` (off, light, medium,
 * strong) and the clip's measured frame levels in `deflicker.levels`
 * (tagged with the media they were measured from). Each frame's brightness
 * gain comes from the shared Rust `deflicker` crate through WASM, as on
 * the desktop (`editor::deflicker_gain`), and the shared compositor
 * applies it before effects. Checked against `fixtures/deflicker/cases.json`.
 */

export const DEFLICKER_STRENGTH_PARAM = "deflicker.strength";
export const DEFLICKER_LEVELS_PARAM = "deflicker.levels";
export const DEFLICKER_STRENGTHS = ["off", "light", "medium", "strong"] as const;
export type DeflickerStrength = (typeof DEFLICKER_STRENGTHS)[number];

export function isDeflickerStrength(value: unknown): value is DeflickerStrength {
	return typeof value === "string" && (DEFLICKER_STRENGTHS as readonly string[]).includes(value);
}

export function readDeflickerStrength(params: ParamValues | undefined): DeflickerStrength {
	const value = params?.[DEFLICKER_STRENGTH_PARAM];
	return isDeflickerStrength(value) ? value : "off";
}

/** Frames averaged around each frame for a strength (0 when off). */
export function strengthWindow(strength: DeflickerStrength): number {
	return deflickerWindow(strength);
}

/** Parsed levels by their stored text; a few clips' worth. */
const parsed = new Map<string, DeflickerLevels | null>();
const PARSED_KEEP = 16;

function parseLevels(text: string): DeflickerLevels | null {
	if (parsed.has(text)) return parsed.get(text) ?? null;
	let levels: DeflickerLevels | null = null;
	try {
		levels = new DeflickerLevels(text);
	} catch {
		levels = null;
	}
	parsed.set(text, levels);
	if (parsed.size > PARSED_KEEP) {
		const oldest = parsed.keys().next().value;
		if (oldest !== undefined) {
			parsed.get(oldest)?.free();
			parsed.delete(oldest);
		}
	}
	return levels;
}

/** The clip's levels when they were measured from `mediaId`. */
export function readDeflickerLevels(params: ParamValues | undefined, mediaId: string): DeflickerLevels | null {
	const text = params?.[DEFLICKER_LEVELS_PARAM];
	if (typeof text !== "string" || text.length === 0) return null;
	const levels = parseLevels(text);
	return levels && levels.mediaId() === mediaId ? levels : null;
}

/** What a video node needs to deflicker, or null when it is off or unmeasured. */
export function deflickerParams(
	params: ParamValues | undefined,
	mediaId: string,
): { window: number; levels: string } | null {
	const window = strengthWindow(readDeflickerStrength(params));
	if (window < 2 || !readDeflickerLevels(params, mediaId)) return null;
	return { window, levels: params?.[DEFLICKER_LEVELS_PARAM] as string };
}

/** The brightness gain at a source time, or null when it leaves the frame as it is. */
export function deflickerGainAt({
	deflicker,
	sourceSeconds,
}: {
	deflicker: { window: number; levels: string };
	sourceSeconds: number;
}): number | null {
	const levels = parseLevels(deflicker.levels);
	if (!levels) return null;
	const gain = levels.gainAt(sourceSeconds, deflicker.window);
	return Math.abs(gain - 1) > 1e-6 ? gain : null;
}
