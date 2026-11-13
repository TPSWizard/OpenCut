/**
 * The asset manifest: `assets.json` in a project bundle, one entry per
 * thing the project uses that someone else may hold rights to (media files,
 * library sounds, stickers, fonts), each with its origin, licence, source
 * and credit line, plus the credits to give. The desktop editor builds it
 * the same way (`crates/editor/src/assets.rs`, checked against the same
 * test data in `fixtures/asset-manifest`). It is written on export and not
 * read back: `project.json` stays the source of truth.
 *
 * A licence of `null` means it is not known: the user's own files, and
 * sources whose licence is not recorded. Nothing is guessed.
 */

import { ICONIFY_API } from "@/stickers/providers/icons";
import { fetchFontsourceCatalogue } from "@/fonts/fontsource";
import type { MediaOrigin } from "@/services/storage/types";

export const ASSET_MANIFEST_FORMAT = "opencut-asset-manifest";
export const ASSET_MANIFEST_FORMAT_VERSION = 1;
export const ASSET_MANIFEST_ENTRY = "assets.json";

export interface IconSetInfo {
	name: string;
	license: string | null;
	licenseUrl: string | null;
	author: string | null;
}

export interface BrandInfo {
	title: string;
	license: string;
	creditRequired: boolean;
}

/** Licences the editor looked up: icon sets by prefix, brands by slug, Fontsource families (SPDX id) by family name. */
export interface AssetLookups {
	iconSets?: Record<string, IconSetInfo>;
	brands?: Record<string, BrandInfo>;
	fonts?: Record<string, string>;
}

/** Fonts the editors treat as the system's own (`@/fonts/system-fonts`, and the desktop's `SYSTEM_FONTS`). */
const SYSTEM_FONTS = new Set(["Arial", "Helvetica", "Times New Roman", "Courier New", "Verdana", "Georgia", "monospace", "sans-serif", "serif"]);

export interface AssetEntry {
	kind: "media" | "sound" | "sticker" | "font";
	id: string;
	name: string;
	file?: string;
	origin: "imported" | "stock" | "library" | "generated" | "fetched" | "built-in" | "system";
	provider?: string;
	license: string | null;
	licenseUrl?: string;
	creator?: string;
	sourceUrl?: string;
	attribution?: string;
	creditRequired: boolean;
	derivedFrom?: string;
	note?: string;
	uses: number;
}

export interface AssetManifest {
	format: typeof ASSET_MANIFEST_FORMAT;
	formatVersion: number;
	project: { id: string | null; name: string | null };
	summary: { assets: number; creditRequired: number; licenseUnknown: number };
	credits: string[];
	assets: AssetEntry[];
}

export function licenseRequiresCredit(license: string): boolean {
	const upper = license.toUpperCase();
	return upper.startsWith("CC BY") || upper.startsWith("CC-BY");
}

export function fontsourceSlug(family: string): string {
	return family
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
}

function freesoundPage(url: string): string | undefined {
	const match = /\/previews\/[^/]+\/(\d+)_/.exec(url);
	return match ? `https://freesound.org/s/${match[1]}/` : undefined;
}

type Json = Record<string, unknown>;
const str = (value: unknown): string | undefined => (typeof value === "string" ? value : undefined);
const nonEmpty = (value: unknown): string | undefined => {
	const text = str(value);
	return text ? text : undefined;
};
const byCodeUnit = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

interface Uses {
	media: Map<string, number>;
	sounds: Map<string, { name: string; count: number }>;
	stickers: Map<string, { name: string; count: number }>;
	fonts: Map<string, number>;
}

/** Visits every object (keys in sorted order, as the desktop's JSON maps are). */
function walk(value: unknown, uses: Uses): void {
	if (Array.isArray(value)) {
		for (const item of value) walk(item, uses);
		return;
	}
	if (!value || typeof value !== "object") return;
	const map = value as Json;
	const mediaId = str(map.mediaId);
	if (mediaId !== undefined) uses.media.set(mediaId, (uses.media.get(mediaId) ?? 0) + 1);
	const type = str(map.type);
	if (type === "audio" && map.sourceType === "library") {
		const url = str(map.sourceUrl);
		if (url !== undefined) {
			const entry = uses.sounds.get(url) ?? { name: str(map.name) ?? url, count: 0 };
			entry.count += 1;
			uses.sounds.set(url, entry);
		}
	} else if (type === "sticker") {
		const id = str(map.stickerId);
		if (id !== undefined) {
			const entry = uses.stickers.get(id) ?? { name: str(map.name) ?? id, count: 0 };
			entry.count += 1;
			uses.stickers.set(id, entry);
		}
	} else if (type === "text") {
		const params = map.params as Json | undefined;
		const family = params && typeof params === "object" ? str(params.fontFamily) : undefined;
		if (family !== undefined && family.trim()) uses.fonts.set(family, (uses.fonts.get(family) ?? 0) + 1);
	}
	for (const key of Object.keys(map).sort(byCodeUnit)) walk(map[key], uses);
}

