// Importing `$lib/layout/registry.ts` into node, where its components
// cannot be loaded: every `.svelte` import becomes an empty stub, `$lib/x.js`
// becomes the source's `x.ts`, and a relative `./x.js` under our own `src/`
// becomes `./x.ts`.
//
// ⚠ The `.js` → `.ts` rewrite applies to OUR sources ONLY, which is what
// `context.parentURL` decides. A package's own relative imports are really
// `.js` — `svelte/store` reaches `./internal/index.js` that way — and
// rewriting those resolves a file that does not exist. The pure tests
// (`lanes-pure`, `nowpoint-pure`, `selection-pure`) always had that guard;
// the two registry importers did not, and it went unnoticed until registry
// first reached a module that imports `svelte/store` (da-7q8r).
//
// One copy, because three files needed the same fix and a fourth wanted the
// hook: a rule kept in three places is a rule that gets fixed in one of them.

import { registerHooks } from 'node:module';

const EMPTY_MODULE = 'data:text/javascript,export default {}';

/** Register the resolve hook. `base` is the calling test's `import.meta.url`. */
export function resolveLibAsSource(base) {
	const lib = new URL('../src/lib/', base);
	registerHooks({
		resolve(specifier, context, next) {
			if (specifier.endsWith('.svelte')) return { url: EMPTY_MODULE, shortCircuit: true };
			if (!/^(\.|\$lib\/)/.test(specifier)) return next(specifier, context);
			const url = new URL(specifier.replace(/^\$lib\//, lib.href), context.parentURL);
			const ours = context.parentURL?.includes('/src/lib/');
			return next(ours ? url.href.replace(/\.js$/, '.ts') : url.href, context);
		}
	});
}
