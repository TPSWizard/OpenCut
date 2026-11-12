/*
 * ConzaCut service worker. Pages (navigations) go network-first with the
 * last copy as the offline fallback (and /projects as the last resort);
 * the app's hashed bundles and bundled assets (sounds, fonts, flags,
 * shapes, icons, effects) are cached on first use and served from the
 * cache after that. The cache is named after the build, so a new deploy
 * installs a new worker, the page offers a reload, and the old cache goes
 * when the new worker takes over.
 */
const BUILD_ID = "__BUILD_ID__";
const CACHE = `conzacut-${BUILD_ID}`;
const SHELL = "/projects";
const STATIC = /^\/(assets|effects|flags|fonts|icons|logos|shapes|sounds|platform-guides|plugin-sdk|plugins|open-graph)\//;
const ALWAYS = new Set(["/manifest.json", "/favicon.ico"]);

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
	if (request.method !== "GET") return;
	const url = new URL(request.url);
	if (url.origin !== self.location.origin) return;
	if (request.mode === "navigate") {
		event.respondWith(networkFirst(request));
		return;
	}
	if (STATIC.test(url.pathname) || ALWAYS.has(url.pathname)) {
		event.respondWith(cacheFirst(request));
	}
});

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