/** Drops absent optional fields, as the desktop's serializer does. */
function clean(entry: AssetEntry): AssetEntry {
	const out: Record<string, unknown> = {};
	for (const [key, value] of Object.entries(entry)) {
		if (value !== undefined) out[key] = value;
	}
	return out as unknown as AssetEntry;
}

function mediaEntry(item: Json, uses: Uses): AssetEntry {
	const id = str(item.id) ?? "";
	const origin = item.origin && typeof item.origin === "object" ? (item.origin as MediaOrigin) : undefined;
	const credit = item.credit && typeof item.credit === "object" ? (item.credit as Json) : undefined;
	const entry: AssetEntry = {
		kind: "media",
		id,
		name: str(item.name) ?? "",
		file: nonEmpty(item.file),
		origin: "imported",
		license: null,
		creditRequired: false,
		uses: uses.media.get(id) ?? 0,
	};
	if (credit) {
		const license = str(credit.license) ?? "";
		entry.origin = origin?.kind === "library" ? "library" : "stock";
		entry.provider = nonEmpty(credit.provider);
		entry.license = license || null;
		entry.licenseUrl = nonEmpty(credit.licenseUrl);
		entry.creator = nonEmpty(credit.creator);
		entry.sourceUrl = nonEmpty(credit.sourceUrl);
		entry.attribution = nonEmpty(credit.attribution);
		entry.creditRequired = licenseRequiresCredit(license);
		if (!license) entry.note = "The licence was not recorded with this item.";
		return entry;
	}
	if (origin?.kind === "generated") {
		const tool = origin.tool ?? "a tool";
		entry.origin = "generated";
		entry.provider = tool;
		entry.derivedFrom = origin.from;
		entry.note =
			origin.from !== undefined && origin.model !== undefined
				? `Made by the ${tool} tool from another item (model: ${origin.model}); it keeps that item's rights.`
				: origin.from !== undefined
					? `Made by the ${tool} tool from another item; it keeps that item's rights.`
					: origin.model !== undefined
						? `Made by the ${tool} tool (model: ${origin.model}).`
						: `Made by the ${tool} tool.`;
	} else if (origin?.kind === "library") {
		entry.origin = "library";
		entry.sourceUrl = origin.source;
		entry.note = "A library sound whose licence was not recorded.";
	} else {
		entry.note = "Added from your own files: the rights are yours to check.";
	}
	return entry;
}

function soundEntry(url: string, name: string, count: number): AssetEntry {
	const entry: AssetEntry = { kind: "sound", id: url, name, origin: "library", license: null, creditRequired: false, uses: count };
	if (url.includes("freesound.org")) {
		entry.provider = "freesound";
		entry.sourceUrl = freesoundPage(url) ?? url;
		entry.note = "A Freesound preview: check its licence and credit on its Freesound page.";
	} else if (url.startsWith("/sounds/")) {
		entry.provider = "bundled-sounds";
		entry.note = "One of the editor's bundled sounds, not copied into this bundle.";
	} else {
		entry.sourceUrl = url;
		entry.note = "A library sound whose licence was not recorded.";
	}
	return entry;
}

