/**
 * Icon stickers from the Iconify API (https://api.iconify.design, MIT),
 * limited to icon sets whose licence allows use in a video without
 * conditions beyond credit: the set list is fetched once and filtered by
 * its SPDX licence, so a set that changes licence drops out by itself.
 * Each sticker carries its set's licence and author in `metadata`, and
 * CC-BY sets are marked as needing credit.
 */

import { buildStickerId, parseStickerId } from "../sticker-id";
import type {
	StickerBrowseResult,
	StickerBrowseSection,
	StickerItem,
	StickerProvider,
} from "../types";

export const ICONS_PROVIDER_ID = "icons";
export const ICONIFY_API = "https://api.iconify.design";

/** Licences accepted outright. */
export const ALLOWED_ICON_LICENSES = new Set([
	"MIT",
	"Apache-2.0",
	"ISC",
	"BSD-2-Clause",
	"BSD-3-Clause",
	"0BSD",
	"CC0-1.0",
	"Unlicense",
	"MPL-2.0",
]);

/** Licences accepted when the set is credited. */
export const CREDIT_ICON_LICENSES = new Set(["CC-BY-4.0", "CC-BY-3.0"]);

const DEFAULT_SEARCH_LIMIT = 64;
const BROWSE_SECTIONS = 8;
const BROWSE_SAMPLES = 12;
/** Sets to show first when browsing; the rest follow by size. */
const FEATURED_PREFIXES = ["mdi", "tabler", "lucide", "ph", "ri", "material-symbols", "fluent-emoji", "twemoji"];

export interface IconCollection {
	prefix: string;
	name: string;
	total: number;
	license: { title: string; spdx: string | null; url: string | null };
	author: { name: string; url: string | null };
	samples: string[];
	palette: boolean;
	requiresCredit: boolean;
}

interface RawCollection {
	name?: unknown;
	total?: unknown;
	license?: { title?: unknown; spdx?: unknown; url?: unknown };
	author?: { name?: unknown; url?: unknown };
	samples?: unknown;
	palette?: unknown;
}

/** Keeps the sets whose licence is on the allow lists. */
export function filterCollections({
	raw,
}: {
	raw: Record<string, RawCollection>;
}): IconCollection[] {
	const kept: IconCollection[] = [];
	for (const [prefix, entry] of Object.entries(raw)) {
		const spdx = typeof entry.license?.spdx === "string" ? entry.license.spdx : null;
		if (!spdx) continue;
		const allowed = ALLOWED_ICON_LICENSES.has(spdx);
		const credit = CREDIT_ICON_LICENSES.has(spdx);
		if (!allowed && !credit) continue;
		kept.push({
			prefix,
			name: typeof entry.name === "string" ? entry.name : prefix,
			total: typeof entry.total === "number" ? entry.total : 0,
			license: {
				title: typeof entry.license?.title === "string" ? entry.license.title : spdx,
				spdx,
				url: typeof entry.license?.url === "string" ? entry.license.url : null,
			},
			author: {
				name: typeof entry.author?.name === "string" ? entry.author.name : "Unknown",
				url: typeof entry.author?.url === "string" ? entry.author.url : null,
			},
			samples: Array.isArray(entry.samples) ? entry.samples.filter((s): s is string => typeof s === "string") : [],
			palette: entry.palette === true,
			requiresCredit: credit,
		});
	}
	return kept.sort((a, b) => {
		const ai = FEATURED_PREFIXES.indexOf(a.prefix);
		const bi = FEATURED_PREFIXES.indexOf(b.prefix);
		if (ai !== -1 || bi !== -1) {
			if (ai === -1) return 1;
			if (bi === -1) return -1;
			return ai - bi;
		}
		return b.total - a.total;
	});
}

export function buildIconUrl({
	prefix,
	name,
	width,
	height,
	color = "#ffffff",
	palette = false,
}: {
	prefix: string;
	name: string;
	width?: number;
	height?: number;
	color?: string;
	palette?: boolean;
}): string {
	const params = new URLSearchParams();
	if (width) params.set("width", String(Math.round(width)));
	if (height) params.set("height", String(Math.round(height)));
	// Monochrome sets take a colour; palette (emoji) sets keep their own.
	if (!palette) params.set("color", color);
	const query = params.toString();
	return `${ICONIFY_API}/${encodeURIComponent(prefix)}/${encodeURIComponent(name)}.svg${query ? `?${query}` : ""}`;
}

let collectionsPromise: Promise<IconCollection[]> | null = null;

export function clearIconCollectionsCache(): void {
	collectionsPromise = null;
}

