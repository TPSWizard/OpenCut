/**
 * Project sharing: a project bundle uploaded to R2 in parts through this
 * worker (each part under the Workers request body limit), then served at
 * `/share/<id>` for 7 days. Uploading needs the passcode set as the
 * `SHARE_UPLOAD_KEY` secret; opening a share needs only its unguessable
 * id. The bucket's lifecycle rule deletes shares after 7 days and aborts
 * unfinished uploads after a day; the worker also refuses shares older
 * than 7 days itself.
 *
 *   POST /share/start     { size, name? }               → { id, uploadId, partSize, parts, expiresAt }
 *   PUT  /share/part?id=&uploadId=&part=N   (bytes)    → { partNumber, etag }
 *   POST /share/complete  { id, uploadId, parts: [{ partNumber, etag }] } → { id, url, bytes, expiresAt }
 *   POST /share/abort     { id, uploadId }               → { aborted: true }
 *   GET  /share/<id>                                     → the bundle
 */

export const MAX_SHARE_BYTES = 2 * 1024 ** 3;
/** R2 wants every part but the last the same size, and at least 5 MiB. */
export const SHARE_PART_BYTES = 32 * 1024 ** 2;
export const SHARE_DAYS = 7;
const SHARE_PREFIX = "shares/";
const DAY_MS = 24 * 60 * 60 * 1000;

export type ShareEnv = {
	SHARES?: R2Bucket;
	SHARE_UPLOAD_KEY?: string;
};

