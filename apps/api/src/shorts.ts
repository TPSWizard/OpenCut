/**
 * Long video to shorts with Claude: the editor sends the video's
 * transcript as timed lines; Claude picks the moments that stand on their
 * own as short vertical clips, each a run of whole lines. The editor then
 * cuts each into its own 9:16 project. It spends the server owner's API
 * credit, so it needs the server passcode (`SHARE_UPLOAD_KEY`) as well as
 * `ANTHROPIC_API_KEY`, like `/broll/plan`.
 *
 *   POST /shorts/plan  { lines: [{ start, end, text }], count?, minSeconds?, maxSeconds? }
 *     (X-Share-Key: passcode)  → { clips: [{ first, last, start, end, title, reason }], model }
 */

import Anthropic from "@anthropic-ai/sdk";
import { passcodeMatches } from "./share";

export type ShortsEnv = {
	ANTHROPIC_API_KEY?: string;
	SHARE_UPLOAD_KEY?: string;
};

export const SHORTS_MODEL = "claude-opus-5-5";
export const MAX_LINES = 2000;
export const MAX_LINE_CHARS = 500;
export const MAX_TOTAL_CHARS = 150_000;
export const MAX_CLIPS = 10;
export const MAX_TITLE_CHARS = 80;
export const MAX_REASON_CHARS = 240;

export interface TimedLine {
	start: number;
	end: number;
	text: string;
}

export interface ShortsRequest {
	lines: TimedLine[];
	count: number;
	minSeconds: number;
	maxSeconds: number;
}

export interface ShortClip {
	/** Indices of the first and last line of the clip. */
	first: number;
	last: number;
	/** Seconds on the video's timeline. */
	start: number;
	end: number;
	title: string;
	reason: string;
}

export const SYSTEM_PROMPT = `You are an editor cutting short vertical clips (YouTube Shorts, TikTok, Reels) out of a longer video.

You get the video's transcript as numbered lines, each with its start and end time in seconds. Pick the moments that work on their own as a short: a strong opening line that hooks a viewer who has seen nothing else, one complete idea, story, tip, joke or opinion, and an ending that lands. Prefer moments with energy, surprise, a clear payoff or a quotable line. Avoid moments that depend on earlier context ("as I said", "this one", unexplained names), that start or stop mid-sentence, or that are greetings, sponsor reads or housekeeping.

Each clip is a run of whole consecutive lines, given by the number of its first and last line. Its length (the last line's end minus the first line's start) must be within the range you are given. Clips must not overlap. Return the best clips first, at most the number asked for; return fewer if fewer moments are good enough. For each clip write a short title of a few words (in the transcript's language) and one sentence on why it works.`;

export const CLIPS_SCHEMA = {
	type: "object",
	properties: {
		clips: {
			type: "array",
			items: {
				type: "object",
				properties: {
					first: { type: "integer" },
					last: { type: "integer" },
					title: { type: "string" },
					reason: { type: "string" },
				},
				required: ["first", "last", "title", "reason"],
				additionalProperties: false,
			},
		},
	},
	required: ["clips"],
	additionalProperties: false,
} as const;

const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);

/** The request from a body, or why it cannot be used. */
export function readShortsRequest(body: unknown): ShortsRequest | string {
	const raw = body as { lines?: unknown; count?: unknown; minSeconds?: unknown; maxSeconds?: unknown } | null;
	const lines = raw?.lines;
	if (!Array.isArray(lines) || lines.length === 0) return "lines must be the transcript as a list of { start, end, text }";
	if (lines.length > MAX_LINES) return `At most ${MAX_LINES} lines at a time`;
	let total = 0;
	const tidy: TimedLine[] = [];
	for (const [index, line] of lines.entries()) {
		const value = line as { start?: unknown; end?: unknown; text?: unknown } | null;
		if (!value || typeof value.text !== "string" || !value.text.trim()) return `lines[${index}] needs text`;
		if (!finite(value.start) || !finite(value.end) || value.start < 0 || value.end < value.start) {
			return `lines[${index}] needs a start and an end in seconds`;
		}
		if (value.text.length > MAX_LINE_CHARS) return `Lines are at most ${MAX_LINE_CHARS} characters`;
		total += value.text.length;
		tidy.push({ start: value.start, end: value.end, text: value.text.replace(/\s+/g, " ").trim() });
	}
	if (total > MAX_TOTAL_CHARS) return `The transcript is at most ${MAX_TOTAL_CHARS} characters`;
	const count = raw?.count ?? 3;
	if (!Number.isInteger(count) || (count as number) < 1 || (count as number) > MAX_CLIPS) return `count must be 1 to ${MAX_CLIPS}`;
	const minSeconds = raw?.minSeconds ?? 15;
	const maxSeconds = raw?.maxSeconds ?? 60;
	if (!finite(minSeconds) || !finite(maxSeconds) || minSeconds < 1 || maxSeconds < minSeconds || maxSeconds > 600) {
		return "minSeconds and maxSeconds must be seconds with 1 <= min <= max <= 600";
	}
	return { lines: tidy, count: count as number, minSeconds, maxSeconds };
}

