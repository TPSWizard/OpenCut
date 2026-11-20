/**
 * Exports that show face-swapped media say so in the file's metadata
 * (`SceneExporter`'s `disclosure`). Kept free of editor imports so the
 * renderer manager can use it.
 */

import type { MediaOrigin } from "@/services/storage/types";

export const FACE_SWAP_EXPORT_NOTE =
	"Contains AI face swap (made with ConzaCut; HyperSwap by FaceFusion Labs via Segmind).";

/** True when any of `mediaIds` is media made by the face swap. */
export function usesFaceSwap({
	assets,
	mediaIds,
}: {
	assets: { id: string; origin?: MediaOrigin }[];
	mediaIds: Iterable<string>;
}): boolean {
	const swapped = new Set(assets.filter((asset) => asset.origin?.tool === "faceswap").map((asset) => asset.id));
	for (const id of mediaIds) if (swapped.has(id)) return true;
	return false;
}
