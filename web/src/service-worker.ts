/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

import { build, files, version } from '$service-worker';
import { workerAssets } from '$lib/workerAssets.generated.js';

const CACHE_NAME = `datum-${version}`;

// `build` is derived from the client Vite manifest, which does not know
// about Vite's separate worker bundle — the SQLite worker and sqlite3.wasm
// live there. `workerAssets` is that bundle, enumerated at build time by
// the datum-worker-precache plugin in vite.config.ts (da-d1l9).
// '/' because nothing else caches it: adapter-static is in fallback mode,
// so no page is prerendered and `$service-worker`'s `prerendered` list is
// empty. Without it the shell the fetch handler falls back to is never
// there, and the app does not load offline at all.
const SHELL = '/';

const ASSETS = [SHELL, ...build, ...files, ...workerAssets];

/**
 * The two headers that make a document cross-origin isolated, which is what
 * gates `SharedArrayBuffer` — and with it the plain OPFS VFS, the one that
 * lets two tabs hold the database at once.
 *
 * Set here, on every same-origin response this worker serves, so isolation is
 * a property of the app rather than of whichever host happens to serve it. A
 * plain bucket sends neither; the deployment's Cloudflare rule adds them to
 * the first load, and this covers every load after. Safe to add blindly
 * because the app loads nothing cross-origin (measured: zero `src`/`href`
 * to another origin, zero css imports), so `require-corp` has nothing to
 * refuse.
 */
const ISOLATION: Record<string, string> = {
	'Cross-Origin-Opener-Policy': 'same-origin',
	'Cross-Origin-Embedder-Policy': 'require-corp'
};

/** The same response with the isolation headers on it, when it can be rebuilt. */
function isolated(response: Response): Response {
	// An opaque or redirect response cannot be reconstructed — its body and
	// headers are not readable here — and `new Response` on one throws. Pass
	// those through; they are not the document, and the document is what has
	// to carry the headers.
	if (response.status === 0 || response.type === 'opaque' || response.type === 'opaqueredirect') {
		return response;
	}
	const headers = new Headers(response.headers);
	for (const [name, value] of Object.entries(ISOLATION)) headers.set(name, value);
	return new Response(response.body, {
		status: response.status,
		statusText: response.statusText,
		headers
	});
}

/** Everything under here is content-hashed, so it is safe to cache forever. */
const IMMUTABLE = '/_app/immutable/';

self.addEventListener('install', (event: ExtendableEvent) => {
	event.waitUntil(
		caches
			.open(CACHE_NAME)
			.then((cache) => cache.addAll(ASSETS))
			.then(() => (self as unknown as ServiceWorkerGlobalScope).skipWaiting())
	);
});

self.addEventListener('activate', (event: ExtendableEvent) => {
	event.waitUntil(
		caches.keys().then(async (keys) => {
			for (const key of keys) {
				if (key !== CACHE_NAME) await caches.delete(key);
			}
		})
	);

	// Claimed outside `waitUntil`, and deliberately not awaited. In Firefox this
	// promise sometimes never settles, and the worker stays in `activating` for as
	// long as its `waitUntil` is pending — a state in which its clients' fetches
	// never complete. The visitor types their first command, the lazy fetch of
	// datum-parse.wasm hangs, and nothing is echoed, stored, or reported.
	// Measured 2026-09-07 on Playwright's Firefox, fresh profile per trial:
	// 27 of 62 first commands lost while awaited, 0 of 10 when not. Chromium
	// never reproduced it. Activation does not depend on the claim, so nothing
	// here needs to wait for it.
	void (self as unknown as ServiceWorkerGlobalScope).clients.claim();
});

/** Serve from the cache, and fill it on a miss so the next visit is offline-safe. */
async function cacheFirst(request: Request): Promise<Response> {
	const cached = await caches.match(request);
	if (cached) return cached;
	const response = await fetch(request);
	if (response.ok) (await caches.open(CACHE_NAME)).put(request, response.clone());
	return response;
}

self.addEventListener('fetch', (event: FetchEvent) => {
	if (event.request.method !== 'GET') return;

	const url = new URL(event.request.url);
	if (url.origin !== location.origin) return;

	// Cache-first for the shell and every precached asset, and for anything
	// else the build put under the immutable directory — a hashed name never
	// means two things. The cache is per build (`version`), so a deploy
	// installs a new service worker and the shell refreshes with it.
	if (ASSETS.includes(url.pathname) || url.pathname.startsWith(IMMUTABLE)) {
		event.respondWith(cacheFirst(event.request).then(isolated));
		return;
	}

	// Network-first for navigation, falling back to the cached shell. Only
	// for navigation: answering a script or wasm request with HTML is worse
	// than answering it with an error, because the failure it produces
	// names the wrong thing.
	event.respondWith(
		fetch(event.request)
			.catch(async () => {
				const cached = await caches.match(event.request);
				if (cached) return cached;
				if (event.request.mode !== 'navigate') return new Response('Offline', { status: 503 });
				return (await caches.match(SHELL)) ?? new Response('Offline', { status: 503 });
			})
			.then(isolated)
	);
});
