import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { useRouter } from "@/compat/navigation";
import { useEditor } from "@/editor/use-editor";
import { t } from "@/i18n";
import { processMediaAssets } from "@/media/processing";
import { buildElementFromMedia } from "@/timeline/element-utils";
import { DEFAULT_NEW_ELEMENT_DURATION } from "@/timeline/creation";
import {
	addMediaTime,
	mediaTimeFromSeconds,
	ZERO_MEDIA_TIME,
} from "@/wasm";
import {
	claimSharePending,
	markSharePending,
	takeSharedFiles,
} from "./share-inbox";

/**
 * On `/projects?shared=<n>` (where the service worker sends a share): makes
 * a project for the shared files and opens it.
 */
export function useShareLanding(): void {
	const editor = useEditor();
	const router = useRouter();
	const handled = useRef(false);

	useEffect(() => {
		if (handled.current) return;
		const params = new URLSearchParams(window.location.search);
		const shared = params.get("shared");
		if (shared === null) return;
		handled.current = true;
		params.delete("shared");
		const query = params.toString();
		window.history.replaceState(
			null,
			"",
			`${window.location.pathname}${query ? `?${query}` : ""}`,
		);
		if (shared === "error" || shared === "0") {
			toast.error(t("Nothing could be taken from that share"), {
				description: t("Share videos, photos or audio files."),
			});
			return;
		}
		(async () => {
			try {
				const projectId = await editor.project.createNewProject({
					name: t("Shared clips"),
				});
				markSharePending(projectId);
				router.push(`/editor/${projectId}`);
			} catch (error) {
				toast.error(t("Failed to create project"), {
					description:
						error instanceof Error ? error.message : t("Please try again"),
				});
			}
		})();
	}, [editor, router]);
}

/**
 * In the project made for a share: imports the shared files and lays them
 * on the timeline one after another, in the order they were shared.
 */
export function useSharedImport(): void {
	const editor = useEditor();
	const projectId = useEditor((e) => e.project.getActiveOrNull()?.metadata.id ?? null);

	useEffect(() => {
		if (!projectId || !claimSharePending(projectId)) return;
		(async () => {
			const files = await takeSharedFiles();
			if (files.length === 0) return;
			const toastId = toast.loading(t("Adding the shared files…"));
			try {
				const assets = await processMediaAssets({ files });
				let start = ZERO_MEDIA_TIME;
				for (const asset of assets) {
					const stored = await editor.media.addMediaAsset({ projectId, asset });
					if (!stored) continue;
					const duration =
						stored.duration != null
							? mediaTimeFromSeconds({ seconds: stored.duration })
							: DEFAULT_NEW_ELEMENT_DURATION;
					editor.timeline.insertElement({
						element: buildElementFromMedia({
							mediaId: stored.id,
							mediaType: stored.type,
							name: stored.name,
							duration,
							startTime: start,
						}),
						placement: { mode: "auto" },
					});
					start = addMediaTime({ a: start, b: duration });
				}
				toast.success(t("Shared files added"), {
					id: toastId,
					description: t("{count} added to the timeline.", { count: assets.length }),
				});
			} catch (error) {
				toast.error(t("The shared files could not be added"), {
					id: toastId,
					description:
						error instanceof Error ? error.message : t("Please try again"),
				});
			}
		})();
	}, [editor, projectId]);
}

/** Mounted inside the editor: runs `useSharedImport`. */
export function SharedMediaImport(): null {
	useSharedImport();
	return null;
}