function stickerEntry(stickerId: string, name: string, count: number, lookups: AssetLookups): AssetEntry {
	const entry: AssetEntry = { kind: "sticker", id: stickerId, name, origin: "fetched", license: null, creditRequired: false, uses: count };
	const separator = stickerId.indexOf(":");
	const provider = separator === -1 ? stickerId : stickerId.slice(0, separator);
	const value = separator === -1 ? "" : stickerId.slice(separator + 1);
	const slug = value.split(":")[0] ?? "";
	if (provider === "icons") {
		const inner = value.indexOf(":");
		const prefix = inner === -1 ? value : value.slice(0, inner);
		const icon = inner === -1 ? "" : value.slice(inner + 1);
		entry.provider = "iconify";
		entry.sourceUrl = `https://icon-sets.iconify.design/${prefix}/${icon}/`;
		if (prefix === "twemoji") {
			entry.license = "CC-BY-4.0";
			entry.licenseUrl = "https://creativecommons.org/licenses/by/4.0/";
			entry.creator = "Twitter";
			entry.attribution = "Twemoji by Twitter (jdecked/twemoji), CC BY 4.0";
		} else if (prefix === "noto") {
			entry.license = "Apache-2.0";
			entry.licenseUrl = "https://www.apache.org/licenses/LICENSE-2.0";
			entry.creator = "Google";
			entry.attribution = "Noto Emoji by Google, Apache 2.0";
		} else {
			const set = lookups.iconSets?.[prefix];
			if (set?.license) {
				entry.license = set.license;
				entry.licenseUrl = set.licenseUrl ?? undefined;
				entry.creator = set.author ?? undefined;
				entry.attribution = set.author ? `${set.name} by ${set.author} (${set.license})` : `${set.name} (${set.license})`;
			} else {
				entry.note = `Icon set "${prefix}": its licence was not looked up.`;
			}
		}
	} else if (provider === "logos") {
		entry.provider = "simple-icons";
		entry.sourceUrl = `https://simpleicons.org/?q=${slug}`;
		const brand = lookups.brands?.[slug];
		if (brand) {
			entry.license = brand.license || null;
			entry.attribution = brand.creditRequired ? `${brand.title} icon from Simple Icons (${brand.license})` : undefined;
			entry.note = `${brand.title} is a trademark of its owner.`;
		} else {
			entry.note = "A brand logo (a trademark of its owner); its licence was not looked up.";
		}
	} else if (provider === "flags") {
		entry.origin = "built-in";
		entry.provider = "conzacut";
		entry.note = "A flag picture shipped with the editor; its source licence is not recorded.";
	} else if (provider === "shapes") {
		entry.origin = "built-in";
		entry.provider = "conzacut";
		entry.note = "A shape drawn by the editor.";
	} else {
		entry.note = "A sticker from an unknown source.";
	}
	entry.creditRequired =
		(entry.license !== null && licenseRequiresCredit(entry.license)) ||
		(provider === "logos" && lookups.brands?.[slug]?.creditRequired === true);
	return entry;
}

function fontEntry(family: string, count: number, lookups: AssetLookups): AssetEntry {
	const entry: AssetEntry = { kind: "font", id: family, name: family, origin: "fetched", license: null, creditRequired: false, uses: count };
	if (SYSTEM_FONTS.has(family)) {
		entry.origin = "system";
		entry.note = "A system font: not included; each computer's own copy is used.";
		return entry;
	}
	const license = lookups.fonts?.[family];
	if (license !== undefined) {
		entry.provider = "fontsource";
		entry.license = license || null;
		entry.sourceUrl = `https://fontsource.org/fonts/${fontsourceSlug(family)}`;
	} else {
		entry.note = "A web font whose licence was not looked up.";
	}
	entry.creditRequired = entry.license !== null && licenseRequiresCredit(entry.license);
	return entry;
}

/** The manifest of a bundle's `project.json` (`{ project, media }`). */
export function buildAssetManifest({ bundle, lookups = {} }: { bundle: unknown; lookups?: AssetLookups }): AssetManifest {
	const root = (bundle ?? {}) as Json;
	const project = (root.project ?? {}) as Json;
	const uses: Uses = { media: new Map(), sounds: new Map(), stickers: new Map(), fonts: new Map() };
	if (project.scenes !== undefined) walk(project.scenes, uses);
	const media = (Array.isArray(root.media) ? root.media : []).map((item) => mediaEntry((item ?? {}) as Json, uses));
	const byId = new Map(media.map((entry) => [entry.id, { ...entry }]));
	const assets: AssetEntry[] = media.map((entry) => {
		const source = entry.derivedFrom !== undefined ? byId.get(entry.derivedFrom) : undefined;
		if (source && entry.origin === "generated") {
			entry.license = source.license;
			entry.licenseUrl = source.licenseUrl;
			entry.creator = source.creator;
			entry.sourceUrl = source.sourceUrl;
			entry.attribution = source.attribution;
			entry.creditRequired = source.creditRequired;
		}
		return entry;
	});
	for (const url of [...uses.sounds.keys()].sort(byCodeUnit)) {
		const used = uses.sounds.get(url)!;
		assets.push(soundEntry(url, used.name, used.count));
	}
	for (const id of [...uses.stickers.keys()].sort(byCodeUnit)) {
		const used = uses.stickers.get(id)!;
		assets.push(stickerEntry(id, used.name, used.count, lookups));
	}
	for (const family of [...uses.fonts.keys()].sort(byCodeUnit)) {
		assets.push(fontEntry(family, uses.fonts.get(family)!, lookups));
	}
	const cleaned = assets.map(clean);
	const credits: string[] = [];
	for (const entry of cleaned) {
		if (entry.attribution && !credits.includes(entry.attribution)) credits.push(entry.attribution);
	}
	const metadata = (project.metadata ?? {}) as Json;
	return {
		format: ASSET_MANIFEST_FORMAT,
		formatVersion: ASSET_MANIFEST_FORMAT_VERSION,
		project: { id: str(metadata.id) ?? null, name: str(metadata.name) ?? null },
		summary: {
			assets: cleaned.length,
			creditRequired: cleaned.filter((entry) => entry.creditRequired).length,
			licenseUnknown: cleaned.filter((entry) => entry.license === null && entry.origin !== "system").length,
		},
		credits,
		assets: cleaned,
	};
}

