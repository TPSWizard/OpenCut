/**
 * Talks to the API worker's face swap route (`apps/api/src/faceswap.ts`):
 * starts a job, uploads the face and the clip, starts the swap, polls it
 * and downloads the result. Every call carries the server passcode.
 */

import { apiUrl } from "@/lib/api";

export type FaceSwapKind = "video" | "image";
export type FaceSwapQuality = "fast" | "best";
export type FaceSwapStage = "uploading" | "queued" | "processing" | "downloading";

/** Who did the swap, for the media's origin and the credits. */
export const FACE_SWAP_MODEL = "HyperSwap (FaceFusion Labs) via Segmind";
export const POLL_MS = 3000;
/** Long enough for a minute of video at Segmind's stated speeds, with room to queue. */
export const TIMEOUT_MS = 20 * 60 * 1000;

async function errorOf(response: Response, fallback: string): Promise<Error> {
	const body = (await response.json().catch(() => null)) as { error?: unknown } | null;
	return new Error(typeof body?.error === "string" ? body.error : `${fallback} (${response.status})`);
}

export async function runFaceSwap({
	kind,
	face,
	target,
	faceIndex = 0,
	quality = "best",
	passcode,
	onStage,
	signal,
	fetchImpl = fetch,
	sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms)),
	now = Date.now,
}: {
	kind: FaceSwapKind;
	face: Blob;
	target: Blob;
	faceIndex?: number;
	quality?: FaceSwapQuality;
	passcode: string;
	onStage?: (stage: FaceSwapStage) => void;
	signal?: AbortSignal;
	fetchImpl?: typeof fetch;
	sleep?: (ms: number) => Promise<unknown>;
	now?: () => number;
}): Promise<Blob> {
	const headers = { "X-Share-Key": passcode };
	onStage?.("uploading");
	const created = await fetchImpl(apiUrl("/faceswap/jobs"), {
		method: "POST",
		headers: { ...headers, "Content-Type": "application/json" },
		body: JSON.stringify({ kind, faceIndex, quality, consent: true }),
		signal,
	});
	if (!created.ok) throw await errorOf(created, "The face swap could not start");
	const { id } = (await created.json()) as { id: string };
	const job = (path = "") => apiUrl(`/faceswap/jobs/${id}${path}`);

	for (const [part, file] of [["face", face], ["target", target]] as const) {
		const stored = await fetchImpl(job(`/${part}`), {
			method: "PUT",
			headers: { ...headers, "Content-Type": file.type },
			body: file,
			signal,
		});
		if (!stored.ok) throw await errorOf(stored, `The ${part === "face" ? "face" : "clip"} could not be uploaded`);
	}

	const started = await fetchImpl(job("/run"), { method: "POST", headers, signal });
	if (!started.ok) throw await errorOf(started, "The face swap could not start");

	const deadline = now() + TIMEOUT_MS;
	let stage: FaceSwapStage = "queued";
	onStage?.(stage);
	for (;;) {
		if (signal?.aborted) throw new DOMException("Cancelled", "AbortError");
		if (now() > deadline) throw new Error("The face swap took too long; try a shorter clip");
		await sleep(POLL_MS);
		const polled = await fetchImpl(job(), { headers, signal });
		if (!polled.ok) throw await errorOf(polled, "The face swap's progress could not be read");
		const body = (await polled.json()) as { status?: string; error?: string };
		if (body.status === "done") break;
		if (body.status === "failed") throw new Error(body.error || "The face swap failed");
		const next: FaceSwapStage = body.status === "processing" ? "processing" : "queued";
		if (next !== stage) {
			stage = next;
			onStage?.(stage);
		}
	}

	onStage?.("downloading");
	const output = await fetchImpl(job("/output"), { headers, signal });
	if (!output.ok) throw await errorOf(output, "The swapped clip could not be downloaded");
	return await output.blob();
}
