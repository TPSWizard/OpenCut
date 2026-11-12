/**
 * The emoji sticker pack: Twemoji (CC-BY-4.0, credit Twitter) or Noto
 * Emoji (Apache-2.0, Google), browsed by the Unicode emoji categories the
 * Iconify API lists for each set, with a skin tone applied to the emoji
 * that have one. Stickers are ordinary icon stickers (`icons:<set>:<name>`),
 * so everything that draws icon stickers (the desktop included) draws them.
 */

import { buildStickerId } from "../sticker-id";
import type { StickerBrowseSection, StickerItem, StickerProvider } from "../types";
import { useStickerPackPrefs, type EmojiSetId, type SkinToneId } from "../pack-prefs";
import { ICONIFY_API, ICONS_PROVIDER_ID, buildIconUrl, iconsProvider } from "./icons";

export const EMOJI_PROVIDER_ID = "emoji";

export const EMOJI_SETS: Record<EmojiSetId, { label: string; license: string; credit: string; requiresCredit: boolean }> = {
	twemoji: {
		label: "Twemoji",
		license: "CC-BY-4.0",
		credit: "Twemoji by Twitter (jdecked/twemoji), CC BY 4.0",
		requiresCredit: true,
	},
	noto: {
		label: "Noto Emoji",
		license: "Apache-2.0",
		credit: "Noto Emoji by Google, Apache 2.0",
		requiresCredit: false,
	},
};

export const SKIN_TONES: { id: SkinToneId; label: string; suffix: string; swatch: string }[] = [
	{ id: "default", label: "Default", suffix: "", swatch: "#ffcc4d" },
	{ id: "light", label: "Light", suffix: "-light-skin-tone", swatch: "#f7dece" },
	{ id: "medium-light", label: "Medium light", suffix: "-medium-light-skin-tone", swatch: "#f3d2a2" },
	{ id: "medium", label: "Medium", suffix: "-medium-skin-tone", swatch: "#d5ab88" },
	{ id: "medium-dark", label: "Medium dark", suffix: "-medium-dark-skin-tone", swatch: "#af7e57" },
	{ id: "dark", label: "Dark", suffix: "-dark-skin-tone", swatch: "#7c533e" },
];

/** Unicode's category order. */
export const EMOJI_CATEGORY_ORDER = [
	"Smileys & Emotion",
	"People & Body",
	"Animals & Nature",
	"Food & Drink",
	"Travel & Places",
	"Activities",
	"Objects",
	"Symbols",
	"Flags",
];

export interface EmojiCollection {
	/** Category → names without a skin tone. */
	categories: Record<string, string[]>;
	/** Every name the set has, skin-toned ones included. */
	names: Set<string>;
}

/** Splits a set's category lists into base emoji and the full name set. */
export function buildEmojiCollection({ categories }: { categories: Record<string, string[]> }): EmojiCollection {
	const names = new Set<string>();
	const base: Record<string, string[]> = {};
	for (const [category, list] of Object.entries(categories)) {
		for (const name of list) names.add(name);
		base[category] = list.filter((name) => !name.includes("skin-tone"));
	}
	return { categories: base, names };
}

/** The emoji's name in the chosen skin tone, when the set has that variant. */
export function withSkinTone({ name, tone, names }: { name: string; tone: SkinToneId; names: Set<string> }): string {
	const suffix = SKIN_TONES.find((entry) => entry.id === tone)?.suffix ?? "";
	if (!suffix) return name;
	const toned = `${name}${suffix}`;
	return names.has(toned) ? toned : name;
}

/** Names containing every word of the query, shortest first. */
export function searchEmojiNames({ collection, query, limit }: { collection: EmojiCollection; query: string; limit: number }): string[] {
	const words = query.toLowerCase().split(/[\s-]+/).filter(Boolean);
	if (words.length === 0) return [];
	const seen = new Set<string>();
	const matches: string[] = [];
	for (const list of Object.values(collection.categories)) {
		for (const name of list) {
			if (seen.has(name)) continue;
			seen.add(name);
			if (words.every((word) => name.includes(word))) matches.push(name);
		}
	}
	return matches.sort((a, b) => a.length - b.length || a.localeCompare(b)).slice(0, limit);
}

const collections = new Map<EmojiSetId, Promise<EmojiCollection>>();

export function fetchEmojiCollection({ set, fetchImpl = fetch }: { set: EmojiSetId; fetchImpl?: typeof fetch }): Promise<EmojiCollection> {
	let promise = collections.get(set);
	if (!promise) {
		promise = fetchImpl(`${ICONIFY_API}/collection?prefix=${set}`)
			.then(async (response) => {
				if (!response.ok) throw new Error(`Iconify ${set}: HTTP ${response.status}`);
				const raw = (await response.json()) as { categories?: Record<string, string[]> };
				return buildEmojiCollection({ categories: raw.categories ?? {} });
			})
			.catch((error) => {
				collections.delete(set);
				throw error;
			});
		collections.set(set, promise);
	}
	return promise;
}

function toItem({ set, name }: { set: EmojiSetId; name: string }): StickerItem {
	const info = EMOJI_SETS[set];
	return {
		id: buildStickerId({ providerId: ICONS_PROVIDER_ID, providerValue: `${set}:${name}` }),
		provider: EMOJI_PROVIDER_ID,
		name: name.replace(/-/g, " "),
		previewUrl: buildIconUrl({ prefix: set, name, width: 64, height: 64, palette: true }),
		metadata: {
			prefix: set,
			icon: name,
			collection: info.label,
			license: { title: info.license, spdx: info.license, url: null },
			credit: info.credit,
			requiresCredit: info.requiresCredit,
			palette: true,
		},
	};
}

const DEFAULT_SECTION_SIZE = 72;

export const emojiProvider: StickerProvider = {
	id: EMOJI_PROVIDER_ID,

	async search({ query, options }) {
		const { emojiSet, skinTone } = useStickerPackPrefs.getState();
		const collection = await fetchEmojiCollection({ set: emojiSet });
		const limit = options?.limit ?? 100;
		const names = searchEmojiNames({ collection, query, limit: limit + 1 });
		const items = names
			.slice(0, limit)
			.map((name) => toItem({ set: emojiSet, name: withSkinTone({ name, tone: skinTone, names: collection.names }) }));
		return { items, total: names.length, hasMore: names.length > limit };
	},

	async browse({ options }) {
		const { emojiSet, skinTone } = useStickerPackPrefs.getState();
		const collection = await fetchEmojiCollection({ set: emojiSet });
		const size = options?.limit ?? DEFAULT_SECTION_SIZE;
		const known = EMOJI_CATEGORY_ORDER.filter((category) => collection.categories[category]?.length);
		const others = Object.keys(collection.categories).filter((category) => !known.includes(category));
		const sections: StickerBrowseSection[] = [...known, ...others].map((category) => {
			const list = collection.categories[category] ?? [];
			return {
				id: `emoji:${category}`,
				title: category,
				layout: "grid",
				items: list
					.slice(0, size)
					.map((name) => toItem({ set: emojiSet, name: withSkinTone({ name, tone: skinTone, names: collection.names }) })),
				hasMore: list.length > size,
			};
		});
		return { sections };
	},

	// Emoji stickers are icon stickers; resolve them the same way.
	resolveUrl: (args) => iconsProvider.resolveUrl(args),
};
