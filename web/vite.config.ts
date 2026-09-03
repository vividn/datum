import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin, type ViteDevServer, type PreviewServer } from 'vite';

// Cross-origin isolation, which the browser grants a document only when
// its response carries both of these headers.
//
// The database no longer depends on it: SQLite runs in a worker now
// ($lib/db.worker.ts) and the SAH-pool VFS needs only that worker's sync
// access handle, so entries persist with or without isolation (verified
// against a plain static server sending neither header). The headers stay
// for what isolation still buys — SharedArrayBuffer and Atomics, which the
// plain OPFS VFS wants and a threaded wasm CLI (da-1vat) will need — and
// the deployment should keep sending them.
const CROSS_ORIGIN_ISOLATION = {
	'Cross-Origin-Opener-Policy': 'same-origin',
	'Cross-Origin-Embedder-Policy': 'require-corp'
};

/**
 * Set the isolation headers on every dev and preview response.
 *
 * Vite's `server.headers` option is not enough: it applies to Vite's own
 * static middleware, while SvelteKit's SSR handler writes the HTML response
 * (the one that has to be isolated) with headers of its own. Middleware
 * installed here runs ahead of both.
 *
 * The production bundle is static, so the deployment — Scaleway bucket
 * behind Cloudflare — should emit the same two headers.
 */
function crossOriginIsolation(): Plugin {
	const install = (server: ViteDevServer | PreviewServer) => {
		server.middlewares.use((_req, res, next) => {
			for (const [name, value] of Object.entries(CROSS_ORIGIN_ISOLATION)) {
				res.setHeader(name, value);
			}
			next();
		});
	};

	return {
		name: 'datum-cross-origin-isolation',
		configureServer(server) {
			install(server);
		},
		configurePreviewServer(server) {
			install(server);
		}
	};
}

export default defineConfig({
	plugins: [sveltekit(), crossOriginIsolation()],
	// The database worker ($lib/db.worker.ts) imports sqlite-wasm
	// dynamically, and Vite's default worker format — iife — cannot
	// code-split. It is a module worker anyway; say so.
	worker: {
		format: 'es'
	},
	optimizeDeps: {
		exclude: ['@sqlite.org/sqlite-wasm']
	}
});
