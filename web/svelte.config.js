import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({
			fallback: 'index.html'
		}),
		// The browser's HTTP cache never answers for the worker script or its
		// imports, so a deploy is seen at the next update check (da-v1wqf).
		serviceWorker: { register: true, options: { updateViaCache: 'none' } }
	}
};

export default config;
