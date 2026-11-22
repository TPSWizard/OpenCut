/**
 * Caption keywords with Claude: the editor sends its captions as lists of
 * words; Claude picks the few words in each that carry the meaning, which
 * the editor then draws in the keyword colour and size. It spends the
 * server owner's API credit, so it needs the server passcode
 * (`SHARE_UPLOAD_KEY`) as well as `ANTHROPIC_API_KEY`, like `/shorts/plan`.
 *
 *   POST /captions/keywords  { captions: [{ words: string[] }] }
 *     (X-Share-Key: passcode)  → { keywords: number[][], model }
 *
 * `keywords[i]` are indices into caption i's words, at most
 * `MAX_PER_CAPTION`, in order.
 */

import Anthropic from "@anthropic-ai/sdk";
import { passcodeMatches } from "./share";

export type KeywordsEnv = {
	ANTHROPIC_API_KEY?: string;
	SHARE_UPLOAD_KEY?: string;
};

export const KEYWORDS_MODEL = "claude-opus-5-5";
export const MAX_CAPTIONS = 1000;
export const MAX_WORDS = 80;
export const MAX_WORD_CHARS = 60;
export const MAX_TOTAL_CHARS = 150_000;
export const MAX_PER_CAPTION = 3;

export interface KeywordsRequest {
	captions: string[][];
}

export const SYSTEM_PROMPT = `You choose the words to emphasise in video captions, the way short-form videos colour or enlarge the words that matter.

You get numbered captions; each word is numbered within its caption. For each caption pick the one to three words a viewer should catch even when skimming: the key noun, number, name, strong verb or surprising word that carries the point. Never pick filler, articles, pronouns or other small function words. Pick one word for a short caption, two or three only for a long one, and none for a caption with nothing worth stressing. Answer with every caption's number and the numbers of its chosen words.`;

export const KEYWORDS_SCHEMA = {
	type: "object",
	properties: {
		captions: {
			type: "array",
			items: {
				type: "object",
				properties: {
					caption: { type: "integer" },
					words: { type: "array", items: { type: "integer" } },
				},
				required: ["caption", "words"],
				additionalProperties: false,
			},
		},
	},
	required: ["captions"],
	additionalProperties: false,
} as const;

/** The request from a body, or why it cannot be used. */
export function readKeywordsRequest(body: unknown): KeywordsRequest | string {
	const captions = (body as { captions?: unknown } | null)?.captions;
	if (!Array.isArray(captions) || captions.length === 0) return "captions must be a list of { words: [text] }";
	if (captions.length > MAX_CAPTIONS) return `At most ${MAX_CAPTIONS} captions at a time`;
	let total = 0;
	const tidy: string[][] = [];
	for (const [index, caption] of captions.entries()) {
		const words = (caption as { words?: unknown } | null)?.words;
		if (!Array.isArray(words) || words.length === 0) return `captions[${index}] needs words`;
		if (words.length > MAX_WORDS) return `Captions have at most ${MAX_WORDS} words`;
		const clean: string[] = [];
		for (const word of words) {
			if (typeof word !== "string" || !word.trim()) return `captions[${index}] has a word that is not text`;
			if (word.length > MAX_WORD_CHARS) return `Words are at most ${MAX_WORD_CHARS} characters`;
			total += word.length + 1;
			clean.push(word.replace(/\s+/g, " ").trim());
		}
		tidy.push(clean);
	}
	if (total > MAX_TOTAL_CHARS) return `The captions are at most ${MAX_TOTAL_CHARS} characters`;
	return { captions: tidy };
}

export function userPrompt(request: KeywordsRequest): string {
	return request.captions.map((words, index) => `${index}: ${words.map((word, at) => `${at}=${word}`).join(" ")}`).join("\n");
}

/**
 * Claude's picks, checked against the captions: one list per caption
 * (missing captions get none), word numbers in range, no repeats, at most
 * `MAX_PER_CAPTION`, sorted. Null when it is not the shape asked for.
 */