export function newShareId(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function isShareId(id: string): boolean {
	return /^[A-Za-z0-9_-]{22}$/.test(id);
}

export function shareKey(id: string): string {
	return `${SHARE_PREFIX}${id}.opencut.zip`;
}

export function partCount(size: number): number {
	return Math.max(1, Math.ceil(size / SHARE_PART_BYTES));
}

/** Compares the given passcode with the secret without leaking where they differ. */
export async function passcodeMatches(given: string | null, expected: string): Promise<boolean> {
	if (!given) return false;
	const encoder = new TextEncoder();
	const [a, b] = await Promise.all([
		crypto.subtle.digest("SHA-256", encoder.encode(given)),
		crypto.subtle.digest("SHA-256", encoder.encode(expected)),
	]);
	const left = new Uint8Array(a);
	const right = new Uint8Array(b);
	let difference = 0;
	for (let index = 0; index < left.length; index++) difference |= left[index] ^ right[index];
	return difference === 0;
}

/** A file name safe for Content-Disposition. */
export function bundleFileName(name: string | undefined): string {
	const cleaned = (name ?? "")
		.replace(/[^\p{L}\p{N} ._-]+/gu, "")
		.trim()
		.slice(0, 80);
	return `${cleaned || "project"}.opencut.zip`;
}

export function isExpired(uploaded: Date, now = Date.now()): boolean {
	return now - uploaded.getTime() > SHARE_DAYS * DAY_MS;
}

function json(body: unknown, status: number, headers: Record<string, string>): Response {
	return new Response(JSON.stringify(body), { status, headers: { ...headers, "Content-Type": "application/json" } });
}

async function readJson(request: Request): Promise<Record<string, unknown> | null> {
	try {
		const value = await request.json();
		return value && typeof value === "object" ? (value as Record<string, unknown>) : null;
	} catch {
		return null;
	}
}

/** Handles `/share/*`; `cors` are the headers every answer carries. */
export async function handleShare(request: Request, env: ShareEnv, cors: Record<string, string>): Promise<Response> {
	const url = new URL(request.url);
	const bucket = env.SHARES;
	if (!bucket) return json({ error: "Sharing is not set up on this server" }, 503, cors);

	if (request.method === "GET" || request.method === "HEAD") {
		const id = url.pathname.slice("/share/".length);
		if (!isShareId(id)) return json({ error: "No such share" }, 404, cors);
		const object = request.method === "HEAD" ? await bucket.head(shareKey(id)) : await bucket.get(shareKey(id));
		if (!object) return json({ error: "No such share, or it has expired" }, 404, cors);
		if (isExpired(object.uploaded)) {
			await bucket.delete(shareKey(id));
			return json({ error: "This share has expired" }, 410, cors);
		}
		const expires = new Date(object.uploaded.getTime() + SHARE_DAYS * DAY_MS);
		const headers: Record<string, string> = {
			...cors,
			"Content-Type": "application/zip",
			"Content-Length": String(object.size),
			"Content-Disposition": `attachment; filename="${bundleFileName(object.customMetadata?.name)}"`,
			"Cache-Control": "private, max-age=3600",
			"X-Share-Expires": expires.toISOString(),
			ETag: object.httpEtag,
		};
		return new Response(request.method === "HEAD" ? null : (object as R2ObjectBody).body, { headers });
	}

	const expected = env.SHARE_UPLOAD_KEY;
	if (!expected) return json({ error: "Sharing needs SHARE_UPLOAD_KEY set on this server" }, 503, cors);
	if (!(await passcodeMatches(request.headers.get("X-Share-Key"), expected))) {
		return json({ error: "Wrong sharing passcode" }, 401, cors);
	}

	if (request.method === "POST" && url.pathname === "/share/start") {
		const body = await readJson(request);
		const size = typeof body?.size === "number" ? body.size : Number.NaN;
		if (!Number.isInteger(size) || size <= 0) return json({ error: "size must be the bundle's byte count" }, 400, cors);
		if (size > MAX_SHARE_BYTES) return json({ error: "The bundle is larger than 2 GB" }, 413, cors);
		const name = typeof body?.name === "string" ? body.name.slice(0, 120) : "";
		const id = newShareId();
		const upload = await bucket.createMultipartUpload(shareKey(id), {
			httpMetadata: { contentType: "application/zip" },
			customMetadata: { name, size: String(size) },
		});
		return json(
			{
				id,
				uploadId: upload.uploadId,
				partSize: SHARE_PART_BYTES,
				parts: partCount(size),
				expiresAt: new Date(Date.now() + SHARE_DAYS * DAY_MS).toISOString(),
			},
			200,
			cors,
		);
	}

	if (request.method === "PUT" && url.pathname === "/share/part") {
		const id = url.searchParams.get("id") ?? "";
		const uploadId = url.searchParams.get("uploadId") ?? "";
		const partNumber = Number(url.searchParams.get("part"));
		const length = Number(request.headers.get("Content-Length"));
		if (!isShareId(id) || !uploadId || !Number.isInteger(partNumber) || partNumber < 1 || partNumber > partCount(MAX_SHARE_BYTES)) {
			return json({ error: "id, uploadId and part are required" }, 400, cors);
		}
		if (!Number.isFinite(length) || length <= 0 || length > SHARE_PART_BYTES || !request.body) {
			return json({ error: `Each part is at most ${SHARE_PART_BYTES} bytes, with a Content-Length` }, 400, cors);
		}
		const upload = bucket.resumeMultipartUpload(shareKey(id), uploadId);
		const part = await upload.uploadPart(partNumber, request.body);
		return json({ partNumber: part.partNumber, etag: part.etag }, 200, cors);
	}

	if (request.method === "POST" && url.pathname === "/share/complete") {
		const body = await readJson(request);
		const id = typeof body?.id === "string" ? body.id : "";
		const uploadId = typeof body?.uploadId === "string" ? body.uploadId : "";
		const parts = Array.isArray(body?.parts) ? (body.parts as Array<{ partNumber?: unknown; etag?: unknown }>) : [];
		const valid = parts.every((part) => typeof part.partNumber === "number" && typeof part.etag === "string");
		if (!isShareId(id) || !uploadId || parts.length === 0 || !valid) {
			return json({ error: "id, uploadId and parts are required" }, 400, cors);
		}
		const upload = bucket.resumeMultipartUpload(shareKey(id), uploadId);
		const object = await upload.complete(parts as R2UploadedPart[]);
		if (object.size > MAX_SHARE_BYTES) {
			await bucket.delete(shareKey(id));
			return json({ error: "The bundle is larger than 2 GB" }, 413, cors);
		}
		return json(
			{
				id,
				url: `${url.origin}/share/${id}`,
				bytes: object.size,
				expiresAt: new Date(object.uploaded.getTime() + SHARE_DAYS * DAY_MS).toISOString(),
			},
			200,
			cors,
		);
	}

	if (request.method === "POST" && url.pathname === "/share/abort") {
		const body = await readJson(request);
		const id = typeof body?.id === "string" ? body.id : "";
		const uploadId = typeof body?.uploadId === "string" ? body.uploadId : "";
		if (!isShareId(id) || !uploadId) return json({ error: "id and uploadId are required" }, 400, cors);
		await bucket.resumeMultipartUpload(shareKey(id), uploadId).abort();
		return json({ aborted: true }, 200, cors);
	}

	return json({ error: "Not found" }, 404, cors);
}
