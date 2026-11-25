import { maskTrackOffset } from "opencut-wasm";
import { TICKS_PER_SECOND } from "@/wasm";

/**
 * A tracked mask (row 92) at a time: its centre moved by its stored track
 * (`track` param) at `localTicks` into the clip, as the desktop's frame
 * plan does (`tracking::mask_offset`). Masks without a track come back as
 * they are.
 */
export function withMaskTrack<T extends { params: unknown }>(mask: T, localTicks: number | undefined): T {
	const params = mask.params as Record<string, unknown>;
	const track = typeof params.track === "string" ? params.track : "";
	if (!track || localTicks === undefined) return mask;
	const [dx, dy] = maskTrackOffset(track, localTicks / TICKS_PER_SECOND);
	if (dx === 0 && dy === 0) return mask;
	const centerX = typeof params.centerX === "number" ? params.centerX : 0;
	const centerY = typeof params.centerY === "number" ? params.centerY : 0;
	return { ...mask, params: { ...params, centerX: centerX + dx, centerY: centerY + dy } };
}
