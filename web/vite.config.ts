import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin, type ViteDevServer, type PreviewServer } from 'vite';

// OPFS-backed SQLite needs SharedArrayBuffer, which the browser only hands
// to a cross-origin-isolated document — so the document response has to
// carry both of these headers or `db.ts` silently falls back to an
// in-memory database and nothing the user logs survives a reload.
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
 * behind Cloudflare — has to emit the same two headers.
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
	optimizeDeps: {
		exclude: ['@sqlite.org/sqlite-wasm']
	}
});
