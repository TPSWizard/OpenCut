/**
 * Face swap through Segmind's hosted HyperSwap models (FaceFusion Labs):
 * the editor uploads a face photo and the clip's span (a short video or a
 * picture) into R2 here, the worker hands Segmind short-lived links to
 * them, and the editor polls until the swapped clip is ready and downloads
 * it through the worker. Each run spends the server owner's Segmind
 * credit, so every call but the file links needs the server passcode
 * (`SHARE_UPLOAD_KEY`) as well as `SEGMIND_API_KEY`. The person running it
 * confirms they may use the face (`consent: true`); the uploads are
 * deleted once the job ends, and anything left is pruned after two hours.
 *
 *   POST /faceswap/jobs            { kind, faceIndex?, quality?, consent }   → { id }
 *   PUT  /faceswap/jobs/<id>/face    (image bytes)                            → { stored: "face", bytes }
 *   PUT  /faceswap/jobs/<id>/target  (video or image bytes)                   → { stored: "target", bytes }
 *   POST /faceswap/jobs/<id>/run                                              → { status }
 *   GET  /faceswap/jobs/<id>                                                  → { status, error? }
 *   GET  /faceswap/jobs/<id>/output                                           → the swapped video or picture
 *   GET  /faceswap/files/<id>/<face|target>   (no passcode: Segmind reads these)
 *
 * Segmind's contract (https://www.segmind.com/models/video-faceswap-by-facefusion-labs/api):
 * `x-api-key`; POST /v2/<model> with `source_image` and `target_video` or
 * `target_image` as URLs; poll /v2/requests/<id>/status for QUEUED,
 * PROCESSING, COMPLETED or FAILED; the result at /v2/requests/<id> has the
 * output's URL and is kept for an hour.
 */

import { isShareId, newShareId, passcodeMatches, publicBase } from "./share";

export type FaceSwapEnv = {
	/** Segmind API key (`wrangler secret put SEGMIND_API_KEY`). */
	SEGMIND_API_KEY?: string;
	SHARE_UPLOAD_KEY?: string;
	SHARES?: R2Bucket;
};

export type FaceSwapKind = "video" | "image";
export type FaceSwapQuality = "fast" | "best";
export type JobStatus = "uploading" | "queued" | "processing" | "done" | "failed";

export const SEGMIND_BASE = "https://api.segmind.com";
export const VIDEO_MODEL = "video-faceswap-by-facefusion-labs";
export const IMAGE_MODEL = "hyperswap-image-faceswap-by-facefusion-labs";
export const PROVIDER = "HyperSwap (FaceFusion Labs) via Segmind";
/** Under the Workers request body limit (100 MB). */
export const MAX_TARGET_BYTES = 90 * 1024 ** 2;
export const MAX_FACE_BYTES = 10 * 1024 ** 2;
export const JOB_TTL_MS = 2 * 60 * 60 * 1000;
const PREFIX = "faceswap/";

export interface Job {
	kind: FaceSwapKind;
	faceIndex: number;
	quality: FaceSwapQuality;
	createdAt: number;
	status: JobStatus;
	face?: { type: string; bytes: number };
	target?: { type: string; bytes: number };
	requestId?: string;
	statusUrl?: string;
	resultUrl?: string;
	outputUrl?: string;
	error?: string;
}

/** The new job's settings from the request body, or why they cannot be used. */
export function readJobRequest(body: unknown): Pick<Job, "kind" | "faceIndex" | "quality"> | string {
	const value = (body ?? {}) as Record<string, unknown>;
	if (value.consent !== true) {
		return "Confirm that the face is yours or that you have the person's permission to use it";
	}
	if (value.kind !== "video" && value.kind !== "image") return 'kind must be "video" or "image"';
	const faceIndex = value.faceIndex === undefined ? 0 : value.faceIndex;
	if (typeof faceIndex !== "number" || !Number.isInteger(faceIndex) || faceIndex < 0 || faceIndex > 10) {
		return "faceIndex must be a whole number from 0 to 10";
	}
	const quality = value.quality === undefined ? "best" : value.quality;
	if (quality !== "fast" && quality !== "best") return 'quality must be "fast" or "best"';
	return { kind: value.kind, faceIndex, quality };
}

