/**
 * Stock media: Openverse (CC-licensed images, called directly from the
 * browser so each visitor has their own quota, no key) and Pixabay (images
 * and videos through the API worker, which holds the key and caches for 24
 * hours). Imported items keep their credit on the media item, and the
 * project's credits can be copied as text. The desktop editor's
 * `crates/editor/src/stock.rs` parses Openverse the same way (same test data).
 */

import type { EditorCore } from "@/core";
import { apiUrl } from "@/lib/api";
import { processMediaAssets } from "@/media/processing";
import type { StockCredit } from "@/services/storage/types";
import { buildElementFromMedia } from "@/timeline/element-utils";
import { mediaTimeFromSeconds } from "@/wasm";

export type StockKind = "image" | "video";
export type StockProvider = "openverse" | "pixabay";

/** One search result (the API worker's and the desktop editor's `StockItem`). */
export interface StockItem {
	provider: StockProvider;
	id: string;
	kind: StockKind;
	title: string;
	thumbnail: string;
	width: number;
	height: number;
	duration?: number;
	license: string;
	licenseUrl: string;
	creator: string;
	creatorUrl?: string;
	sourceUrl: string;
	attribution: string;
	/** The file itself, when the provider gives it out (Openverse). */
	fileUrl?: string;
}

/** Free to use and change commercially: CC0, Public Domain Mark, CC BY (no share-alike, no non-commercial). */
export const OPENVERSE_LICENSES = "cc0,pdm,by";

export function openverseLicenseLabel({ license, version }: { license: string; version: string }): string {
	const name = license.toLowerCase() === "cc0" ? "CC0" : license.toLowerCase() === "pdm" ? "Public Domain Mark" : `CC ${license.toUpperCase()}`;
	return version ? `${name} ${version}` : name;
}

type Json = Record<string, unknown>;
const text = (item: Json, key: string): string => (typeof item[key] === "string" ? (item[key] as string) : "");

export function parseOpenverse({ json }: { json: unknown }): StockItem[] {
	const results = (json as { results?: unknown })?.results;
	if (!Array.isArray(results)) return [];
	return results.flatMap((raw): StockItem[] => {
		const item = (raw ?? {}) as Json;
		const id = text(item, "id");
		const url = text(item, "url");
		if (!id || !url.startsWith("http")) return [];
		const title = text(item, "title") || "Untitled";
		const creator = text(item, "creator") || "Unknown";
		const creatorUrl = text(item, "creator_url");
		return [
			{
				provider: "openverse",
				id,
				kind: "image",
				title,
				thumbnail: text(item, "thumbnail"),
				width: typeof item.width === "number" ? item.width : 0,
				height: typeof item.height === "number" ? item.height : 0,
				license: openverseLicenseLabel({ license: text(item, "license"), version: text(item, "license_version") }),
				licenseUrl: text(item, "license_url"),
				creator,
				...(creatorUrl ? { creatorUrl } : {}),
				sourceUrl: text(item, "foreign_landing_url"),
				attribution: text(item, "attribution") || `"${title}" by ${creator}`,
				fileUrl: url,
			},
		];
	});
}

export function creditOf({ item }: { item: StockItem }): StockCredit {
	return {
		provider: item.provider,
		id: item.id,
		title: item.title,
		creator: item.creator,
		...(item.creatorUrl ? { creatorUrl: item.creatorUrl } : {}),
		license: item.license,
		licenseUrl: item.licenseUrl,
		sourceUrl: item.sourceUrl,
		attribution: item.attribution,
	};
}

export class StockUnavailableError extends Error {}

