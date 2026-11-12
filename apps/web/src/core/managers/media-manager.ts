import type { EditorCore } from "@/core";
import { toast } from "sonner";
import type { MediaAsset } from "@/media/types";
import type { MediaAssetData } from "@/services/storage/types";
import { processMediaAssets } from "@/media/processing";
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
	/** Items whose record is stored but whose file is gone; see `relinkMedia`. */
	private missing: MediaAssetData[] = [];
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
			const { assets, missing } = await storageService.loadProjectMediaState({
				projectId,
			});
			this.assets = assets;
			this.missing = missing;
			this.notify();
			if (missing.length > 0) {
				toast.warning(`${missing.length} media file(s) are missing from this browser's storage`, {
					description: "Relink them from the media panel; clips using them stay empty until then.",
				});
			}
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

		const mediaIds = [...this.assets.map((asset) => asset.id), ...this.missing.map((item) => item.id)];
		this.assets = [];
		this.missing = [];
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
		this.missing = [];
		this.notify();
	}

	getMissingAssets(): MediaAssetData[] {
		return this.missing;
	}

	/**
	 * Gives a stored media item a new file (it went missing, or the user is
	 * swapping in a better copy). The id stays, so clips keep using it.
	 */
	async relinkMedia({
		projectId,
		mediaId,
		file,
	}: {
		projectId: string;
		mediaId: string;
		file: File;
	}): Promise<MediaAsset> {
		const current = this.assets.find((item) => item.id === mediaId) ?? this.missing.find((item) => item.id === mediaId);
		if (!current) throw new Error("No such media item");
		const [processed] = await processMediaAssets({ files: [file] });
		if (!processed) throw new Error(`${file.name} could not be read`);
		if (processed.type !== current.type) {
			const kind = (type: string) => (type === "image" ? "an image" : type === "audio" ? "an audio file" : "a video");
			throw new Error(`${file.name} is ${kind(processed.type)}; ${current.name} was ${kind(current.type)}`);
		}
		const asset: MediaAsset = { ...processed, id: mediaId };
		await storageService.saveMediaAsset({ projectId, mediaAsset: asset });
		const old = this.assets.find((item) => item.id === mediaId);
		if (old?.url) URL.revokeObjectURL(old.url);
		videoCache.clearVideo({ mediaId });
		this.assets = old ? this.assets.map((item) => (item.id === mediaId ? asset : item)) : [...this.assets, asset];
		this.missing = this.missing.filter((item) => item.id !== mediaId);
		this.notify();
		return asset;
	}

	/**
	 * Relinks missing items to picked files with the same name (ignoring
	 * case; the same size wins when names repeat). Returns what matched.
	 */
	async relinkByFiles({
		projectId,
		files,
	}: {
		projectId: string;
		files: File[];
	}): Promise<{ relinked: string[]; unmatched: string[]; failed: string[] }> {
		const relinked: string[] = [];
		const failed: string[] = [];
		const used = new Set<File>();
		for (const item of [...this.missing]) {
			const sameName = files.filter((file) => !used.has(file) && file.name.toLowerCase() === item.name.toLowerCase());
			const file = sameName.find((candidate) => candidate.size === item.size) ?? sameName[0];
			if (!file) continue;
			used.add(file);
			try {
				await this.relinkMedia({ projectId, mediaId: item.id, file });
				relinked.push(item.id);
			} catch (error) {
				failed.push(`${file.name}: ${error instanceof Error ? error.message : String(error)}`);
			}
		}
		return { relinked, unmatched: files.filter((file) => !used.has(file)).map((file) => file.name), failed };
	}

	/** Drops a missing item's record (its clips stay empty). */
	async forgetMissing({ projectId, mediaId }: { projectId: string; mediaId: string }): Promise<void> {
		await storageService.deleteMediaAsset({ projectId, id: mediaId });
		this.missing = this.missing.filter((item) => item.id !== mediaId);
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