/** The HyperSwap model: 1a is the fastest, 1c the most detailed (Segmind's notes). */
export function modelName(quality: FaceSwapQuality): string {
	return quality === "fast" ? "hyperswap_1a" : "hyperswap_1c";
}

/** What Segmind is asked to do, with links it can fetch the uploads from. */
export function segmindRequest({ job, base, id }: { job: Job; base: string; id: string }): { url: string; body: Record<string, unknown> } {
	const face = `${base}/faceswap/files/${id}/face`;
	const target = `${base}/faceswap/files/${id}/target`;
	if (job.kind === "video") {
		return {
			url: `${SEGMIND_BASE}/v2/${VIDEO_MODEL}`,
			body: {
				source_image: face,
				target_video: target,
				model_name: modelName(job.quality),
				target_face_index: job.faceIndex,
				face_detector_score: 0.5,
			},
		};
	}
	return {
		url: `${SEGMIND_BASE}/v2/${IMAGE_MODEL}`,
		body: { source_image: face, target_image: target, model_name: modelName(job.quality), output_format: "png" },
	};
}

/** Only Segmind's own API host is polled with the key. */
function isSegmindUrl(value: unknown): value is string {
	if (typeof value !== "string") return false;
	try {
		const url = new URL(value);
		return url.protocol === "https:" && url.hostname === "api.segmind.com";
	} catch {
		return false;
	}
}

/** The request id and the status and result URLs from Segmind's answer to a submit. */
export function readSubmit(value: unknown): { requestId: string; statusUrl: string; resultUrl: string } | null {
	const body = (value ?? {}) as Record<string, unknown>;
	const id = [body.request_id, body.requestId, body.id].find((item) => typeof item === "string" && item.length > 0) as string | undefined;
	if (!id || !/^[A-Za-z0-9_-]{1,200}$/.test(id)) return null;
	return {
		requestId: id,
		statusUrl: isSegmindUrl(body.status_url) ? body.status_url : `${SEGMIND_BASE}/v2/requests/${id}/status`,
		resultUrl: isSegmindUrl(body.response_url) ? body.response_url : `${SEGMIND_BASE}/v2/requests/${id}`,
	};
}

/** QUEUED, PROCESSING, COMPLETED or FAILED (any case) from a status answer. */
export function readStatus(value: unknown): "queued" | "processing" | "completed" | "failed" | null {
	const status = String(((value ?? {}) as Record<string, unknown>).status ?? "").toUpperCase();
	if (status === "QUEUED" || status === "IN_QUEUE" || status === "PENDING") return "queued";
	if (status === "PROCESSING" || status === "IN_PROGRESS" || status === "RUNNING") return "processing";
	if (status === "COMPLETED" || status === "SUCCEEDED" || status === "SUCCESS") return "completed";
	if (status === "FAILED" || status === "ERROR" || status === "CANCELLED") return "failed";
	return null;
}

/** The output's https URL from a finished result (a URL, a list of them, or an object holding one). */
export function readOutputUrl(value: unknown): string | null {
	const pick = (item: unknown, depth: number): string | null => {
		if (depth > 3 || item == null) return null;
		if (typeof item === "string") return /^https:\/\//.test(item) ? item : null;
		if (Array.isArray(item)) {
			for (const entry of item) {
				const found = pick(entry, depth + 1);
				if (found) return found;
			}
			return null;
		}
		if (typeof item === "object") {
			const record = item as Record<string, unknown>;
			for (const key of ["output", "url", "video", "image", "output_url", "result"]) {
				const found = pick(record[key], depth + 1);
				if (found) return found;
			}
		}
		return null;
	};
	return pick(value, 0);
}

