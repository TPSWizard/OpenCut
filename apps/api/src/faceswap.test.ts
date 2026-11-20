import { describe, expect, it } from "bun:test";
import {
	type Fetcher,
	handleFaceSwap,
	JOB_TTL_MS,
	MAX_TARGET_BYTES,
	modelName,
	pruneOldJobs,
	readJobRequest,
	readOutputUrl,
	readStatus,
	readSubmit,
	segmindRequest,
} from "./faceswap";

/** Enough of R2 for the face swap route, in memory. */
function memoryBucket(clock: () => number) {
	const store = new Map<string, { bytes: Uint8Array; type?: string; uploaded: Date }>();
	const bucket = {
		store,
		async put(key: string, value: ReadableStream | string, options?: { httpMetadata?: { contentType?: string } }) {
			const bytes = typeof value === "string" ? new TextEncoder().encode(value) : new Uint8Array(await new Response(value).arrayBuffer());
			store.set(key, { bytes, type: options?.httpMetadata?.contentType, uploaded: new Date(clock()) });
			return {};
		},
		async get(key: string) {
			const item = store.get(key);
			if (!item) return null;
			return {
				size: item.bytes.length,
				httpMetadata: { contentType: item.type },
				body: new Response(item.bytes).body,
				json: async () => JSON.parse(new TextDecoder().decode(item.bytes)),
			};
		},
		async delete(keys: string | string[]) {
			for (const key of Array.isArray(keys) ? keys : [keys]) store.delete(key);
		},
		async list({ prefix }: { prefix: string }) {
			return {
				objects: [...store.entries()].filter(([key]) => key.startsWith(prefix)).map(([key, item]) => ({ key, uploaded: item.uploaded })),
				truncated: false,
			};
		},
	};
	return bucket;
}

/** Answers Segmind's endpoints from a script; records every call. */
function fakeSegmind(script: Record<string, () => Response>) {
	const calls: { url: string; init?: RequestInit }[] = [];
	const fetcher: Fetcher = async (url, init) => {
		calls.push({ url, init });
		for (const [pattern, answer] of Object.entries(script)) {
			if (url.includes(pattern)) return answer();
		}
		return new Response("no route", { status: 500 });
	};
	return { fetcher, calls };
}

const jsonResponse = (body: unknown, status = 200) =>
	new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

const BASE = "https://api.example";
function call(path: string, init: RequestInit & { key?: string } = {}) {
	const headers = new Headers(init.headers);
	if (init.key !== "") headers.set("X-Share-Key", init.key ?? "pass");
	return new Request(`${BASE}${path}`, { ...init, headers });
}

