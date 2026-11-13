/**
 * The open project's asset manifest (what a bundle's `assets.json` would
 * say right now) and its credit lines.
 */

import type { EditorCore } from "@/core";
import { buildAssetManifest, gatherAssetLookups, type AssetManifest } from "./assets";
import { projectBundleJson } from "./bundle";

export async function projectAssetManifest({
	editor,
	lookUp = true,
}: {
	editor: EditorCore;
	/** Look up icon set and font licences online (a few seconds at most). */
	lookUp?: boolean;
}): Promise<AssetManifest> {
	// Saving folds the live timeline into the active project.
	await editor.project.saveCurrentProject();
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const bundle = projectBundleJson({ project, mediaAssets: editor.media.getAssets() });
	const lookups = lookUp ? await gatherAssetLookups({ bundle }) : {};
	return buildAssetManifest({ bundle, lookups });
}

/** The credit lines of everything in the project that has one, one per line. */
export async function projectCreditsText({ editor }: { editor: EditorCore }): Promise<string> {
	const manifest = await projectAssetManifest({ editor });
	return manifest.credits.join("\n");
}