/** A short reason from a failed answer, without anything secret. */
export function readError(value: unknown, fallback: string): string {
	const body = (value ?? {}) as Record<string, unknown>;
	for (const key of ["error", "detail", "message"]) {
		const item = body[key];
		if (typeof item === "string" && item.trim()) return item.trim().slice(0, 300);
		if (item && typeof item === "object") {
			const nested = (item as Record<string, unknown>).message;
			if (typeof nested === "string" && nested.trim()) return nested.trim().slice(0, 300);
		}
	}
	return fallback;
}

export function isFaceType(type: string): boolean {
	return /^image\/(png|jpeg|webp)$/.test(type);
}

export function isTargetType(kind: FaceSwapKind, type: string): boolean {
	return kind === "video" ? /^video\/(mp4|webm|quicktime)$/.test(type) : isFaceType(type);
}

function json(body: unknown, status: number, headers: Record<string, string>): Response {
	return new Response(JSON.stringify(body), { status, headers: { ...headers, "Content-Type": "application/json" } });
}

const jobKey = (id: string) => `${PREFIX}${id}/job.json`;
const fileKey = (id: string, part: "face" | "target") => `${PREFIX}${id}/${part}`;

async function readJob(bucket: R2Bucket, id: string): Promise<Job | null> {
	const object = await bucket.get(jobKey(id));
	if (!object) return null;
	try {
		return (await object.json()) as Job;
	} catch {
		return null;
	}
}

async function writeJob(bucket: R2Bucket, id: string, job: Job): Promise<void> {
	await bucket.put(jobKey(id), JSON.stringify(job), { httpMetadata: { contentType: "application/json" } });
}

async function deleteUploads(bucket: R2Bucket, id: string): Promise<void> {
	await bucket.delete([fileKey(id, "face"), fileKey(id, "target")]);
}

/** Deletes face swap jobs and uploads older than two hours; returns how many objects. */
export async function pruneOldJobs(bucket: Pick<R2Bucket, "list" | "delete">, now = Date.now()): Promise<number> {
	let removed = 0;
	let cursor: string | undefined;
	do {
		const page = await bucket.list({ prefix: PREFIX, limit: 1000, cursor });
		const old = page.objects.filter((object) => now - object.uploaded.getTime() > JOB_TTL_MS).map((object) => object.key);
		if (old.length > 0) await bucket.delete(old);
		removed += old.length;
		cursor = page.truncated ? page.cursor : undefined;
	} while (cursor);
	return removed;
}

export type Fetcher = (input: string, init?: RequestInit) => Promise<Response>;

async function segmind(fetcher: Fetcher, key: string, url: string, init: RequestInit = {}): Promise<{ status: number; body: unknown }> {
	const response = await fetcher(url, {
		...init,
		headers: { ...(init.headers as Record<string, string> | undefined), "x-api-key": key, Accept: "application/json" },
	});
	let body: unknown = null;
	try {
		body = await response.json();
	} catch {
		body = null;
	}
	return { status: response.status, body };
}

function providerProblem(status: number, body: unknown): { message: string; status: number } {
	if (status === 401 || status === 403) return { message: "Segmind refused the server's API key (SEGMIND_API_KEY)", status: 502 };
	if (status === 406) return { message: "The Segmind account is out of credits", status: 402 };
	if (status === 429) return { message: "Segmind is busy; try again in a minute", status: 429 };
	return { message: readError(body, `Segmind answered ${status}`), status: 502 };
}