export function readKeywords(text: string, request: KeywordsRequest): number[][] | null {
	let value: unknown;
	try {
		value = JSON.parse(text);
	} catch {
		return null;
	}
	const captions = (value as { captions?: unknown } | null)?.captions;
	if (!Array.isArray(captions)) return null;
	const picks: number[][] = request.captions.map(() => []);
	for (const raw of captions) {
		const item = raw as { caption?: unknown; words?: unknown } | null;
		if (!item || !Number.isInteger(item.caption) || !Array.isArray(item.words)) continue;
		const index = item.caption as number;
		if (index < 0 || index >= picks.length || picks[index].length > 0) continue;
		const count = request.captions[index].length;
		const words = [...new Set(item.words.filter((at): at is number => Number.isInteger(at) && at >= 0 && at < count))];
		picks[index] = words.slice(0, MAX_PER_CAPTION).sort((a, b) => a - b);
	}
	return picks;
}

function json(body: unknown, status: number, headers: Record<string, string>): Response {
	return new Response(JSON.stringify(body), { status, headers: { ...headers, "Content-Type": "application/json" } });
}

/** Handles `/captions/*`; `cors` are the headers every answer carries. */
export async function handleKeywords(
	request: Request,
	env: KeywordsEnv,
	cors: Record<string, string>,
	client?: Pick<Anthropic, "beta">,
): Promise<Response> {
	const { pathname } = new URL(request.url);
	if (request.method !== "POST" || pathname !== "/captions/keywords") return json({ error: "Not found" }, 404, cors);
	if (!env.ANTHROPIC_API_KEY) return json({ error: "Claude is not set up on this server (ANTHROPIC_API_KEY)" }, 503, cors);
	if (!env.SHARE_UPLOAD_KEY) return json({ error: "Picking keywords with Claude needs the server passcode set (SHARE_UPLOAD_KEY)" }, 503, cors);
	if (!(await passcodeMatches(request.headers.get("X-Share-Key"), env.SHARE_UPLOAD_KEY))) {
		return json({ error: "Wrong passcode" }, 401, cors);
	}
	let body: unknown = null;
	try {
		body = await request.json();
	} catch {
		// Reported below.
	}
	const parsed = readKeywordsRequest(body);
	if (typeof parsed === "string") return json({ error: parsed }, 400, cors);

	const anthropic = client ?? new Anthropic({ apiKey: env.ANTHROPIC_API_KEY });
	try {
		const stream = anthropic.beta.messages.stream({
			model: KEYWORDS_MODEL,
			max_tokens: 32000,
			// On a policy decline the API re-runs the request on its recommended fallback model.
			betas: ["server-side-fallback-2026-07-01"],
			fallbacks: "default",
			thinking: { type: "adaptive" },
			output_config: { effort: "low", format: { type: "json_schema", schema: KEYWORDS_SCHEMA } },
			system: SYSTEM_PROMPT,
			messages: [{ role: "user", content: userPrompt(parsed) }],
		});
		const response = await stream.finalMessage();
		if (response.stop_reason === "refusal") {
			return json({ error: "Claude declined these captions; the editor's own pick still works" }, 422, cors);
		}
		if (response.stop_reason === "max_tokens") return json({ error: "Claude's answer was cut off" }, 502, cors);
		const text = response.content.map((block) => (block.type === "text" ? block.text : "")).join("");
		const keywords = readKeywords(text, parsed);
		if (!keywords) return json({ error: "Claude's answer was not a list of keywords" }, 502, cors);
		return json({ keywords, model: response.model }, 200, cors);
	} catch (error) {
		if (error instanceof Anthropic.RateLimitError) return json({ error: "Claude is busy; try again in a minute" }, 429, cors);
		if (error instanceof Anthropic.AuthenticationError) return json({ error: "The server's Claude key was refused" }, 502, cors);
		if (error instanceof Anthropic.APIError) return json({ error: `Claude failed (${error.status ?? "no status"})` }, 502, cors);
		throw error;
	}
}
