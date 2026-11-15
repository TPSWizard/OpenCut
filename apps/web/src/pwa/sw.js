/*
 * ConzaCut service worker. Pages (navigations) go network-first with the
 * last copy as the offline fallback (and /projects as the last resort);
 * the app's hashed bundles and bundled assets (sounds, fonts, flags,
 * shapes, icons, effects) are cached on first use and served from the
 * cache after that. The cache is named after the build, so a new deploy
 * installs a new worker, the page offers a reload, and the old cache goes
 * when the new worker takes over. Files shared from another app's share
 * sheet arrive as a POST to /share-target (see receiveShare).
 */
const BUILD_ID = "__BUILD_ID__";
const CACHE = `conzacut-${BUILD_ID}`;
const SHELL = "/projects";
const STATIC = /^\/(assets|effects|flags|fonts|icons|logos|shapes|sounds|platform-guides|plugin-sdk|plugins|open-graph)\//;
const ALWAYS = new Set(["/manifest.json", "/favicon.ico"]);
/** Files shared to the installed app; `src/pwa/share-inbox.ts` reads them. */
const SHARE_INBOX = "share-inbox";

self.addEventListener("install", (event) => {
	event.waitUntil(
		(async () => {
			const cache = await caches.open(CACHE);
			try {
				await cache.addAll([SHELL, "/manifest.json"]);
			} catch {
				// Offline at install time: the shell is cached on the next visit.
			}
		})(),
	);
});

self.addEventListener("activate", (event) => {
	event.waitUntil(
		(async () => {
			const keys = await caches.keys();
			await Promise.all(keys.filter((key) => key.startsWith("conzacut-") && key !== CACHE).map((key) => caches.delete(key)));
			await self.clients.claim();
		})(),
	);
});

self.addEventListener("message", (event) => {
	if (event.data && event.data.type === "SKIP_WAITING") {
		self.skipWaiting();
	}
});

self.addEventListener("fetch", (event) => {
	const { request } = event;
	const url = new URL(request.url);
	if (request.method === "POST" && url.origin === self.location.origin && url.pathname === "/share-target") {
		event.respondWith(receiveShare(request));
		return;
	}
	if (request.method !== "GET") return;
	if (url.origin !== self.location.origin) return;
	if (request.mode === "navigate") {
		event.respondWith(networkFirst(request));
		return;
	}
	if (STATIC.test(url.pathname) || ALWAYS.has(url.pathname)) {
		event.respondWith(cacheFirst(request));
	}
});

/**
 * The manifest's share target: keeps the shared files in the inbox (in
 * order, with their names) and opens the projects page, which makes a
 * project for them. Text and links on their own are not media.
 */
async function receiveShare(request) {
	try {
		const form = await request.formData();
		const files = form.getAll("media").filter((item) => typeof item === "object" && item && "size" in item && item.size > 0);
		await caches.delete(SHARE_INBOX);
		const inbox = await caches.open(SHARE_INBOX);
		await Promise.all(
			files.map((file, index) =>
				inbox.put(
					new Request(`/share-inbox/file?i=${index}`),
					new Response(file, {
						headers: {
							"Content-Type": file.type || "application/octet-stream",
							"X-File-Name": encodeURIComponent(file.name || `shared-${index + 1}`),
							"X-Last-Modified": String(file.lastModified || Date.now()),
						},
					}),
				),
			),
		);
		return Response.redirect(`/projects?shared=${files.length}`, 303);
	} catch {
		return Response.redirect("/projects?shared=error", 303);
	}
}

async function cacheFirst(request) {
	const cache = await caches.open(CACHE);
	const hit = await cache.match(request);
	if (hit) return hit;
	const response = await fetch(request);
	if (response.ok) {
		cache.put(request, response.clone()).catch(() => {});
	}
	return response;
}

async function networkFirst(request) {
	const cache = await caches.open(CACHE);
	try {
		const response = await fetch(request);
		if (response.ok) {
			cache.put(request, response.clone()).catch(() => {});
		}
		return response;
	} catch {
		const cached = await cache.match(request);
		if (cached) return cached;
		const shell = await cache.match(SHELL);
		if (shell) return shell;
		return new Response("ConzaCut is offline and this page has not been opened before.", {
			status: 503,
			headers: { "Content-Type": "text/plain; charset=utf-8" },
		});
	}
}