/** Handles `/faceswap/*`; `cors` are the headers every answer carries. */
export async function handleFaceSwap(
	request: Request,
	env: FaceSwapEnv,
	cors: Record<string, string>,
	fetcher: Fetcher = (input, init) => fetch(input, init),
	now: () => number = Date.now,
): Promise<Response> {
	const url = new URL(request.url);
	const parts = url.pathname.split("/").filter(Boolean); // faceswap, ...
	const bucket = env.SHARES;

	// The uploads, for Segmind to fetch: the unguessable job id is the key.
	if (parts[1] === "files" && request.method === "GET") {
		const [, , id, part] = parts;
		if (!bucket || !id || !isShareId(id) || (part !== "face" && part !== "target")) {
			return new Response("Not found", { status: 404 });
		}
		const object = await bucket.get(fileKey(id, part));
		if (!object) return new Response("Not found", { status: 404 });
		return new Response(object.body, {
			headers: {
				"Content-Type": object.httpMetadata?.contentType ?? "application/octet-stream",
				"Content-Length": String(object.size),
				"Cache-Control": "no-store",
				"X-Robots-Tag": "noindex",
			},
		});
	}

	if (!bucket) return json({ error: "Face swap is not set up on this server (no storage)" }, 503, cors);
	const key = env.SEGMIND_API_KEY;
	if (!key) return json({ error: "Face swap is not set up on this server (SEGMIND_API_KEY)" }, 503, cors);
	if (!env.SHARE_UPLOAD_KEY) return json({ error: "Face swap needs the server passcode (SHARE_UPLOAD_KEY)" }, 503, cors);
	if (!(await passcodeMatches(request.headers.get("X-Share-Key"), env.SHARE_UPLOAD_KEY))) {
		return json({ error: "Wrong passcode" }, 401, cors);
	}
	if (parts[1] !== "jobs") return json({ error: "Not found" }, 404, cors);

	// POST /faceswap/jobs
	if (parts.length === 2) {
		if (request.method !== "POST") return json({ error: "Method not allowed" }, 405, cors);
		let body: unknown;
		try {
			body = await request.json();
		} catch {
			body = null;
		}
		const settings = readJobRequest(body);
		if (typeof settings === "string") return json({ error: settings }, 400, cors);
		await pruneOldJobs(bucket, now());
		const id = newShareId();
		await writeJob(bucket, id, { ...settings, createdAt: now(), status: "uploading" });
		return json({ id }, 201, cors);
	}

	const id = parts[2];
	if (!id || !isShareId(id)) return json({ error: "Not found" }, 404, cors);
	const job = await readJob(bucket, id);
	if (!job || now() - job.createdAt > JOB_TTL_MS) return json({ error: "This face swap has expired" }, 404, cors);
	const action = parts[3];

	// PUT /faceswap/jobs/<id>/face | target
	if ((action === "face" || action === "target") && request.method === "PUT") {
		if (job.status !== "uploading") return json({ error: "This face swap has already started" }, 409, cors);
		const type = (request.headers.get("Content-Type") ?? "").split(";")[0].trim().toLowerCase();
		const size = Number(request.headers.get("Content-Length"));
		const limit = action === "face" ? MAX_FACE_BYTES : MAX_TARGET_BYTES;
		if (action === "face" ? !isFaceType(type) : !isTargetType(job.kind, type)) {
			return json({ error: action === "face" ? "The face must be a PNG, JPEG or WebP picture" : job.kind === "video" ? "The clip must be MP4, WebM or MOV" : "The picture must be PNG, JPEG or WebP" }, 415, cors);
		}
		if (!Number.isFinite(size) || size <= 0) return json({ error: "Content-Length is required" }, 411, cors);
		if (size > limit) return json({ error: `At most ${Math.round(limit / 1024 ** 2)} MB` }, 413, cors);
		if (!request.body) return json({ error: "No file" }, 400, cors);
		await bucket.put(fileKey(id, action), request.body, { httpMetadata: { contentType: type } });
		await writeJob(bucket, id, { ...job, [action]: { type, bytes: size } });
		return json({ stored: action, bytes: size }, 200, cors);
	}

	// POST /faceswap/jobs/<id>/run
	if (action === "run" && request.method === "POST") {
		if (job.status !== "uploading") return json({ status: job.status }, 200, cors);
		if (!job.face || !job.target) return json({ error: "Upload the face and the clip first" }, 400, cors);
		const { url: submitUrl, body } = segmindRequest({ job, base: publicBase(request), id });
		const answer = await segmind(fetcher, key, submitUrl, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body),
		});
		if (answer.status < 200 || answer.status >= 300) {
			const problem = providerProblem(answer.status, answer.body);
			await deleteUploads(bucket, id);
			await writeJob(bucket, id, { ...job, status: "failed", error: problem.message });
			return json({ error: problem.message }, problem.status, cors);
		}
		const submitted = readSubmit(answer.body);
		if (!submitted) {
			await deleteUploads(bucket, id);
			await writeJob(bucket, id, { ...job, status: "failed", error: "Segmind's answer had no request id" });
			return json({ error: "Segmind's answer had no request id" }, 502, cors);
		}
		await writeJob(bucket, id, { ...job, ...submitted, status: "queued" });
		return json({ status: "queued" }, 202, cors);
	}

	// GET /faceswap/jobs/<id>
	if (action === undefined && request.method === "GET") {
		if (job.status === "done" || job.status === "failed" || job.status === "uploading") {
			return json({ status: job.status, ...(job.error ? { error: job.error } : {}) }, 200, cors);
		}
		if (!job.statusUrl || !job.resultUrl) return json({ status: "failed", error: "The job was never sent" }, 200, cors);
		const answer = await segmind(fetcher, key, job.statusUrl);
		const state = readStatus(answer.body);
		if (answer.status === 404) {
			await deleteUploads(bucket, id);
			await writeJob(bucket, id, { ...job, status: "failed", error: "Segmind no longer has this job" });
			return json({ status: "failed", error: "Segmind no longer has this job" }, 200, cors);
		}
		if (answer.status === 422 || state === "failed") {
			const error = readError(answer.body, "Segmind could not swap the face (no face found, or the content was blocked)");
			await deleteUploads(bucket, id);
			await writeJob(bucket, id, { ...job, status: "failed", error });
			return json({ status: "failed", error }, 200, cors);
		}
		if (answer.status < 200 || answer.status >= 300) {
			const problem = providerProblem(answer.status, answer.body);
			return json({ error: problem.message }, problem.status, cors);
		}
		if (state === "completed") {
			const result = await segmind(fetcher, key, job.resultUrl);
			const outputUrl = readOutputUrl(result.body);
			await deleteUploads(bucket, id);
			if (!outputUrl) {
				const error = readError(result.body, "Segmind's result had no output");
				await writeJob(bucket, id, { ...job, status: "failed", error });
				return json({ status: "failed", error }, 200, cors);
			}
			await writeJob(bucket, id, { ...job, status: "done", outputUrl });
			return json({ status: "done" }, 200, cors);
		}
		const status: JobStatus = state === "processing" ? "processing" : "queued";
		if (status !== job.status) await writeJob(bucket, id, { ...job, status });
		return json({ status }, 200, cors);
	}

	// GET /faceswap/jobs/<id>/output
	if (action === "output" && request.method === "GET") {
		if (job.status !== "done" || !job.outputUrl) return json({ error: "The face swap is not finished" }, 409, cors);
		const response = await fetcher(job.outputUrl);
		if (!response.ok || !response.body) return json({ error: `The result could not be fetched (${response.status})` }, 502, cors);
		const type = response.headers.get("Content-Type") ?? (job.kind === "video" ? "video/mp4" : "image/png");
		return new Response(response.body, {
			headers: {
				...cors,
				"Content-Type": type,
				...(response.headers.get("Content-Length") ? { "Content-Length": response.headers.get("Content-Length") as string } : {}),
				"Cache-Control": "no-store",
			},
		});
	}

	return json({ error: "Not found" }, 404, cors);
}
