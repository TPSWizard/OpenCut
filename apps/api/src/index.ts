import { env } from "cloudflare:workers";
import { Elysia, t } from "elysia";
import { CloudflareAdapter } from "elysia/adapter/cloudflare-worker";

/**
 * Bindings and variables the worker can use. Every one of them is optional:
 * a feature whose binding is missing answers 503 with a clear message instead
 * of failing at startup, so a bare `wrangler dev` still serves the editor.
 */
type Env = {
	/** Freesound API token (`wrangler secret put FREESOUND_API_KEY`). */
	FREESOUND_API_KEY?: string;
	/** KV namespace that stores feedback entries. */
	FEEDBACK?: KVNamespace;
	/** Workers rate limiting binding, applied per client IP. */
	RATE_LIMITER?: RateLimit;
	/** Allowed browser origin for CORS; defaults to any origin. */
	CORS_ORIGIN?: string;
};

const bindings = env as Env;

const MAX_FEEDBACK_LENGTH = 2000;

const FREESOUND_FIELDS = [
	"id",
	"name",
	"description",
	"url",
	"previews",
	"download",
	"duration",
	"filesize",
	"type",
	"channels",
	"bitrate",
	"bitdepth",
	"samplerate",
	"username",
	"tags",
	"license",
	"created",
	"num_downloads",
	"avg_rating",
	"num_ratings",
].join(",");

const COMMERCIAL_LICENSES_FILTER = [
	"Attribution",
	"Creative Commons 0",
	"Attribution Noncommercial",
	"Attribution Commercial",
]
	.map((license) => `"${license}"`)
	.join(" OR ");

const EFFECT_TAGS_FILTER = [
	"sound-effect",
	"sfx",
	"foley",
	"ambient",
	"nature",
	"mechanical",
	"electronic",
	"impact",
	"whoosh",
	"explosion",
]
	.map((tag) => `tag:${tag}`)
	.join(" OR ");

function corsHeaders(request: Request): Record<string, string> {
	const allowed = bindings.CORS_ORIGIN ?? "*";
	const origin = request.headers.get("Origin");
	let allowOrigin = "*";
	if (allowed !== "*") {
		allowOrigin = origin === allowed ? allowed : "null";
	}
	return {
		"Access-Control-Allow-Origin": allowOrigin,
		"Access-Control-Allow-Methods": "GET, POST, OPTIONS",
		"Access-Control-Allow-Headers": "Content-Type",
		"Access-Control-Max-Age": "86400",
		Vary: "Origin",
	};
}

async function isRateLimited(request: Request): Promise<boolean> {
	const limiter = bindings.RATE_LIMITER;
	if (!limiter) return false;
	const key = request.headers.get("CF-Connecting-IP") ?? "anonymous";
	const { success } = await limiter.limit({ key });
	return !success;
}

type FreesoundResult = {
	id: number;
	name: string;
	description: string;
	url: string;
	previews?: Record<string, string>;
	download?: string;
	duration: number;
	filesize: number;
	type: string;
	channels: number;
	bitrate: number;
	bitdepth: number;
	samplerate: number;
	username: string;
	tags: string[];
	license: string;
	created: string;
	num_downloads?: number;
	avg_rating?: number;
	num_ratings?: number;
};

type FreesoundResponse = {
	count: number;
	next: string | null;
	previous: string | null;
	results: FreesoundResult[];
};

function transformFreesoundResult(result: FreesoundResult) {
	return {
		id: result.id,
		name: result.name,
		description: result.description,
		url: result.url,
		previewUrl:
			result.previews?.["preview-hq-mp3"] ??
			result.previews?.["preview-lq-mp3"],
		downloadUrl: result.download,
		duration: result.duration,
		filesize: result.filesize,
		type: result.type,
		channels: result.channels,
		bitrate: result.bitrate,
		bitdepth: result.bitdepth,
		samplerate: result.samplerate,
		username: result.username,
		tags: result.tags,
		license: result.license,
		created: result.created,
		downloads: result.num_downloads ?? 0,
		rating: result.avg_rating ?? 0,
		ratingCount: result.num_ratings ?? 0,
	};
}

