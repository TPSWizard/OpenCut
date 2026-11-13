/**
 * "Open in ConzaCut" links: what bundle URL a link points at, and fetching
 * it. Both editors read links by the same rules (the desktop's
 * `crates/editor/src/open_link.rs`, checked against the same cases in
 * `fixtures/open-links/cases.json`):
 *
 * - a bundle URL itself: `https://…`, or `http://` only on this computer
 *   (localhost, 127.0.0.1, [::1]);
 * - a ConzaCut open link wrapping one: `https://<app>/open?bundle=<url>`
 *   or `conzacut://open?bundle=<url>` (one level; a wrapped open link is
 *   refused).
 *
 * Anything with credentials, spaces, control characters or a malformed
 * host is refused. The URL is returned as given (trimmed), not rewritten.
 */

const MAX_LINK_LENGTH = 4096;
/** Bundles larger than this are not downloaded into the page. */
export const MAX_LINKED_BUNDLE_BYTES = 2 * 1024 ** 3;

interface Parts {
	scheme: string;
	authority: string;
	path: string;
	query: string | null;
}

function split(url: string): Parts | null {
	const marker = url.indexOf("://");
	if (marker === -1) return null;
	const scheme = url.slice(0, marker);
	const rest = url.slice(marker + 3);
	const authorityEnd = rest.search(/[/?#]/);
	const authority = authorityEnd === -1 ? rest : rest.slice(0, authorityEnd);
	const after = authorityEnd === -1 ? "" : rest.slice(authorityEnd);
	const pathEnd = after.search(/[?#]/);
	const path = pathEnd === -1 ? after : after.slice(0, pathEnd);
	const tail = pathEnd === -1 ? "" : after.slice(pathEnd);
	const query = tail.startsWith("?") ? tail.slice(1).split("#")[0] : null;
	return { scheme, authority, path, query };
}

function validHost(authority: string): string | null {
	let host: string;
	let port: string | null;
	if (authority.startsWith("[")) {
		const close = authority.indexOf("]");
		if (close === -1) return null;
		const inside = authority.slice(1, close);
		if (!inside || !/^[0-9a-fA-F:.]+$/.test(inside)) return null;
		host = authority.slice(0, close + 1);
		const after = authority.slice(close + 1);
		if (after === "") port = null;
		else if (after.startsWith(":")) port = after.slice(1);
		else return null;
	} else {
		const colon = authority.indexOf(":");
		host = colon === -1 ? authority : authority.slice(0, colon);
		port = colon === -1 ? null : authority.slice(colon + 1);
		if (!/^[A-Za-z0-9.-]+$/.test(host)) return null;
	}
	if (!host) return null;
	if (port !== null && !/^[0-9]{1,5}$/.test(port)) return null;
	return host;
}

function direct(url: string): string | null {
	// Whitespace and control characters (C0, DEL, C1).
	if (!url || /[\s\u0000-\u001f\u007f-\u009f]/.test(url)) return null;
	const parts = split(url);
	if (!parts || parts.authority.includes("@")) return null;
	const host = validHost(parts.authority)?.toLowerCase();
	if (!host) return null;
	const scheme = parts.scheme.toLowerCase();
	if (scheme === "https") return url;
	if (scheme === "http" && (host === "localhost" || host === "127.0.0.1" || host === "[::1]")) return url;
	return null;
}

function percentDecode(value: string): string | null {
	const bytes: number[] = [];
	for (let index = 0; index < value.length; ) {
		const char = value[index];
		if (char === "%") {
			const hex = value.slice(index + 1, index + 3);
			if (!/^[0-9a-fA-F]{2}$/.test(hex)) return null;
			bytes.push(Number.parseInt(hex, 16));
			index += 3;
		} else if (char === "+") {
			bytes.push(0x20);
			index += 1;
		} else {
			for (const byte of new TextEncoder().encode(char)) bytes.push(byte);
			index += 1;
		}
	}
	try {
		return new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(bytes));
	} catch {
		return null;
	}
}

/** `undefined`: not an open link; `null`: an open link without a bundle. */
function openLinkBundle(url: string): string | null | undefined {
	const parts = split(url);
	if (!parts) return undefined;
	const wrapper = parts.scheme.toLowerCase() === "conzacut" || parts.path.endsWith("/open");
	if (!wrapper) return undefined;
	for (const pair of (parts.query ?? "").split("&")) {
		if (pair.startsWith("bundle=")) return pair.slice("bundle=".length);
	}
	return null;
}

/** The bundle URL a link points at, or `null` when the link is refused. */
export function bundleUrlFromLink(link: string): string | null {
	const trimmed = link.trim();
	if (!trimmed || trimmed.length > MAX_LINK_LENGTH) return null;
	const wrapped = openLinkBundle(trimmed);
	if (wrapped === null) return null;
	if (wrapped === undefined) return direct(trimmed);
	const inner = percentDecode(wrapped);
	if (inner === null || openLinkBundle(inner) !== undefined) return null;
	return direct(inner);
}

/** The open link for a bundle URL on this app (what to put behind an "Open in ConzaCut" button). */
export function openLinkFor({ bundleUrl, origin }: { bundleUrl: string; origin: string }): string {
	return `${origin.replace(/\/$/, "")}/open?bundle=${encodeURIComponent(bundleUrl)}`;
}

/** Downloads a bundle with progress (0–1 when the size is known), refusing ones over the limit. */
export async function fetchLinkedBundle({
	url,
	onProgress,
	fetchImpl = fetch,
	maxBytes = MAX_LINKED_BUNDLE_BYTES,
}: {
	url: string;
	onProgress?: (progress: { received: number; total: number | null }) => void;
	fetchImpl?: typeof fetch;
	maxBytes?: number;
}): Promise<Blob> {
	let response: Response;
	try {
		response = await fetchImpl(url, { credentials: "omit", redirect: "follow" });
	} catch {
		throw new Error("The bundle could not be reached. Its server must allow this app to download it (CORS).");
	}
	if (!response.ok) throw new Error(`The bundle could not be downloaded (${response.status})`);
	const length = Number(response.headers.get("content-length"));
	const total = Number.isFinite(length) && length > 0 ? length : null;
	if (total !== null && total > maxBytes) throw new Error("The bundle is too large to open here");
	if (!response.body) return response.blob();
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let received = 0;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		received += value.byteLength;
		if (received > maxBytes) {
			await reader.cancel();
			throw new Error("The bundle is too large to open here");
		}
		chunks.push(value);
		onProgress?.({ received, total });
	}
	return new Blob(chunks as BlobPart[], { type: "application/zip" });
}