describe("face swap settings", () => {
	it("needs consent and a known kind", () => {
		expect(readJobRequest({ kind: "video", consent: true })).toEqual({ kind: "video", faceIndex: 0, quality: "best" });
		expect(readJobRequest({ kind: "image", consent: true, faceIndex: 2, quality: "fast" })).toEqual({ kind: "image", faceIndex: 2, quality: "fast" });
		expect(typeof readJobRequest({ kind: "video" })).toBe("string");
		expect(typeof readJobRequest({ kind: "video", consent: "yes" })).toBe("string");
		expect(typeof readJobRequest({ kind: "audio", consent: true })).toBe("string");
		expect(typeof readJobRequest({ kind: "video", consent: true, faceIndex: 11 })).toBe("string");
		expect(typeof readJobRequest({ kind: "video", consent: true, quality: "max" })).toBe("string");
		expect(modelName("fast")).toBe("hyperswap_1a");
		expect(modelName("best")).toBe("hyperswap_1c");
	});

	it("asks Segmind with links to the uploads", () => {
		const video = segmindRequest({ job: { kind: "video", faceIndex: 1, quality: "best", createdAt: 0, status: "uploading" }, base: BASE, id: "abc" });
		expect(video.url).toBe("https://api.segmind.com/v2/video-faceswap-by-facefusion-labs");
		expect(video.body).toEqual({
			source_image: `${BASE}/faceswap/files/abc/face`,
			target_video: `${BASE}/faceswap/files/abc/target`,
			model_name: "hyperswap_1c",
			target_face_index: 1,
			face_detector_score: 0.5,
		});
		const image = segmindRequest({ job: { kind: "image", faceIndex: 0, quality: "fast", createdAt: 0, status: "uploading" }, base: BASE, id: "abc" });
		expect(image.url).toBe("https://api.segmind.com/v2/hyperswap-image-faceswap-by-facefusion-labs");
		expect(image.body).toMatchObject({ target_image: `${BASE}/faceswap/files/abc/target`, model_name: "hyperswap_1a" });
	});

	it("reads Segmind's answers defensively", () => {
		expect(readSubmit({ request_id: "r-1" })).toEqual({
			requestId: "r-1",
			statusUrl: "https://api.segmind.com/v2/requests/r-1/status",
			resultUrl: "https://api.segmind.com/v2/requests/r-1",
		});
		// Status URLs elsewhere are never polled with the key.
		expect(readSubmit({ request_id: "r-1", status_url: "https://evil.example/x" })?.statusUrl).toBe("https://api.segmind.com/v2/requests/r-1/status");
		expect(readSubmit({ request_id: "r-1", status_url: "https://api.segmind.com/v2/requests/r-1/status?x=1" })?.statusUrl).toBe("https://api.segmind.com/v2/requests/r-1/status?x=1");
		expect(readSubmit({})).toBeNull();
		expect(readSubmit({ request_id: "../x" })).toBeNull();
		expect(readStatus({ status: "QUEUED" })).toBe("queued");
		expect(readStatus({ status: "processing" })).toBe("processing");
		expect(readStatus({ status: "COMPLETED" })).toBe("completed");
		expect(readStatus({ status: "FAILED" })).toBe("failed");
		expect(readStatus({})).toBeNull();
		expect(readOutputUrl({ status: "COMPLETED", output: "https://cdn.example/out.mp4" })).toBe("https://cdn.example/out.mp4");
		expect(readOutputUrl({ output: ["https://cdn.example/a.mp4"] })).toBe("https://cdn.example/a.mp4");
		expect(readOutputUrl({ output: { video: "https://cdn.example/v.mp4" } })).toBe("https://cdn.example/v.mp4");
		expect(readOutputUrl({ output: "http://insecure.example/x.mp4" })).toBeNull();
		expect(readOutputUrl({ status: "COMPLETED" })).toBeNull();
	});
});