export function userPrompt(request: ShortsRequest): string {
	const lines = request.lines.map((line, index) => `${index} [${line.start.toFixed(1)}-${line.end.toFixed(1)}] ${line.text}`);
	return [
		`Pick up to ${request.count} clips, each ${request.minSeconds} to ${request.maxSeconds} seconds long.`,
		"",
		"Transcript:",
		...lines,
	].join("\n");
}

const clip = (text: string, limit: number) => [...text.replace(/\s+/g, " ").trim()].slice(0, limit).join("").trim();

/**
 * Claude's clips, checked against the transcript: line numbers in range,
 * the length within the asked range (with a little slack), no overlaps
 * (the earlier-listed clip wins), at most `count`. Null when it is not the
 * shape asked for.
 */
export function readClips(text: string, request: ShortsRequest): ShortClip[] | null {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch {
		return null;
	}
	const clips = (value as { clips?: unknown } | null)?.clips;
	if (!Array.isArray(clips)) return null;
	const kept: ShortClip[] = [];
	const slack = 0.15;
	for (const raw of clips) {
		const item = raw as { first?: unknown; last?: unknown; title?: unknown; reason?: unknown } | null;
		if (!item || !Number.isInteger(item.first) || !Number.isInteger(item.last)) continue;
		const first = item.first as number;
		const last = item.last as number;
		if (first < 0 || last < first || last >= request.lines.length) continue;
		const start = request.lines[first].start;
		const end = request.lines[last].end;
		const length = end - start;
		if (length < request.minSeconds * (1 - slack) || length > request.maxSeconds * (1 + slack)) continue;
		if (kept.some((other) => first <= other.last && last >= other.first)) continue;
		kept.push({
			first,
			last,
			start,
			end,
			title: clip(typeof item.title === "string" ? item.title : "", MAX_TITLE_CHARS) || `Short ${kept.length + 1}`,
			reason: clip(typeof item.reason === "string" ? item.reason : "", MAX_REASON_CHARS),
		});
		if (kept.length === request.count) break;
	}
	return kept;
}

function json(body: unknown, status: number, headers: Record<string, string>): Response {
	return new Response(JSON.stringify(body), { status, headers: { ...headers, "Content-Type": "application/json" } });
}

/** Handles `/shorts/*`; `cors` are the headers every answer carries. */
export async function handleShorts(
	request: Request,
	env: ShortsEnv,
	cors: Record<string, string>,
	client?: Pick<Anthropic, "beta">,
): Promise<Response> {
	const { pathname } = new URL(request.url);
	if (request.method !== "POST" || pathname !== "/shorts/plan") return json({ error: "Not found" }, 404, cors);
	if (!env.ANTHROPIC_API_KEY) return json({ error: "Claude is not set up on this server (ANTHROPIC_API_KEY)" }, 503, cors);
	if (!env.SHARE_UPLOAD_KEY) return json({ error: "Picking shorts with Claude needs the server passcode set (SHARE_UPLOAD_KEY)" }, 503, cors);
	if (!(await passcodeMatches(request.headers.get("X-Share-Key"), env.SHARE_UPLOAD_KEY))) {
		return json({ error: "Wrong passcode" }, 401, cors);
	}
	let body: unknown = null;
	try {
		body = await request.json();
	} catch {
		// Reported below.
	}
	const parsed = readShortsRequest(body);
	if (typeof parsed === "string") return json({ error: parsed }, 400, cors);

	const anthropic = client ?? new Anthropic({ apiKey: env.ANTHROPIC_API_KEY });
	try {
		const response = await anthropic.beta.messages.create({
			model: SHORTS_MODEL,
			max_tokens: 16000,
			// On a policy decline the API re-runs the request on its recommended fallback model.
			betas: ["server-side-fallback-2026-07-01"],
			fallbacks: "default",
			thinking: { type: "adaptive" },
			output_config: { effort: "medium", format: { type: "json_schema", schema: CLIPS_SCHEMA } },
			system: SYSTEM_PROMPT,
			messages: [{ role: "user", content: userPrompt(parsed) }],
		});
		if (response.stop_reason === "refusal") {
			return json({ error: "Claude declined this transcript; the editor's own picks still work" }, 422, cors);
		}
		if (response.stop_reason === "max_tokens") return json({ error: "Claude's answer was cut off" }, 502, cors);
		const text = response.content.map((block) => (block.type === "text" ? block.text : "")).join("");
		const clips = readClips(text, parsed);
		if (!clips) return json({ error: "Claude's answer was not a list of clips" }, 502, cors);
		if (clips.length === 0) return json({ error: "Claude found no moment that fits the length asked for" }, 422, cors);
		return json({ clips, model: response.model }, 200, cors);
	} catch (error) {
		if (error instanceof Anthropic.RateLimitError) return json({ error: "Claude is busy; try again in a minute" }, 429, cors);
		if (error instanceof Anthropic.AuthenticationError) return json({ error: "The server's Claude key was refused" }, 502, cors);
		if (error instanceof Anthropic.APIError) return json({ error: `Claude failed (${error.status ?? "no status"})` }, 502, cors);
		throw error;
	}
}
