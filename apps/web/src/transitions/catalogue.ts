import { listTransitions } from "opencut-wasm";
import { mediaTime, TICKS_PER_SECOND } from "@/wasm";
import type { TransitionInfo } from "./types";

/** What a clip gets when a transition is added without choosing one. */
export const DEFAULT_TRANSITION_TYPE = "Fade";

/** One second. */
export const DEFAULT_TRANSITION_DURATION = mediaTime({ ticks: TICKS_PER_SECOND });

let catalogue: TransitionInfo[] | null = null;

/**
 * Every transition the compositor can draw, sorted by name, straight from
 * the Wasm build so the list can never drift from what renders.
 */
export function getTransitionCatalogue(): TransitionInfo[] {
	if (!catalogue) {
		catalogue = listTransitions() as TransitionInfo[];
	}
	return catalogue;
}

export function getTransitionInfo({
	name,
}: {
	name: string;
}): TransitionInfo | null {
	return getTransitionCatalogue().find((info) => info.name === name) ?? null;
}

/** Catalogue entries whose name, description or tags mention `query`. */
export function searchTransitions({
	query,
}: {
	query: string;
}): TransitionInfo[] {
	const needle = query.trim().toLowerCase();
	const all = getTransitionCatalogue();
	if (!needle) return all;
	return all.filter(
		(info) =>
			info.name.toLowerCase().includes(needle) ||
			(info.description ?? "").toLowerCase().includes(needle) ||
			info.tags.some((tag) => tag.includes(needle)),
	);
}
