import type { EditorCore } from "@/core";
import { toast } from "sonner";
import type { MediaAsset } from "@/media/types";
import { storageService } from "@/services/storage/service";
import { generateUUID } from "@/utils/id";
import { videoCache } from "@/services/video-cache/service";
import { waveformCache } from "@/services/waveform-cache/service";
import { BatchCommand, RemoveMediaAssetCommand } from "@/commands";
import { makeProxy, wantsProxy } from "@/media/proxy";
import { useProxyJobs } from "@/media/proxy-store";
import { usePreviewStore } from "@/preview/preview-store";

export class MediaManager {
	private assets: MediaAsset[] = [];
	private isLoading = false;
	private listeners = new Set<() => void>();
	private proxyJobs = new Map<string, Promise<MediaAsset>>();

	constructor(private editor: EditorCore) {}

	async addMediaAsset({
		projectId,
		asset,
	}: {
		projectId: string;
		asset: Omit<MediaAsset, "id">;
	}): Promise<MediaAsset | null> {
		const newAsset: MediaAsset = {
			...asset,
			id: generateUUID(),
		};

		this.assets = [...this.assets, newAsset];
		this.notify();

		try {
			await storageService.saveMediaAsset({ projectId, mediaAsset: newAsset });
			this.editor.project.ratchetFpsForImportedMedia({
				importedAssets: [newAsset],
			});
			if (
				newAsset.type === "video" &&
				!newAsset.proxyFile &&
				usePreviewStore.getState().autoProxies &&
				wantsProxy({ width: newAsset.width, height: newAsset.height })
			) {
				void this.makeProxy({ projectId, mediaId: newAsset.id }).catch(() => {});
			}
			return newAsset;
		} catch (error) {
			console.error("Failed to save media asset:", error);
			this.assets = this.assets.filter((asset) => asset.id !== newAsset.id);
			this.notify();

			if (storageService.isQuotaExceededError({ error })) {
				toast.error("Not enough browser storage", {
					description: error instanceof Error ? error.message : undefined,
				});
			}

			return null;
		}
	}

	removeMediaAsset({ projectId, id }: { projectId: string; id: string }): void {
		this.removeMediaAssets({ projectId, ids: [id] });
	}

	removeMediaAssets({
		projectId,
		ids,
	}: {
		projectId: string;
		ids: string[];
	}): void {
		const uniqueIds = [...new Set(ids)];
		if (uniqueIds.length === 0) {
			return;
		}

		const command =
			uniqueIds.length === 1
				? new RemoveMediaAssetCommand({
						projectId,
						assetId: uniqueIds[0],
					})
				: new BatchCommand(
						uniqueIds.map((id) =>
							new RemoveMediaAssetCommand({
								projectId,
								assetId: id,
							}),
						),
					);

		this.editor.command.execute({ command });
	}

	async loadProjectMedia({ projectId }: { projectId: string }): Promise<void> {
		this.isLoading = true;
		this.notify();

		try {
			const mediaAssets = await storageService.loadAllMediaAssets({
				projectId,
			});
			this.assets = mediaAssets;
			this.notify();
		} catch (error) {
			console.error("Failed to load media assets:", error);
		} finally {
			this.isLoading = false;
			this.notify();
		}
	}

	async clearProjectMedia({ projectId }: { projectId: string }): Promise<void> {
		waveformCache.clearAll();

		this.assets.forEach((asset) => {
			if (asset.url) {
				URL.revokeObjectURL(asset.url);
			}
			if (asset.thumbnailUrl) {
				URL.revokeObjectURL(asset.thumbnailUrl);
			}
		});

		const mediaIds = this.assets.map((asset) => asset.id);
		this.assets = [];
		this.notify();

		try {
			await Promise.all(
				mediaIds.map((id) =>
					storageService.deleteMediaAsset({ projectId, id }),
				),
			);
		} catch (error) {
			console.error("Failed to clear media assets from storage:", error);
		}
	}

	clearAllAssets(): void {
		videoCache.clearAll();
		waveformCache.clearAll();

		this.assets.forEach((asset) => {
			if (asset.url) {
				URL.revokeObjectURL(asset.url);
			}
			if (asset.thumbnailUrl) {
				URL.revokeObjectURL(asset.thumbnailUrl);
			}
		});

		this.assets = [];
		this.notify();
	}

	getAssets(): MediaAsset[] {
		return this.assets;
	}

	/**
	 * Makes and stores a small proxy of a video for the preview to decode
	 * (export keeps reading the original). Concurrent calls share one job.
	 */
	makeProxy({ projectId, mediaId }: { projectId: string; mediaId: string }): Promise<MediaAsset> {
		const running = this.proxyJobs.get(mediaId);
		if (running) return running;
		const asset = this.assets.find((item) => item.id === mediaId);
		if (!asset) return Promise.reject(new Error("No such media item"));
		if (asset.type !== "video") return Promise.reject(new Error("Only videos get proxies"));
		const jobs = useProxyJobs.getState();
		jobs.start(mediaId);
		const job = (async () => {
			try {
				const made = await makeProxy({
					file: asset.file,
					width: asset.width ?? 1920,
					height: asset.height ?? 1080,
					onProgress: (progress) => useProxyJobs.getState().progress(mediaId, progress),
				});
				await storageService.saveMediaProxy({
					projectId,
					mediaId,
					file: made.file,
					width: made.width,
					height: made.height,
				});
				const updated = this.updateAsset(mediaId, {
					proxyFile: made.file,
					proxyWidth: made.width,
					proxyHeight: made.height,
				});
				useProxyJobs.getState().finish(mediaId);
				if (!updated) throw new Error("The media item was removed");
				return updated;
			} catch (error) {
				const message = error instanceof Error ? error.message : String(error);
				useProxyJobs.getState().finish(mediaId, message);
				throw error;
			} finally {
				this.proxyJobs.delete(mediaId);
			}
		})();
		this.proxyJobs.set(mediaId, job);
		return job;
	}

	/** Deletes a video's proxy; the preview decodes the original again. */
	async removeProxy({ projectId, mediaId }: { projectId: string; mediaId: string }): Promise<MediaAsset | null> {
		await storageService.deleteMediaProxy({ projectId, mediaId });
		videoCache.clearVideo({ mediaId });
		return this.updateAsset(mediaId, {
			proxyFile: undefined,
			proxyWidth: undefined,
			proxyHeight: undefined,
		});
	}

	private updateAsset(mediaId: string, patch: Partial<MediaAsset>): MediaAsset | null {
		let updated: MediaAsset | null = null;
		this.assets = this.assets.map((item) => {
			if (item.id !== mediaId) return item;
			const next: MediaAsset = { ...item, ...patch };
			for (const key of Object.keys(patch) as (keyof MediaAsset)[]) {
				if (patch[key] === undefined) delete next[key];
			}
			updated = next;
			return next;
		});
		this.notify();
		return updated;
	}

	setAssets({ assets }: { assets: MediaAsset[] }): void {
		this.assets = assets;
		this.notify();
	}

	isLoadingMedia(): boolean {
		return this.isLoading;
	}

	subscribe(listener: () => void): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	private notify(): void {
		this.listeners.forEach((fn) => {
			fn();
		});
	}
}
