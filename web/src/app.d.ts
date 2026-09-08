// See https://kit.svelte.dev/docs/types#app
// for information about these interfaces
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface Platform {}
	}

	// Injected by vite at build time (see vite.config.ts). Inside `declare
	// global` because this file is a module — a top-level declaration here is
	// module-scoped and invisible to components.
	const __DATUM_VERSION__: string;
	const __DATUM_COMMIT__: string;
}

export {};
