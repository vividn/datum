/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

import { build, files, version } from '$service-worker';

const CACHE_NAME = `datum-${version}`;
const ASSETS = [...build, ...files];

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
			await (self as unknown as ServiceWorkerGlobalScope).clients.claim();
		})
	);
});

self.addEventListener('fetch', (event: FetchEvent) => {
	if (event.request.method !== 'GET') return;

	const url = new URL(event.request.url);
	if (url.origin !== location.origin) return;

	// Cache-first for static assets
	const isAsset = ASSETS.includes(url.pathname);
	if (isAsset) {
		event.respondWith(
			caches.match(event.request).then((cached) => cached ?? fetch(event.request))
		);
	} else {
		// Network-first for navigation
		event.respondWith(
			fetch(event.request).catch(() =>
				caches.match('/').then((cached) => cached ?? new Response('Offline', { status: 503 }))
			)
		);
	}
});
