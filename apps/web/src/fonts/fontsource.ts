/**
 * Fontsource: a keyless catalogue of open-licence fonts (OFL, Apache,
 * MIT, …) with files on jsDelivr. The catalogue lists every family with
 * its licence; a family loads as `@font-face` rules built from the API's
 * pinned file URLs, so nothing is fetched from Google at runtime. The
 * desktop fetches the same families' TTF files into `~/.opencut/fonts`.
 *
 * API: https://api.fontsource.org/v1/fonts (list) and
 * https://api.fontsource.org/v1/fonts/{id} (variants). Both are public
 * and rate limited to 2500 requests per 10 seconds.
 */

export const FONTSOURCE_API = "https://api.fontsource.org/v1/fonts";

export interface FontsourceFamily {
	id: string;
	family: string;
	category: string;
	/** SPDX identifier the API reports, such as "OFL-1.1" or "Apache-2.0". */
	license: string;
	weights: number[];
	styles: string[];
	defSubset: string;
	variable: boolean;
	subsets: string[];
}

interface FontsourceVariants {
	[weight: string]: {
		[style: string]: {
			[subset: string]: { url: { woff2?: string; woff?: string; ttf?: string } };
		};
	};
}

export interface FontsourceFamilyDetail extends FontsourceFamily {
	version: string;
	variants: FontsourceVariants;
}

/** Human labels for the licences Fontsource reports. */
export const FONT_LICENSE_LABELS: Record<string, string> = {
	"OFL-1.1": "SIL Open Font License 1.1",
	"Apache-2.0": "Apache 2.0",
	MIT: "MIT",
	"UFL-1.0": "Ubuntu Font Licence 1.0",
	"CC0-1.0": "CC0",
	"LGPL-2.1": "LGPL 2.1",
	"GPL-2.0": "GPL 2.0",
};

/** Licences that allow embedding the font in a video without conditions beyond keeping the licence file. */
export const CLEAR_FONT_LICENSES = new Set(["OFL-1.1", "Apache-2.0", "MIT", "UFL-1.0", "CC0-1.0"]);

export function describeFontLicense({ license }: { license: string }): string {
	return FONT_LICENSE_LABELS[license] ?? license;
}

/** Fontsource ids are the family lower-cased with spaces as dashes. */
export function fontsourceId({ family }: { family: string }): string {
	return family
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-|-$/g, "");
}

let cataloguePromise: Promise<FontsourceFamily[]> | null = null;
const detailCache = new Map<string, Promise<FontsourceFamilyDetail>>();
const loadedFamilies = new Set<string>();

export function clearFontsourceCache(): void {
	cataloguePromise = null;
	detailCache.clear();
	loadedFamilies.clear();
}

function parseFamily(raw: unknown): FontsourceFamily | null {
	if (!raw || typeof raw !== "object") return null;
	const entry = raw as Record<string, unknown>;
	if (typeof entry.id !== "string" || typeof entry.family !== "string") return null;
	return {
		id: entry.id,
		family: entry.family,
		category: typeof entry.category === "string" ? entry.category : "other",
		license: typeof entry.license === "string" ? entry.license : "unknown",
		weights: Array.isArray(entry.weights) ? entry.weights.filter((w): w is number => typeof w === "number") : [],
		styles: Array.isArray(entry.styles) ? entry.styles.filter((s): s is string => typeof s === "string") : [],
		defSubset: typeof entry.defSubset === "string" ? entry.defSubset : "latin",
		variable: entry.variable === true,
		subsets: Array.isArray(entry.subsets) ? entry.subsets.filter((s): s is string => typeof s === "string") : [],
	};
}

/** Every family Fontsource offers, fetched once per session. */
export function fetchFontsourceCatalogue({
	fetchImpl = fetch,
}: { fetchImpl?: typeof fetch } = {}): Promise<FontsourceFamily[]> {
	if (!cataloguePromise) {
		cataloguePromise = fetchImpl(FONTSOURCE_API)
			.then(async (response) => {
				if (!response.ok) throw new Error(`Fontsource catalogue: HTTP ${response.status}`);
				const raw: unknown = await response.json();
				if (!Array.isArray(raw)) throw new Error("Fontsource catalogue: unexpected shape");
				return raw.map(parseFamily).filter((entry): entry is FontsourceFamily => entry !== null);
			})
			.catch((error) => {
				cataloguePromise = null;
				throw error;
			});
	}
	return cataloguePromise;
}