describe("face swap route", () => {
	const env = (bucket: ReturnType<typeof memoryBucket>, extra: Record<string, string | undefined> = {}) => ({
		SEGMIND_API_KEY: "sg-key",
		SHARE_UPLOAD_KEY: "pass",
		SHARES: bucket as unknown as R2Bucket,
		...extra,
	});

	it("explains what is missing", async () => {
		let time = 1_000;
		const bucket = memoryBucket(() => time);
		const noKey = await handleFaceSwap(call("/faceswap/jobs", { method: "POST" }), env(bucket, { SEGMIND_API_KEY: undefined }), {});
		expect(noKey.status).toBe(503);
		expect((await noKey.json()).error).toContain("SEGMIND_API_KEY");
		const wrong = await handleFaceSwap(call("/faceswap/jobs", { method: "POST", key: "nope" }), env(bucket), {});
		expect(wrong.status).toBe(401);
		const noConsent = await handleFaceSwap(call("/faceswap/jobs", { method: "POST", body: JSON.stringify({ kind: "video" }) }), env(bucket), {});
		expect(noConsent.status).toBe(400);
		time += 1;
	});

	it("runs a video swap from upload to output and cleans up", async () => {
		let time = 10_000;
		const bucket = memoryBucket(() => time);
		const segmind = fakeSegmind({
			"/v2/video-faceswap-by-facefusion-labs": () => jsonResponse({ request_id: "req-7", status: "QUEUED" }),
			"/v2/requests/req-7/status": () => jsonResponse({ status: segmindState }),
			"/v2/requests/req-7": () => jsonResponse({ status: "COMPLETED", output: "https://cdn.segmind.example/out.mp4" }),
			"cdn.segmind.example/out.mp4": () => new Response(new Uint8Array([1, 2, 3]), { headers: { "Content-Type": "video/mp4" } }),
		});
		let segmindState = "PROCESSING";
		const run = (request: Request) => handleFaceSwap(request, env(bucket), { "X-Cors": "1" }, segmind.fetcher, () => time);

		const created = await run(call("/faceswap/jobs", { method: "POST", body: JSON.stringify({ kind: "video", consent: true, faceIndex: 0, quality: "fast" }) }));
		expect(created.status).toBe(201);
		const { id } = (await created.json()) as { id: string };

		// Wrong types and sizes are refused before anything is stored.
		const badType = await run(call(`/faceswap/jobs/${id}/target`, { method: "PUT", headers: { "Content-Type": "image/png", "Content-Length": "3" }, body: new Uint8Array([1, 2, 3]) }));
		expect(badType.status).toBe(415);
		const tooBig = await run(call(`/faceswap/jobs/${id}/target`, { method: "PUT", headers: { "Content-Type": "video/mp4", "Content-Length": String(MAX_TARGET_BYTES + 1) }, body: new Uint8Array([1]) }));
		expect(tooBig.status).toBe(413);
		const early = await run(call(`/faceswap/jobs/${id}/run`, { method: "POST" }));
		expect(early.status).toBe(400);

		const face = await run(call(`/faceswap/jobs/${id}/face`, { method: "PUT", headers: { "Content-Type": "image/jpeg", "Content-Length": "4" }, body: new Uint8Array([9, 9, 9, 9]) }));
		expect(face.status).toBe(200);
		const target = await run(call(`/faceswap/jobs/${id}/target`, { method: "PUT", headers: { "Content-Type": "video/mp4", "Content-Length": "5" }, body: new Uint8Array([5, 5, 5, 5, 5]) }));
		expect(target.status).toBe(200);

		// Segmind can read the uploads without the passcode.
		const file = await run(call(`/faceswap/files/${id}/target`, { key: "" }));
		expect(file.status).toBe(200);
		expect(file.headers.get("Content-Type")).toBe("video/mp4");
		expect(new Uint8Array(await file.arrayBuffer())).toEqual(new Uint8Array([5, 5, 5, 5, 5]));

		const started = await run(call(`/faceswap/jobs/${id}/run`, { method: "POST" }));
		expect(started.status).toBe(202);
		const submit = segmind.calls.find((entry) => entry.url.endsWith("/v2/video-faceswap-by-facefusion-labs"));
		expect((submit?.init?.headers as Record<string, string>)["x-api-key"]).toBe("sg-key");
		expect(JSON.parse(String(submit?.init?.body))).toMatchObject({
			source_image: `${BASE}/faceswap/files/${id}/face`,
			target_video: `${BASE}/faceswap/files/${id}/target`,
			model_name: "hyperswap_1a",
		});
		// Uploading again after the run is refused.
		const late = await run(call(`/faceswap/jobs/${id}/face`, { method: "PUT", headers: { "Content-Type": "image/jpeg", "Content-Length": "1" }, body: new Uint8Array([1]) }));
		expect(late.status).toBe(409);

		expect(await (await run(call(`/faceswap/jobs/${id}`))).json()).toEqual({ status: "processing" });
		const notYet = await run(call(`/faceswap/jobs/${id}/output`));
		expect(notYet.status).toBe(409);

		segmindState = "COMPLETED";
		expect(await (await run(call(`/faceswap/jobs/${id}`))).json()).toEqual({ status: "done" });
		// The uploads are gone once the swap is done.
		expect(bucket.store.has(`faceswap/${id}/face`)).toBe(false);
		expect(bucket.store.has(`faceswap/${id}/target`)).toBe(false);
		expect((await run(call(`/faceswap/files/${id}/face`, { key: "" }))).status).toBe(404);

		const output = await run(call(`/faceswap/jobs/${id}/output`));
		expect(output.status).toBe(200);
		expect(output.headers.get("Content-Type")).toBe("video/mp4");
		expect(output.headers.get("X-Cors")).toBe("1");
		expect(new Uint8Array(await output.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));

		// After two hours the job is gone.
		time += JOB_TTL_MS + 1;
		expect((await run(call(`/faceswap/jobs/${id}`))).status).toBe(404);
	});

	it("reports a failed swap and deletes the uploads", async () => {
		const time = 5_000;
		const bucket = memoryBucket(() => time);
		const segmind = fakeSegmind({
			"/v2/hyperswap-image-faceswap-by-facefusion-labs": () => jsonResponse({ request_id: "req-9" }),
			"/v2/requests/req-9/status": () => jsonResponse({ status: "FAILED", error: "No face detected in target" }, 422),
		});
		const run = (request: Request) => handleFaceSwap(request, env(bucket), {}, segmind.fetcher, () => time);
		const { id } = (await (await run(call("/faceswap/jobs", { method: "POST", body: JSON.stringify({ kind: "image", consent: true }) }))).json()) as { id: string };
		await run(call(`/faceswap/jobs/${id}/face`, { method: "PUT", headers: { "Content-Type": "image/png", "Content-Length": "1" }, body: new Uint8Array([1]) }));
		await run(call(`/faceswap/jobs/${id}/target`, { method: "PUT", headers: { "Content-Type": "image/webp", "Content-Length": "1" }, body: new Uint8Array([2]) }));
		expect((await run(call(`/faceswap/jobs/${id}/run`, { method: "POST" }))).status).toBe(202);
		expect(await (await run(call(`/faceswap/jobs/${id}`))).json()).toEqual({ status: "failed", error: "No face detected in target" });
		expect(bucket.store.has(`faceswap/${id}/face`)).toBe(false);
	});

	it("turns an out-of-credit account into a clear message", async () => {
		const bucket = memoryBucket(() => 1);
		const segmind = fakeSegmind({ "/v2/video-faceswap": () => jsonResponse({ error: "Insufficient Credits" }, 406) });
		const run = (request: Request) => handleFaceSwap(request, env(bucket), {}, segmind.fetcher, () => 1);
		const { id } = (await (await run(call("/faceswap/jobs", { method: "POST", body: JSON.stringify({ kind: "video", consent: true }) }))).json()) as { id: string };
		await run(call(`/faceswap/jobs/${id}/face`, { method: "PUT", headers: { "Content-Type": "image/png", "Content-Length": "1" }, body: new Uint8Array([1]) }));
		await run(call(`/faceswap/jobs/${id}/target`, { method: "PUT", headers: { "Content-Type": "video/webm", "Content-Length": "1" }, body: new Uint8Array([2]) }));
		const started = await run(call(`/faceswap/jobs/${id}/run`, { method: "POST" }));
		expect(started.status).toBe(402);
		expect((await started.json()).error).toContain("credits");
	});

	it("prunes jobs older than two hours", async () => {
		let time = 0;
		const bucket = memoryBucket(() => time);
		await bucket.put("faceswap/old/job.json", "{}");
		await bucket.put("shares/keep.opencut.zip", "x");
		time = JOB_TTL_MS + 10;
		await bucket.put("faceswap/new/job.json", "{}");
		expect(await pruneOldJobs(bucket as never, time)).toBe(1);
		expect([...bucket.store.keys()].sort()).toEqual(["faceswap/new/job.json", "shares/keep.opencut.zip"]);
	});
});
