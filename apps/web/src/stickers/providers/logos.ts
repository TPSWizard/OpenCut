/**
 * The brands sticker pack: brand logos from Simple Icons (CC0-1.0), drawn
 * through the Iconify API in the brand's colour, white or black. The brand
 * list (`brands.json`, built by `scripts/build-brand-stickers.py` from a
 * pinned Simple Icons release) keeps only icons without share-alike,
 * copyleft or custom terms; the desktop reads the same file. Logos are
 * trademarks: the panel shows the notice and each sticker carries the
 * brand's guidelines link.
 *
 * Sticker ids are `logos:<slug>:<RRGGBB>`.
 */

import { buildStickerId, parseStickerId } from "../sticker-id";
import type { StickerItem, StickerProvider } from "../types";
import { useStickerPackPrefs, type BrandColorMode } from "../pack-prefs";
import { ICONIFY_API } from "./icons";

export const LOGOS_PROVIDER_ID = "logos";

/** [slug, title, hex, guidelines, licence, needs credit] */
export type BrandEntry = [string, string, string, string, string, boolean];

export interface BrandData {
	source: string;
	sourceUrl: string;
	license: string;
	trademark: string;
	brands: BrandEntry[];
}

/** Shown first when browsing (present in the data; others drop out silently). */
export const POPULAR_BRANDS = [
	"youtube", "instagram", "tiktok", "x", "facebook", "threads", "snapchat", "bluesky", "reddit", "discord",
	"twitch", "kick", "spotify", "applemusic", "soundcloud", "netflix", "apple", "google", "android", "paypal",
	"stripe", "github", "gitlab", "figma", "notion", "whatsapp", "telegram", "signal", "zoom", "gmail",
	"wordpress", "shopify", "airbnb", "uber", "adidas", "nike", "playstation", "steam", "epicgames", "anthropic",
	"claude", "mastodon", "pinterest", "tumblr", "vimeo", "patreon", "kofi", "substack", "medium", "dribbble",
	"behance", "davinciresolve", "obsstudio", "unity", "unrealengine",
];

let dataPromise: Promise<BrandData> | null = null;

export function loadBrandData(): Promise<BrandData> {
	if (!dataPromise) {
		dataPromise = import("./brands.json").then((module) => (module.default ?? module) as unknown as BrandData);
	}
	return dataPromise;
}

export function brandColorHex({ hex, mode }: { hex: string; mode: BrandColorMode }): string {
	if (mode === "white") return "FFFFFF";
	if (mode === "black") return "000000";
	return hex.toUpperCase();
}

export function buildBrandUrl({ slug, hex, width, height }: { slug: string; hex: string; width?: number; height?: number }): string {
	const params = new URLSearchParams();
	if (width) params.set("width", String(Math.round(width)));
	if (height) params.set("height", String(Math.round(height)));
	params.set("color", `#${hex}`);
	return `${ICONIFY_API}/simple-icons/${encodeURIComponent(slug)}.svg?${params.toString()}`;
}

export function parseBrandStickerId({ stickerId }: { stickerId: string }): { slug: string; hex: string } {
	const { providerId, providerValue } = parseStickerId({ stickerId });
	if (providerId !== LOGOS_PROVIDER_ID) throw new Error(`Not a brand sticker: ${stickerId}`);
	const [slug, hex] = providerValue.split(":");
	if (!slug || !/^[0-9a-fA-F]{6}$/.test(hex ?? "")) throw new Error(`Brand sticker needs slug:RRGGBB, got ${providerValue}`);
	return { slug, hex: hex.toUpperCase() };
}

/** Brands whose title, slug or words match the query, best first. */
export function searchBrands({ brands, query, limit }: { brands: BrandEntry[]; query: string; limit: number }): BrandEntry[] {
	const needle = query.trim().toLowerCase();
	if (!needle) return [];
	const compact = needle.replace(/[^a-z0-9]/g, "");
	const scored: [number, BrandEntry][] = [];
	for (const brand of brands) {
		const title = brand[1].toLowerCase();
		const slug = brand[0];
		let score = -1;
		if (title === needle || slug === compact) score = 0;
		else if (title.startsWith(needle) || slug.startsWith(compact)) score = 1;
		else if (title.includes(needle) || (compact && slug.includes(compact))) score = 2;
		if (score >= 0) scored.push([score, brand]);
	}
	return scored
		.sort((a, b) => a[0] - b[0] || a[1][1].length - b[1][1].length || a[1][1].localeCompare(b[1][1]))
		.slice(0, limit)
		.map(([, brand]) => brand);
}

function toItem({ brand, mode, data }: { brand: BrandEntry; mode: BrandColorMode; data: BrandData }): StickerItem {
	const [slug, title, hex, guidelines, licence, credit] = brand;
	const color = brandColorHex({ hex, mode });
	return {
		id: buildStickerId({ providerId: LOGOS_PROVIDER_ID, providerValue: `${slug}:${color}` }),
		provider: LOGOS_PROVIDER_ID,
		name: title,
		previewUrl: buildBrandUrl({ slug, hex: color, width: 64, height: 64 }),
		metadata: {
			brand: title,
			brandColor: `#${hex}`,
			guidelines: guidelines || null,
			license: { title: licence || data.license, spdx: licence || data.license, url: null },
			requiresCredit: credit,
			source: data.source,
			trademark: data.trademark,
		},
	};
}

export const logosProvider: StickerProvider = {
	id: LOGOS_PROVIDER_ID,

	async search({ query, options }) {
		const data = await loadBrandData();
		const { brandColor } = useStickerPackPrefs.getState();
		const limit = options?.limit ?? 100;
		const found = searchBrands({ brands: data.brands, query, limit: limit + 1 });
		return {
			items: found.slice(0, limit).map((brand) => toItem({ brand, mode: brandColor, data })),
			total: found.length,
			hasMore: found.length > limit,
		};
	},

	async browse({ options }) {
		const data = await loadBrandData();
		const { brandColor } = useStickerPackPrefs.getState();
		const bySlug = new Map(data.brands.map((brand) => [brand[0], brand]));
		const popular = POPULAR_BRANDS.map((slug) => bySlug.get(slug)).filter((brand): brand is BrandEntry => Boolean(brand));
		const size = options?.limit ?? 240;
		return {
			sections: [
				{
					id: "logos:popular",
					title: "Popular",
					layout: "grid",
					items: popular.slice(0, Math.max(size, 12)).map((brand) => toItem({ brand, mode: brandColor, data })),
					hasMore: false,
				},
				{
					id: "logos:all",
					title: `All brands A–Z (${data.brands.length}; search for the rest)`,
					layout: "grid",
					items: data.brands.slice(0, size).map((brand) => toItem({ brand, mode: brandColor, data })),
					hasMore: data.brands.length > size,
				},
			],
		};
	},

	resolveUrl({ stickerId, options }) {
		const { slug, hex } = parseBrandStickerId({ stickerId });
		return buildBrandUrl({ slug, hex, width: options?.width, height: options?.height });
	},
};
