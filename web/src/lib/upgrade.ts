// The page's own upgrade (da-ylkin): the root bar's «↑ New version», shown once
// SvelteKit's `updated` has read a `/_app/version.json` naming another build,
// loads the deployed one past every cache. vividn, 2026-11-04: «This does a
// forced refresh without cache and should always go to the latest version».
// The page's build, not the CLI's `datum upgrade`.
//
// It waits for this tab's CLI to be idle, every command answered and no sync
// session applying: a local-first page must never lose a write to its own
// upgrade. Every CLI process, a sync session's too, is a request `dry()` sees,
// and each commits whole, so a session cut between two of them resumes at the
// next load as one cut by a closed tab does. Then the worker is unregistered
// and its caches dropped, so the reload is no navigation of the old worker's,
// whose network-first answer falls back to the shell it cached once the origin
// is 3 s late, and the next load registers the deployed worker and precaches
// afresh. A wait that never ends leaves both in place: a command stuck that
// long holds every other command too.

import { get } from 'svelte/store';
import { dry } from './db.ts';
import { settled } from './run.ts';
import { syncStates } from './sync.ts';

const applying = (): boolean => Object.values(get(syncStates)).some((state) => state.applying);

/** Resolves once nothing is out to the CLI and no sync session is applying. */
async function idle(): Promise<void> {
	for (;;) {
		await settled();
		if (dry() && !applying()) return;
		await new Promise((resolve) => setTimeout(resolve, 50));
	}
}

/** Once the CLI is idle: unregister the service worker, drop its caches, and reload. */
export async function upgrade(): Promise<void> {
	await idle();
	await (await navigator.serviceWorker?.getRegistration())?.unregister();
	for (const key of (await globalThis.caches?.keys()) ?? []) await caches.delete(key);
	await idle();
	location.reload();
}