export function fetchIconCollections({
	fetchImpl = fetch,
}: { fetchImpl?: typeof fetch } = {}): Promise<IconCollection[]> {
	if (!collectionsPromise) {
		collectionsPromise = fetchImpl(`${ICONIFY_API}/collections`)
			.then(async (response) => {
				if (!response.ok) throw new Error(`Iconify collections: HTTP ${response.status}`);
				const raw = (await response.json()) as Record<string, RawCollection>;
				return filterCollections({ raw });
			})
			.catch((error) => {
				collectionsPromise = null;
				throw error;
			});
	}
	return collectionsPromise;
}

function toItem({
	prefix,
	name,
	collection,
}: {
	prefix: string;
	name: string;
	collection: IconCollection | undefined;
}): StickerItem {
	const palette = collection?.palette ?? false;
	return {
		id: buildStickerId({ providerId: ICONS_PROVIDER_ID, providerValue: `${prefix}:${name}` }),
		provider: ICONS_PROVIDER_ID,
		name: name.replace(/-/g, " "),
		previewUrl: buildIconUrl({ prefix, name, width: 64, height: 64, palette }),
		metadata: {
			prefix,
			icon: name,
			collection: collection?.name ?? prefix,
			license: collection?.license ?? null,
			author: collection?.author ?? null,
			requiresCredit: collection?.requiresCredit ?? false,
			palette,
		},
	};
}

export function parseIconStickerId({ stickerId }: { stickerId: string }): { prefix: string; name: string } {
	const { providerId, providerValue } = parseStickerId({ stickerId });
	if (providerId !== ICONS_PROVIDER_ID) {
		throw new Error(`Not an icon sticker: ${stickerId}`);
	}
	const separator = providerValue.indexOf(":");
	if (separator <= 0) {
		throw new Error(`Icon sticker needs prefix:name, got ${providerValue}`);
	}
	return { prefix: providerValue.slice(0, separator), name: providerValue.slice(separator + 1) };
}

/** Palette (emoji) sets known without a network round trip, for resolving URLs. */
const PALETTE_PREFIXES = new Set(["fluent-emoji", "fluent-emoji-flat", "twemoji", "noto", "emojione", "openmoji", "fxemoji", "streamline-emojis", "logos", "skill-icons", "devicon", "vscode-icons", "flat-color-icons", "circle-flags", "flagpack", "flag"]);

export const iconsProvider: StickerProvider = {
	id: ICONS_PROVIDER_ID,

	async search({ query, options }) {
		const trimmed = query.trim();
		if (!trimmed) return { items: [], total: 0, hasMore: false };
		const collections = await fetchIconCollections();
		const byPrefix = new Map(collections.map((collection) => [collection.prefix, collection]));
		const limit = Math.min(999, Math.max(32, options?.limit ?? DEFAULT_SEARCH_LIMIT));
		const params = new URLSearchParams({
			query: trimmed,
			limit: String(limit),
			prefixes: collections.map((collection) => collection.prefix).join(","),
		});
		const response = await fetch(`${ICONIFY_API}/search?${params.toString()}`);
		if (!response.ok) throw new Error(`Iconify search: HTTP ${response.status}`);
		const raw = (await response.json()) as { icons?: unknown; total?: unknown };
		const icons = Array.isArray(raw.icons) ? raw.icons.filter((i): i is string => typeof i === "string") : [];
		const items = icons
			.map((entry) => {
				const separator = entry.indexOf(":");
				if (separator <= 0) return null;
				const prefix = entry.slice(0, separator);
				if (!byPrefix.has(prefix)) return null;
				return toItem({ prefix, name: entry.slice(separator + 1), collection: byPrefix.get(prefix) });
			})
			.filter((item): item is StickerItem => item !== null);
		const total = typeof raw.total === "number" ? raw.total : items.length;
		return { items, total, hasMore: total > items.length };
	},

	async browse({ options }) {
		const collections = await fetchIconCollections();
		const page = options?.page ?? 0;
		const perPage = options?.limit ?? BROWSE_SECTIONS;
		const sections: StickerBrowseSection[] = collections
			.slice(page * perPage, page * perPage + perPage)
			.map((collection) => ({
				id: `icons:${collection.prefix}`,
				title: `${collection.name} · ${collection.license.title}${collection.requiresCredit ? " (credit the author)" : ""}`,
				layout: "row",
				items: collection.samples
					.slice(0, BROWSE_SAMPLES)
					.map((name) => toItem({ prefix: collection.prefix, name, collection })),
				hasMore: collection.total > BROWSE_SAMPLES,
			}));
		const result: StickerBrowseResult = { sections };
		return result;
	},

	resolveUrl({ stickerId, options }) {
		const { prefix, name } = parseIconStickerId({ stickerId });
		return buildIconUrl({
			prefix,
			name,
			width: options?.width,
			height: options?.height,
			palette: PALETTE_PREFIXES.has(prefix),
		});
	},
};
