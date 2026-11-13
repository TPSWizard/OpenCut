/**
 * Stock media through the API worker.
 *
 * Pixabay (images and videos) needs an API key, which stays here as the
 * PIXABAY_API_KEY secret. Its API rules (https://pixabay.com/api/docs/,
 * read 2026-11-12): requests are cached for 24 hours, search-result URLs
 * may only be shown temporarily (a picture that is used is downloaded
 * first), and the results show users where they come from.
 *
 * Openverse (CC-licensed images) works without a key and the browser calls
 * it directly; this worker only downloads an Openverse file when its own
 * host does not allow the browser to (looked up by id, never an arbitrary
 * URL, so this is not an open proxy).
 */

export type StockKind = "image" | "video";
export type StockProvider = "pixabay" | "openverse";

/** One search result, the same shape for every provider (the web app's `StockItem`). */
export interface StockItem {
	provider: StockProvider;
	id: string;
	kind: StockKind;
	title: string;
	/** Small preview, only for showing the result. */
	thumbnail: string;
	width: number;
	height: number;
	duration?: number;
	license: string;
	licenseUrl: string;
	creator: string;
	creatorUrl?: string;
	/** The item's page at the provider. */
	sourceUrl: string;
	/** A ready-made credit line. */
	attribution: string;
}

export const PIXABAY_LICENSE = "Pixabay Content License";
export const PIXABAY_LICENSE_URL = "https://pixabay.com/service/license-summary/";
const PIXABAY_CACHE_SECONDS = 24 * 60 * 60;
/** Files larger than this are refused (bytes). */
export const MAX_STOCK_FILE_BYTES = 200 * 1024 * 1024;

export interface PixabayImageHit {
	id: number;
	pageURL: string;
	tags: string;
	previewURL: string;
	webformatURL: string;
	largeImageURL: string;
	imageWidth: number;
	imageHeight: number;
	user: string;
	user_id: number;
}

interface PixabayVideoFile {
	url: string;
	width: number;
	height: number;
	size: number;
	thumbnail: string;
}

export interface PixabayVideoHit {
	id: number;
	pageURL: string;
	tags: string;
	duration: number;
	videos: { large?: PixabayVideoFile; medium?: PixabayVideoFile; small?: PixabayVideoFile; tiny?: PixabayVideoFile };
	user: string;
	user_id: number;
}

function pixabayTitle(tags: string): string {
	const first = tags.split(",")[0]?.trim();
	return first ? first.charAt(0).toUpperCase() + first.slice(1) : "Pixabay";
}

function pixabayCreatorUrl(user: string, userId: number): string {
	return `https://pixabay.com/users/${encodeURIComponent(user)}-${userId}/`;
}

export function normalizePixabayImage(hit: PixabayImageHit): StockItem {
	const title = pixabayTitle(hit.tags);
	return {
		provider: "pixabay",
		id: String(hit.id),
		kind: "image",
		title,
		thumbnail: hit.webformatURL || hit.previewURL,
		width: hit.imageWidth,
		height: hit.imageHeight,
		license: PIXABAY_LICENSE,
		licenseUrl: PIXABAY_LICENSE_URL,
		creator: hit.user,
		creatorUrl: pixabayCreatorUrl(hit.user, hit.user_id),
		sourceUrl: hit.pageURL,
		attribution: `"${title}" by ${hit.user} on Pixabay (${hit.pageURL}), ${PIXABAY_LICENSE}`,
	};
}

/** The video file to download: the 1080p one when there is one, else the next size down. */
export function pixabayVideoFile(hit: PixabayVideoHit): PixabayVideoFile | null {
	const { large, medium, small, tiny } = hit.videos ?? {};
	return [large, medium, small, tiny].find((file) => file?.url) ?? null;
}

