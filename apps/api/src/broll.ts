/**
 * Script to B-roll with Claude: the editors split a script into lines and
 * time them themselves (`fixtures/broll`); this asks Claude for a better
 * stock search query for each line. It spends the server owner's API
 * credit, so it needs the server passcode (the sharing passcode,
 * `SHARE_UPLOAD_KEY`) as well as `ANTHROPIC_API_KEY`.
 *
 *   POST /broll/plan  { lines: string[] }  (X-Share-Key: passcode)  → { queries: string[], model }
 */

import Anthropic from "@anthropic-ai/sdk";
import { passcodeMatches } from "./share";

export type BrollEnv = {
	/** Claude API key (`wrangler secret put ANTHROPIC_API_KEY`). */
	ANTHROPIC_API_KEY?: string;
	SHARE_UPLOAD_KEY?: string;
};

export const BROLL_MODEL = "claude-opus-5-5";
export const MAX_LINES = 80;
export const MAX_LINE_CHARS = 400;
export const MAX_QUERY_CHARS = 60;

export const SYSTEM_PROMPT = `You choose stock-footage search terms for the B-roll of a short video.

You get the video's script as numbered lines. For each line, write one search query for a free stock library (Openverse pictures, Pixabay photos and videos): two to four plain English words naming something a camera can film, such as objects, places, food, weather, or people doing something. The query is searched as typed, so prefer common words that libraries tag.

Avoid names of real people, brands and logos, text on screen, and abstract ideas; for an abstract line, pick a concrete scene that shows it. Write the query in English even when the line is in another language. Do not add words like "video", "stock", "footage" or "photo".

Return exactly one query per line, in the same order.`;

export const QUERIES_SCHEMA = {
	type: "object",
	properties: {
		queries: { type: "array", items: { type: "string" } },
	},
	required: ["queries"],
	additionalProperties: false,
} as const;

/** The script lines from a request body, or why they cannot be used. */
export function readLines(body: unknown): string[] | string {
	const lines = (body as { lines?: unknown } | null)?.lines;
	if (!Array.isArray(lines) || lines.length === 0) return "lines must be a list of the script's lines";
	if (lines.length > MAX_LINES) return `At most ${MAX_LINES} lines at a time`;
	if (!lines.every((line) => typeof line === "string" && line.trim().length > 0)) return "Every line must be text";
	if (lines.some((line: string) => line.length > MAX_LINE_CHARS)) return `Lines are at most ${MAX_LINE_CHARS} characters`;
	return lines.map((line: string) => line.trim());
}

export function userPrompt(lines: string[]): string {
	return lines.map((line, index) => `${index + 1}. ${line}`).join("\n");
}

/** Claude's answer as one query per line, tidied, or null when it does not fit. */
export function readQueries(text: string, count: number): string[] | null {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch {
		return null;
	}
	const queries = (value as { queries?: unknown } | null)?.queries;
	if (!Array.isArray(queries) || queries.length !== count) return null;
	const tidy = queries.map((query) =>
		typeof query === "string" ? [...query.replace(/\s+/g, " ").trim()].slice(0, MAX_QUERY_CHARS).join("").trim() : "",
	);
	return tidy.every((query) => query.length > 0) ? tidy : null;
}

function json(body: unknown, status: number, headers: Record<string, string>): Response {
	return new Response(JSON.stringify(body), { status, headers: { ...headers, "Content-Type": "application/json" } });
}

/** Handles `/broll/*`; `cors` are the headers every answer carries. */
export async function handleBroll(
	request: Request,
	env: BrollEnv,
	cors: Record<string, string>,
	client?: Pick<Anthropic, "beta">,
): Promise<Response> {
	const { pathname } = new URL(request.url);
	if (request.method !== "POST" || pathname !== "/broll/plan") return json({ error: "Not found" }, 404, cors);
	if (!env.ANTHROPIC_API_KEY) return json({ error: "Claude is not set up on this server (ANTHROPIC_API_KEY)" }, 503, cors);
	if (!env.SHARE_UPLOAD_KEY) return json({ error: "Claude search terms need the server passcode set (SHARE_UPLOAD_KEY)" }, 503, cors);
	if (!(await passcodeMatches(request.headers.get("X-Share-Key"), env.SHARE_UPLOAD_KEY))) {
		return json({ error: "Wrong passcode" }, 401, cors);
	}
	let body: unknown = null;
	try {
		body = await request.json();
	} catch {
		// Reported below.
	}
	const lines = readLines(body);
	if (typeof lines === "string") return json({ error: lines }, 400, cors);

	const anthropic = client ?? new Anthropic({ apiKey: env.ANTHROPIC_API_KEY });
	try {
		const response = await anthropic.beta.messages.create({
			model: BROLL_MODEL,
			max_tokens: 4000,
			// On a policy decline the API re-runs the request on its recommended fallback model.
			betas: ["server-side-fallback-2026-07-01"],
			fallbacks: "default",
			output_config: { effort: "low", format: { type: "json_schema", schema: QUERIES_SCHEMA } },
			system: SYSTEM_PROMPT,
			messages: [{ role: "user", content: userPrompt(lines) }],
		});
		if (response.stop_reason === "refusal") {
			return json({ error: "Claude declined this script; the editor's own search terms still work" }, 422, cors);
		}
		if (response.stop_reason === "max_tokens") return json({ error: "Claude's answer was cut off" }, 502, cors);
		const text = response.content.map((block) => (block.type === "text" ? block.text : "")).join("");
		const queries = readQueries(text, lines.length);
		if (!queries) return json({ error: "Claude's answer did not have one query per line" }, 502, cors);
		return json({ queries, model: response.model }, 200, cors);
	} catch (error) {
		if (error instanceof Anthropic.RateLimitError) return json({ error: "Claude is busy; try again in a minute" }, 429, cors);
		if (error instanceof Anthropic.AuthenticationError) return json({ error: "The server's Claude key was refused" }, 502, cors);
		if (error instanceof Anthropic.APIError) return json({ error: `Claude failed (${error.status ?? "no status"})` }, 502, cors);
		throw error;
	}
}