export function fetchFontsourceFamily({
	id,
	fetchImpl = fetch,
}: {
	id: string;
	fetchImpl?: typeof fetch;
}): Promise<FontsourceFamilyDetail> {
	let pending = detailCache.get(id);
	if (!pending) {
		pending = fetchImpl(`${FONTSOURCE_API}/${encodeURIComponent(id)}`)
			.then(async (response) => {
				if (!response.ok) throw new Error(`Fontsource family ${id}: HTTP ${response.status}`);
				const raw = (await response.json()) as Record<string, unknown>;
				const family = parseFamily(raw);
				if (!family || !raw.variants || typeof raw.variants !== "object") {
					throw new Error(`Fontsource family ${id}: unexpected shape`);
				}
				return {
					...family,
					version: typeof raw.version === "string" ? raw.version : "latest",
					variants: raw.variants as FontsourceVariants,
				};
			})
			.catch((error) => {
				detailCache.delete(id);
				throw error;
			});
		detailCache.set(id, pending);
	}
	return pending;
}

export interface FontFaceSource {
	weight: number;
	style: string;
	url: string;
	format: "woff2" | "woff" | "truetype";
}

/**
 * The files to declare for a family: the requested weights in both styles
 * where they exist, from the default subset, preferring woff2. A weight
 * the family lacks falls back to its nearest available weight.
 */
export function selectFontFaces({
	detail,
	weights = [400, 700],
}: {
	detail: Pick<FontsourceFamilyDetail, "variants" | "defSubset" | "weights" | "styles">;
	weights?: number[];
}): FontFaceSource[] {
	const available = detail.weights.length > 0 ? detail.weights : Object.keys(detail.variants).map(Number);
	const chosen = new Set<number>();
	for (const wanted of weights) {
		const nearest = [...available].sort((a, b) => Math.abs(a - wanted) - Math.abs(b - wanted))[0];
		if (nearest !== undefined) chosen.add(nearest);
	}
	const faces: FontFaceSource[] = [];
	for (const weight of chosen) {
		const byStyle = detail.variants[String(weight)];
		if (!byStyle) continue;
		for (const style of ["normal", "italic"]) {
			const subsetUrls = byStyle[style]?.[detail.defSubset]?.url ?? byStyle[style]?.latin?.url;
			if (!subsetUrls) continue;
			if (subsetUrls.woff2) faces.push({ weight, style, url: subsetUrls.woff2, format: "woff2" });
			else if (subsetUrls.woff) faces.push({ weight, style, url: subsetUrls.woff, format: "woff" });
			else if (subsetUrls.ttf) faces.push({ weight, style, url: subsetUrls.ttf, format: "truetype" });
		}
	}
	return faces;
}

export function buildFontFaceCss({
	family,
	faces,
}: {
	family: string;
	faces: FontFaceSource[];
}): string {
	const safeFamily = family.replace(/["\\]/g, "");
	return faces
		.map(
			(face) =>
				`@font-face{font-family:"${safeFamily}";font-style:${face.style};font-weight:${face.weight};font-display:swap;src:url("${face.url}") format("${face.format}");}`,
		)
		.join("\n");
}

/**
 * Loads a Fontsource family into the document: declares its faces and
 * waits for them, so the canvas can draw with it. Resolves to the family's
 * licence. Safe to call again; later calls return at once.
 */
export async function loadFontsourceFont({
	family,
	weights,
	fetchImpl = fetch,
}: {
	family: string;
	weights?: number[];
	fetchImpl?: typeof fetch;
}): Promise<{ license: string; faces: FontFaceSource[] }> {
	const id = fontsourceId({ family });
	const detail = await fetchFontsourceFamily({ id, fetchImpl });
	const faces = selectFontFaces({ detail, weights });
	if (loadedFamilies.has(id) || typeof document === "undefined") {
		return { license: detail.license, faces };
	}
	const style = document.createElement("style");
	style.dataset.fontsource = id;
	style.textContent = buildFontFaceCss({ family: detail.family, faces });
	document.head.appendChild(style);
	loadedFamilies.add(id);
	await Promise.all(
		faces.map((face) =>
			document.fonts.load(`${face.style} ${face.weight} 16px "${detail.family.replace(/"/g, '\\"')}"`).catch(() => []),
		),
	);
	return { license: detail.license, faces };
}

export function isFontsourceLoaded({ family }: { family: string }): boolean {
	return loadedFamilies.has(fontsourceId({ family }));
}
