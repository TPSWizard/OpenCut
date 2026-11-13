/**
 * Sharing a project by link: its bundle is uploaded in parts to the API
 * worker (`apps/api/src/share.ts`, R2 behind it, passcode required) and
 * kept for 7 days; the link to give out is an "Open in ConzaCut" link
 * (`/open?bundle=<share URL>`), so whoever opens it sees what it is and
 * confirms before it becomes a project. The desktop shares the same way
 * (`apps/desktop/src/share.rs`).
 */

import type { EditorCore } from "@/core";
import { apiUrl } from "@/lib/api";
import { openLinkFor } from "./open-link";

export interface ShareResult {
	/** The bundle itself (what `/open` downloads). */
	bundleUrl: string;
	/** The link to give out. */
	link: string;
	bytes: number;
	expiresAt: string;
}

export class ShareError extends Error {
	constructor(
		message: string,
		readonly status: number,
	) {
		super(message);
	}
}

const PASSCODE_STORAGE_KEY = "conzacut.sharePasscode";
const RECENT_STORAGE_KEY = "conzacut.recentShares";

export function rememberedPasscode(): string {
	try {
		return localStorage.getItem(PASSCODE_STORAGE_KEY) ?? "";
	} catch {
		return "";
	}
}

export function rememberPasscode({ passcode }: { passcode: string | null }): void {
	try {
		if (passcode) localStorage.setItem(PASSCODE_STORAGE_KEY, passcode);
		else localStorage.removeItem(PASSCODE_STORAGE_KEY);
	} catch {
		// Private windows may refuse storage; the passcode is then asked again.
	}
}

export interface RecentShare {
	name: string;
	link: string;
	expiresAt: string;
}

/** Shares made in this browser that have not expired, newest first. */
export function recentShares({ now = Date.now() }: { now?: number } = {}): RecentShare[] {
	try {
		const list = JSON.parse(localStorage.getItem(RECENT_STORAGE_KEY) ?? "[]") as RecentShare[];
		return Array.isArray(list) ? list.filter((share) => Date.parse(share.expiresAt) > now) : [];
	} catch {
		return [];
	}
}

function addRecentShare({ share }: { share: RecentShare }): void {
	try {
		const list = [share, ...recentShares()].slice(0, 10);
		localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify(list));
	} catch {
		// Not remembered; the link was still shown.
	}
}

async function call<T>({
	fetchImpl,
	path,
	passcode,
	init,
}: {
	fetchImpl: typeof fetch;
	path: string;
	passcode: string;
	init: RequestInit;
}): Promise<T> {
	let response: Response;
	try {
		response = await fetchImpl(apiUrl(path), { ...init, headers: { ...(init.headers ?? {}), "X-Share-Key": passcode } });
	} catch {
		throw new ShareError("The sharing server could not be reached", 0);
	}
	const body = (await response.json().catch(() => ({}))) as T & { error?: string };
	if (!response.ok) {
		const message =
			response.status === 401
				? "Wrong sharing passcode"
				: response.status === 503
					? "Sharing is not set up on the server yet"
					: response.status === 413
						? "The project is larger than 2 GB, the most a share can hold"
						: (body.error ?? `Sharing failed (${response.status})`);
		throw new ShareError(message, response.status);
	}
	return body;
}

/** Uploads a bundle as a share (parts, with retries) and returns its link. */
export async function shareBundle({
	blob,
	name,
	passcode,
	onProgress,
	fetchImpl = fetch,
	origin = typeof window === "undefined" ? "https://conzacut.conzahub.com" : window.location.origin,
}: {
	blob: Blob;
	name: string;
	passcode: string;
	onProgress?: (share: number) => void;
	fetchImpl?: typeof fetch;
	origin?: string;
}): Promise<ShareResult> {
	const json = { "Content-Type": "application/json" };
	const start = await call<{ id: string; uploadId: string; partSize: number; parts: number }>({
		fetchImpl,
		path: "/share/start",
		passcode,
		init: { method: "POST", headers: json, body: JSON.stringify({ size: blob.size, name }) },
	});
	const parts: Array<{ partNumber: number; etag: string }> = [];
	try {
		for (let index = 0; index < start.parts; index++) {
			const chunk = blob.slice(index * start.partSize, Math.min(blob.size, (index + 1) * start.partSize));
			const query = new URLSearchParams({ id: start.id, uploadId: start.uploadId, part: String(index + 1) });
			let lastError: unknown;
			for (let attempt = 0; attempt < 3; attempt++) {
				try {
					parts.push(await call({ fetchImpl, path: `/share/part?${query}`, passcode, init: { method: "PUT", body: chunk } }));
					lastError = undefined;
					break;
				} catch (error) {
					lastError = error;
					if (error instanceof ShareError && error.status >= 400 && error.status < 500) break;
				}
			}
			if (lastError) throw lastError;
			onProgress?.((index + 1) / start.parts);
		}
		const done = await call<{ url: string; bytes: number; expiresAt: string }>({
			fetchImpl,
			path: "/share/complete",
			passcode,
			init: { method: "POST", headers: json, body: JSON.stringify({ id: start.id, uploadId: start.uploadId, parts }) },
		});
		const result = { bundleUrl: done.url, link: openLinkFor({ bundleUrl: done.url, origin }), bytes: done.bytes, expiresAt: done.expiresAt };
		addRecentShare({ share: { name, link: result.link, expiresAt: result.expiresAt } });
		return result;
	} catch (error) {
		await call({
			fetchImpl,
			path: "/share/abort",
			passcode,
			init: { method: "POST", headers: json, body: JSON.stringify({ id: start.id, uploadId: start.uploadId }) },
		}).catch(() => undefined);
		throw error;
	}
}

/** Shares the open project (saved first) with its media. */
export async function shareCurrentProject({
	editor,
	passcode,
	onProgress,
}: {
	editor: EditorCore;
	passcode: string;
	onProgress?: (step: { step: "packing" } | { step: "uploading"; share: number }) => void;
}): Promise<ShareResult> {
	onProgress?.({ step: "packing" });
	const { blob } = await editor.project.exportProjectBundle();
	const name = editor.project.getActive()?.metadata.name ?? "Project";
	return shareBundle({ blob, name, passcode, onProgress: (share) => onProgress?.({ step: "uploading", share }) });
}
