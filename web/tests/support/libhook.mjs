// Importing `#lib/layout/registry.ts` into node, where its components
// cannot be loaded: every `.svelte` import becomes an empty stub, and a
// relative `./x.js` under our own `src/` becomes `./x.ts`. `#lib/x.ts` needs
// nothing: node reads it from package.json's `imports`.
//
// ⚠ The `.js` → `.ts` rewrite applies to OUR sources ONLY, which is what
// `context.parentURL` decides. A package's own relative imports are really
// `.js` — `svelte/store` reaches `./internal/index.js` that way — and
// rewriting those resolves a file that does not exist. The pure tests
// (`lanes-pure`, `nowpoint-pure`, `selection-pure`) always had that guard;
// the two registry importers did not, and it went unnoticed until registry
// first reached a module that imports `svelte/store` (da-7q8r).
//
// `$app/env` is SvelteKit's, and node cannot resolve it: it becomes
// `browser = false`, which is the honest value there. `database.ts`,
// `changes.ts`, `cliStore.ts` and `storageState.ts` import it, so without
// this nothing that reaches `run.ts` loads in node at all (da-mgxdt).
//
// One copy, because three files needed the same fix and a fourth wanted the
// hook: a rule kept in three places is a rule that gets fixed in one of them.

import { registerHooks } from 'node:module';

const EMPTY_MODULE = 'data:text/javascript,export default {}';
const APP_ENV = 'data:text/javascript,export const browser = false';

/** Register the resolve hook. */
export function resolveLibAsSource() {
	registerHooks({
		resolve(specifier, context, next) {
			if (specifier.endsWith('.svelte')) return { url: EMPTY_MODULE, shortCircuit: true };
			if (specifier === '$app/env') return { url: APP_ENV, shortCircuit: true };
			const ours = specifier.startsWith('.') && context.parentURL?.includes('/src/lib/');
			return next(ours ? specifier.replace(/\.js$/, '.ts') : specifier, context);
		}
	});
}
