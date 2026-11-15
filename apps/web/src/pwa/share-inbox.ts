/**
 * Files shared to the installed app from another app's share sheet (the
 * manifest's `share_target`). The service worker (`sw.js`) receives the
 * POST, keeps each file in this cache, and opens `/projects?shared=<n>`;
 * the projects page makes a project, and the editor takes the files out
 * and lays them on the timeline in the order they were shared.
 */

/** Must match `SHARE_INBOX` in `sw.js`. */
export const SHARE_INBOX_CACHE = "share-inbox";
/** The project the shared files go into, set by the projects page. */
const PENDING_KEY = "conzacut-share-pending";

/** Takes the shared files out of the inbox (oldest first) and empties it. */
export async function takeSharedFiles(): Promise<File[]> {
	if (typeof caches === "undefined") return [];
	const cache = await caches.open(SHARE_INBOX_CACHE);
	const requests = await cache.keys();
	const entries = await Promise.all(
		requests.map(async (request) => {
			const response = await cache.match(request);
			if (!response) return null;
			const blob = await response.blob();
			const index = Number(new URL(request.url).searchParams.get("i") ?? "0");
			const name = decodeURIComponent(
				response.headers.get("X-File-Name") ?? `shared-${index + 1}`,
			);
			const file = new File([blob], name, {
				type: response.headers.get("Content-Type") ?? blob.type,
				lastModified: Number(response.headers.get("X-Last-Modified")) || Date.now(),
			});
			return { index, file };
		}),
	);
	await caches.delete(SHARE_INBOX_CACHE);
	return entries
		.filter((entry): entry is { index: number; file: File } => entry !== null)
		.sort((a, b) => a.index - b.index)
		.map((entry) => entry.file);
}

export function markSharePending(projectId: string): void {
	try {
		sessionStorage.setItem(PENDING_KEY, projectId);
	} catch {
		// Storage refused: the files stay in the inbox for the next try.
	}
}

/** True once for the project the projects page made for a share. */
export function claimSharePending(projectId: string): boolean {
	try {
		if (sessionStorage.getItem(PENDING_KEY) !== projectId) return false;
		sessionStorage.removeItem(PENDING_KEY);
		return true;
	} catch {
		return false;
	}
}