export async function searchStock({
	provider,
	kind,
	query,
	page = 1,
}: {
	provider: StockProvider;
	kind: StockKind;
	query: string;
	page?: number;
}): Promise<{ items: StockItem[]; total: number }> {
	if (provider === "openverse") {
		if (kind !== "image") throw new StockUnavailableError("Openverse has pictures only; videos come from Pixabay.");
		const params = new URLSearchParams({
			q: query.slice(0, 100),
			page: String(page),
			page_size: "20", // Openverse's most for anonymous requests
			license: OPENVERSE_LICENSES,
			mature: "false",
		});
		const response = await fetch(`https://api.openverse.org/v1/images/?${params.toString()}`);
		if (response.status === 429) throw new Error("Openverse's limit for this hour or day is reached; try again later.");
		if (!response.ok) throw new Error(`Openverse search failed (${response.status})`);
		const json = (await response.json()) as { result_count?: number };
		return { items: parseOpenverse({ json }), total: json.result_count ?? 0 };
	}
	const params = new URLSearchParams({ q: query.slice(0, 100), kind, page: String(page) });
	const response = await fetch(apiUrl(`/stock/search?${params.toString()}`));
	if (response.status === 503) throw new StockUnavailableError("Pixabay is not set up on this server yet (it needs an API key).");
	if (!response.ok) throw new Error(`Pixabay search failed (${response.status})`);
	const json = (await response.json()) as { results?: StockItem[]; total?: number };
	return { items: json.results ?? [], total: json.total ?? 0 };
}

/** The file of a result: straight from the source when it allows it, else through the API worker. */
async function fetchStockFile({ item }: { item: StockItem }): Promise<Blob> {
	if (item.provider === "openverse" && item.fileUrl) {
		try {
			const direct = await fetch(item.fileUrl);
			if (direct.ok) return await direct.blob();
		} catch {
			// The source does not allow the browser to read it; the worker can.
		}
	}
	const params = new URLSearchParams({ provider: item.provider, id: item.id, kind: item.kind });
	const response = await fetch(apiUrl(`/stock/file?${params.toString()}`));
	if (!response.ok) throw new Error(`The file could not be downloaded (${response.status})`);
	return response.blob();
}

function extensionFor({ blob, item }: { blob: Blob; item: StockItem }): string {
	const fromType = blob.type.split("/")[1]?.replace("jpeg", "jpg").split(";")[0];
	if (fromType && /^[a-z0-9]{2,5}$/.test(fromType)) return fromType;
	return item.kind === "video" ? "mp4" : "jpg";
}

/** Downloads a result into the project with its credit; with `addToTimeline`, puts it at the playhead. */
export async function importStockItem({
	editor,
	item,
	addToTimeline = true,
}: {
	editor: EditorCore;
	item: StockItem;
	addToTimeline?: boolean;
}): Promise<{ mediaId: string; name: string }> {
	const project = editor.project.getActive();
	if (!project) throw new Error("No project is open");
	const blob = await fetchStockFile({ item });
	const safeTitle = item.title.replace(/[\\/:*?"<>|]/g, "").slice(0, 60) || "Stock";
	const file = new File([blob], `${safeTitle} (${item.provider}).${extensionFor({ blob, item })}`, {
		type: blob.type || (item.kind === "video" ? "video/mp4" : "image/jpeg"),
	});
	const [processed] = await processMediaAssets({ files: [file] });
	if (!processed) throw new Error("The file could not be read");
	const saved = await editor.media.addMediaAsset({
		projectId: project.metadata.id,
		asset: { ...processed, credit: creditOf({ item }) },
	});
	if (!saved) throw new Error("The file could not be stored");
	if (addToTimeline) {
		const element = buildElementFromMedia({
			mediaId: saved.id,
			mediaType: saved.type,
			name: saved.name,
			duration: mediaTimeFromSeconds({ seconds: saved.duration ?? 5 }),
			startTime: editor.playback.getCurrentTime(),
		});
		editor.timeline.insertElement({
			element,
			placement: { mode: "auto", trackType: saved.type === "audio" ? "audio" : "video" },
		});
	}
	return { mediaId: saved.id, name: saved.name };
}

/** The credits of the project's stock media, one line each. */
export function projectCredits({ editor }: { editor: EditorCore }): string {
	const lines: string[] = [];
	for (const asset of editor.media.getAssets()) {
		const line = asset.credit?.attribution;
		if (line && !lines.includes(line)) lines.push(line);
	}
	return lines.join("\n");
}