export function normalizePixabayVideo(hit: PixabayVideoHit): StockItem {
	const title = pixabayTitle(hit.tags);
	const file = pixabayVideoFile(hit);
	const preview = hit.videos?.small ?? hit.videos?.tiny ?? file;
	return {
		provider: "pixabay",
		id: String(hit.id),
		kind: "video",
		title,
		thumbnail: preview?.thumbnail ?? "",
		width: file?.width ?? 0,
		height: file?.height ?? 0,
		duration: hit.duration,
		license: PIXABAY_LICENSE,
		licenseUrl: PIXABAY_LICENSE_URL,
		creator: hit.user,
		creatorUrl: pixabayCreatorUrl(hit.user, hit.user_id),
		sourceUrl: hit.pageURL,
		attribution: `"${title}" by ${hit.user} on Pixabay (${hit.pageURL}), ${PIXABAY_LICENSE}`,
	};
}

export function pixabayUrl({
	key,
	kind,
	q,
	page,
	perPage,
	id,
}: {
	key: string;
	kind: StockKind;
	q?: string;
	page?: number;
	perPage?: number;
	id?: string;
}): string {
	const params = new URLSearchParams({ key, safesearch: "true" });
	if (id) params.set("id", id);
	else {
		params.set("q", (q ?? "").slice(0, 100));
		params.set("page", String(page ?? 1));
		params.set("per_page", String(Math.min(200, Math.max(3, perPage ?? 30))));
	}
	return `https://pixabay.com/api/${kind === "video" ? "videos/" : ""}?${params.toString()}`;
}

/** The same URL without the key: what the 24-hour cache is keyed on. */
export function cacheKeyFor(url: string): string {
	const parsed = new URL(url);
	parsed.searchParams.delete("key");
	return `https://stock-cache.invalid${parsed.pathname}?${parsed.searchParams.toString()}`;
}

/** Fetches JSON through the Cache API, keeping it 24 hours (Pixabay's rule). */
export async function cachedJson<T>(url: string): Promise<{ ok: true; data: T } | { ok: false; status: number }> {
	const cache = (globalThis as unknown as { caches?: { default?: Cache } }).caches?.default;
	const key = new Request(cacheKeyFor(url));
	const hit = await cache?.match(key);
	if (hit) return { ok: true, data: (await hit.json()) as T };
	const upstream = await fetch(url);
	if (!upstream.ok) return { ok: false, status: upstream.status };
	const body = await upstream.text();
	await cache?.put(
		key,
		new Response(body, {
			headers: { "Content-Type": "application/json", "Cache-Control": `public, max-age=${PIXABAY_CACHE_SECONDS}` },
		}),
	);
	return { ok: true, data: JSON.parse(body) as T };
}

/** The file URL of an Openverse image, looked up by its id. */
export async function openverseFileUrl(id: string): Promise<string | null> {
	if (!/^[0-9a-f-]{36}$/i.test(id)) return null;
	const response = await fetch(`https://api.openverse.org/v1/images/${id}/`);
	if (!response.ok) return null;
	const data = (await response.json()) as { url?: string };
	return typeof data.url === "string" && /^https?:\/\//.test(data.url) ? data.url : null;
}

/** Streams a file back with its type, refusing very large ones. */
export async function streamFile(url: string, headers: Record<string, string>): Promise<Response> {
	const upstream = await fetch(url, { headers: { "User-Agent": "ConzaCut stock import" } });
	if (!upstream.ok || !upstream.body) {
		return new Response(JSON.stringify({ error: "The file could not be fetched" }), {
			status: 502,
			headers: { ...headers, "Content-Type": "application/json" },
		});
	}
	const length = Number(upstream.headers.get("Content-Length") ?? 0);
	if (length > MAX_STOCK_FILE_BYTES) {
		return new Response(JSON.stringify({ error: "The file is too large" }), {
			status: 413,
			headers: { ...headers, "Content-Type": "application/json" },
		});
	}
	return new Response(upstream.body, {
		status: 200,
		headers: {
			...headers,
			"Content-Type": upstream.headers.get("Content-Type") ?? "application/octet-stream",
			...(length ? { "Content-Length": String(length) } : {}),
			"Cache-Control": "private, max-age=3600",
		},
	});
}