const soundsSearchQuery = t.Object({
	q: t.Optional(t.String({ maxLength: 500 })),
	type: t.Optional(t.Union([t.Literal("songs"), t.Literal("effects")])),
	page: t.Optional(t.Numeric({ minimum: 1, maximum: 1000 })),
	page_size: t.Optional(t.Numeric({ minimum: 1, maximum: 150 })),
	sort: t.Optional(
		t.Union([
			t.Literal("downloads"),
			t.Literal("rating"),
			t.Literal("created"),
			t.Literal("score"),
		]),
	),
	min_rating: t.Optional(t.Numeric({ minimum: 0, maximum: 5 })),
	commercial_only: t.Optional(t.String()),
});

export default new Elysia({ adapter: CloudflareAdapter })
	.onAfterHandle(({ request, set }) => {
		Object.assign(set.headers, corsHeaders(request));
	})
	.onError(({ request, set }) => {
		Object.assign(set.headers, corsHeaders(request));
	})
	.options(
		"/*",
		({ request }) =>
			new Response(null, { status: 204, headers: corsHeaders(request) }),
	)
	.get("/", () => ({ status: "ok" }))
	.get("/health", () => ({ healthy: true, timestamp: new Date().toISOString() }))
	.post("/echo", ({ body }) => body, {
		body: t.Object({ message: t.String() }),
	})
	.get(
		"/sounds/search",
		async ({ request, query, set }) => {
			if (await isRateLimited(request)) {
				set.status = 429;
				return { error: "Too many requests" };
			}

			const token = bindings.FREESOUND_API_KEY;
			if (!token) {
				set.status = 503;
				return {
					error: "Sound search is not configured",
					message:
						"Set the FREESOUND_API_KEY secret on the API worker to enable sound search.",
				};
			}

			const type = query.type ?? "effects";
			if (type === "songs") {
				set.status = 501;
				return {
					error: "Songs are not available yet",
					message:
						"Song search functionality is coming soon. Try searching for sound effects instead.",
				};
			}

			const page = query.page ?? 1;
			const pageSize = query.page_size ?? 20;
			const sort = query.sort ?? "downloads";
			const minRating = query.min_rating ?? 3;
			const commercialOnly = query.commercial_only !== "false";
			const searchQuery = query.q ?? "";

			const params = new URLSearchParams({
				query: searchQuery,
				token,
				page: String(page),
				page_size: String(pageSize),
				sort: searchQuery && sort === "score" ? "score" : `${sort}_desc`,
				fields: FREESOUND_FIELDS,
			});
			params.append("filter", "duration:[* TO 30.0]");
			params.append("filter", `avg_rating:[${minRating} TO *]`);
			if (commercialOnly) {
				params.append("filter", `license:(${COMMERCIAL_LICENSES_FILTER})`);
			}
			params.append("filter", EFFECT_TAGS_FILTER);

			const upstream = await fetch(
				`https://freesound.org/apiv2/search/text/?${params.toString()}`,
			);
			if (!upstream.ok) {
				set.status = upstream.status === 429 ? 429 : 502;
				return { error: "Failed to search sounds" };
			}

			const data = (await upstream.json()) as FreesoundResponse;
			if (!Array.isArray(data.results)) {
				set.status = 502;
				return { error: "Invalid response from Freesound API" };
			}

			return {
				count: data.count,
				next: data.next,
				previous: data.previous,
				results: data.results.map(transformFreesoundResult),
				query: searchQuery,
				type,
				page,
				pageSize,
				sort,
				minRating,
			};
		},
		{ query: soundsSearchQuery },
	)
	.post(
		"/feedback",
		async ({ request, body, set }) => {
			if (await isRateLimited(request)) {
				set.status = 429;
				return { error: "Too many requests" };
			}

			const store = bindings.FEEDBACK;
			if (!store) {
				set.status = 503;
				return {
					error: "Feedback is not configured",
					message:
						"Bind a FEEDBACK KV namespace on the API worker to collect feedback.",
				};
			}

			const message = body.message.trim();
			if (message.length === 0) {
				set.status = 400;
				return { error: "Message is required" };
			}

			const entry = {
				id: crypto.randomUUID(),
				message,
				createdAt: new Date().toISOString(),
			};
			await store.put(
				`feedback:${entry.createdAt}:${entry.id}`,
				JSON.stringify(entry),
			);

			set.status = 201;
			return { entry };
		},
		{
			body: t.Object({
				message: t.String({ minLength: 1, maxLength: MAX_FEEDBACK_LENGTH }),
			}),
		},
	)
	// .compile() is required, it triggers AoT compilation at startup
	.compile();
