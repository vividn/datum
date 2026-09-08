// Where the database lives, as something the UI can react to.
//
// `storageKind()` answers for the instant it is called, which was enough when
// the answer could not change. It can now: a tab that opened in memory because
// another held the OPFS pool takes the pool over when that tab closes, and
// moves its entries across. A page that read the kind once goes on warning
// about data that is no longer at risk.
//
// One store rather than a subscription in each component, because the banner
// and the repl frame are rendering the SAME fact and should never be able to
// disagree about it.

import { readable } from 'svelte/store';
import { browser } from '$app/environment';
import { initDb, onStorageChange, storageKind, type StorageKind } from '$lib/db.js';

/**
 * `null` is "not determined yet"; `'failed'` is "we tried and it did not
 * open". Both must withhold any reassurance about persistence, but a reader
 * who is never going to get an answer deserves to be told so (#108).
 */
export type StorageState = StorageKind | 'failed' | null;

export const storage = readable<StorageState>(null, (set) => {
	// A `$storage` in a template subscribes during server-side render too,
	// and there is no `Worker` there. `initDb()` would construct one. Every
	// other caller reaches the database from `onMount`, which is browser-only
	// by construction; a store start function is not, so it has to say so.
	if (!browser) return;
	// Idempotent: the page's own init shares this promise.
	initDb().then(
		() => set(storageKind()),
		() => set('failed')
	);
	return onStorageChange(set);
});

/**
 * Whether entries typed now would be lost on reload.
 *
 * `null` is "we do not know yet" and is deliberately NOT degraded — the
 * database is opening, and flashing a data-loss warning at every visitor for
 * the first few hundred milliseconds would teach them to ignore it.
 */
export function isDegraded(kind: StorageState): boolean {
	return kind === 'failed' || kind === 'memory' || kind === 'memory-other-tab';
}