/** The prefixes of a bundle's icon stickers (emoji sets the manifest knows are left out) and its font families. */
function usedNames({ bundle }: { bundle: unknown }): { prefixes: string[]; fonts: string[] } {
	const uses: Uses = { media: new Map(), sounds: new Map(), stickers: new Map(), fonts: new Map() };
	walk(((bundle ?? {}) as Json).project, uses);
	const prefixes = new Set<string>();
	for (const id of uses.stickers.keys()) {
		const match = /^icons:([a-z0-9-]+):/.exec(id);
		if (match && match[1] !== "twemoji" && match[1] !== "noto") prefixes.add(match[1]);
	}
	return { prefixes: [...prefixes].sort(byCodeUnit), fonts: [...uses.fonts.keys()].filter((family) => !SYSTEM_FONTS.has(family)) };
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T | undefined> {
	return Promise.race([promise, new Promise<undefined>((resolve) => setTimeout(() => resolve(undefined), ms))]);
}

/**
 * Looks up what it can for a bundle: brand logos from `brands.json`, the
 * icon sets it uses from the Iconify API (one request) and its fonts from
 * the Fontsource catalogue. Anything that fails or takes over a few seconds
 * stays unknown.
 */
export async function gatherAssetLookups({
	bundle,
	fetchImpl = fetch,
}: {
	bundle: unknown;
	fetchImpl?: typeof fetch;
}): Promise<AssetLookups> {
	// Loaded on demand, as the brands sticker pack loads it.
	const module = (await import("@/stickers/providers/brands.json")) as unknown as { default?: unknown };
	const data = (module.default ?? module) as { license: string; brands: [string, string, string, string, string, boolean][] };
	const brands: Record<string, BrandInfo> = {};
	for (const [slug, title, , , license, credit] of data.brands) {
		brands[slug] = { title, license: license || data.license, creditRequired: credit };
	}
	const { prefixes, fonts: families } = usedNames({ bundle });
	const iconSets: Record<string, IconSetInfo> = {};
	const fonts: Record<string, string> = {};
	await Promise.all([
		prefixes.length === 0
			? undefined
			: withTimeout(
					fetchImpl(`${ICONIFY_API}/collections?prefixes=${prefixes.join(",")}`)
						.then((response) => (response.ok ? response.json() : {}))
						.then((raw: Record<string, Json>) => {
							for (const [prefix, entry] of Object.entries(raw ?? {})) {
								const license = (entry?.license ?? {}) as Json;
								const author = (entry?.author ?? {}) as Json;
								iconSets[prefix] = {
									name: str(entry?.name) ?? prefix,
									license: str(license.spdx) ?? null,
									licenseUrl: str(license.url) ?? null,
									author: str(author.name) ?? null,
								};
							}
						})
						.catch(() => undefined),
					8000,
				),
		families.length === 0
			? undefined
			: withTimeout(
					fetchFontsourceCatalogue({ fetchImpl })
						.then((catalogue) => {
							const wanted = new Map(families.map((family) => [fontsourceSlug(family), family]));
							for (const family of catalogue) {
								const name = wanted.get(family.id);
								if (name !== undefined) fonts[name] = family.license;
							}
						})
						.catch(() => undefined),
					8000,
				),
	]);
	return { iconSets, brands, fonts };
}

/** The credit lines of a manifest, one per line. */
export function manifestCreditsText({ manifest }: { manifest: AssetManifest }): string {
	return manifest.credits.join("\n");
}
