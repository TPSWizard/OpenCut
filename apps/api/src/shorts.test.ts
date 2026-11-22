import { describe, expect, it } from "bun:test";
import { CLIPS_SCHEMA, SHORTS_MODEL, handleShorts, readClips, readShortsRequest, userPrompt, type ShortsRequest } from "./shorts";

const env = { ANTHROPIC_API_KEY: "test-key", SHARE_UPLOAD_KEY: "pass" };
const post = (body: unknown, key = "pass") =>
	new Request("https://api.example/shorts/plan", {
		method: "POST",
		headers: { "Content-Type": "application/json", "X-Share-Key": key },
		body: JSON.stringify(body),
	});

function fakeClient(answer: { stop_reason: string; text?: string }) {
	const seen: unknown[] = [];
	const client = {
		beta: {
			messages: {
				create: async (params: unknown) => {
					seen.push(params);
					return {
						model: SHORTS_MODEL,
						stop_reason: answer.stop_reason,
						content: answer.text === undefined ? [] : [{ type: "thinking", thinking: "…" }, { type: "text", text: answer.text }],
					};
				},
			},
		},
	};
	return { client: client as never, seen };
}

// Ten lines of ten seconds each.
const lines = Array.from({ length: 10 }, (_, index) => ({ start: index * 10, end: index * 10 + 9.5, text: `Line ${index}.` }));
const request = (extra: Partial<ShortsRequest> = {}): ShortsRequest => ({ lines, count: 3, minSeconds: 15, maxSeconds: 40, ...extra });

describe("shorts from Claude", () => {
	it("checks the transcript and the settings", () => {
		const parsed = readShortsRequest({ lines: [{ start: 0, end: 1, text: "  Hi   there " }] });
		expect(parsed).toEqual({ lines: [{ start: 0, end: 1, text: "Hi there" }], count: 3, minSeconds: 15, maxSeconds: 60 });
		expect(typeof readShortsRequest({ lines: [] })).toBe("string");
		expect(typeof readShortsRequest({ lines: [{ start: 2, end: 1, text: "x" }] })).toBe("string");
		expect(typeof readShortsRequest({ lines: [{ start: 0, end: 1, text: " " }] })).toBe("string");
		expect(typeof readShortsRequest({ lines, count: 11 })).toBe("string");
		expect(typeof readShortsRequest({ lines, minSeconds: 50, maxSeconds: 20 })).toBe("string");
		expect(userPrompt(request()).split("\n").slice(0, 4)).toEqual([
			"Pick up to 3 clips, each 15 to 40 seconds long.",
			"",
			"Transcript:",
			"0 [0.0-9.5] Line 0.",
		]);
	});

	it("keeps clips that fit and do not overlap", () => {
		const text = JSON.stringify({
			clips: [
				{ first: 2, last: 4, title: "  The  middle ", reason: "Lands." },
				{ first: 4, last: 5, title: "Overlaps", reason: "" },
				{ first: 0, last: 0, title: "Too short", reason: "" },
				{ first: 5, last: 9, title: "Too long", reason: "" },
				{ first: 7, last: 12, title: "Out of range", reason: "" },
				{ first: 6, last: 7, title: "", reason: "Ok." },
			],
		});
		expect(readClips(text, request())).toEqual([
			{ first: 2, last: 4, start: 20, end: 49.5, title: "The middle", reason: "Lands." },
			{ first: 6, last: 7, start: 60, end: 79.5, title: "Short 2", reason: "Ok." },
		]);
		expect(readClips(text, request({ count: 1 }))).toHaveLength(1);
		expect(readClips("nope", request())).toBeNull();
	});

	it("asks Claude with the schema, adaptive thinking and fallbacks", async () => {
		const { client, seen } = fakeClient({ stop_reason: "end_turn", text: '{"clips":[{"first":1,"last":2,"title":"A","reason":"B"}]}' });
		const response = await handleShorts(post({ lines, maxSeconds: 40 }), env, {}, client);
		expect(response.status).toBe(200);
		expect(await response.json()).toEqual({
			clips: [{ first: 1, last: 2, start: 10, end: 29.5, title: "A", reason: "B" }],
			model: SHORTS_MODEL,
		});
		const params = seen[0] as Record<string, unknown>;
		expect(params.model).toBe("claude-opus-5-5");
		expect(params.thinking).toEqual({ type: "adaptive" });
		expect(params.fallbacks).toBe("default");
		expect(params.output_config).toEqual({ effort: "medium", format: { type: "json_schema", schema: CLIPS_SCHEMA } });
	});

	it("answers refusals, empty picks, wrong passcodes and missing setup clearly", async () => {
		const refused = fakeClient({ stop_reason: "refusal" });
		expect((await handleShorts(post({ lines }), env, {}, refused.client)).status).toBe(422);
		const none = fakeClient({ stop_reason: "end_turn", text: '{"clips":[]}' });
		expect((await handleShorts(post({ lines }), env, {}, none.client)).status).toBe(422);
		const bad = fakeClient({ stop_reason: "end_turn", text: "{}" });
		expect((await handleShorts(post({ lines }), env, {}, bad.client)).status).toBe(502);
		const never = fakeClient({ stop_reason: "end_turn", text: "{}" });
		expect((await handleShorts(post({ lines }, "wrong"), env, {}, never.client)).status).toBe(401);
		expect(never.seen.length).toBe(0);
		expect((await handleShorts(post({ lines }), { SHARE_UPLOAD_KEY: "pass" }, {}, never.client)).status).toBe(503);
		expect((await handleShorts(post({ lines: [] }), env, {}, never.client)).status).toBe(400);
	});
});
