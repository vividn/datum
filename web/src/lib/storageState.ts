// Where the database lives, as something the UI can react to.
//
// `storageKind()` answers for the instant it is called, and the answer can
// change: a tab that opened in memory because the browser refused it the file
// goes back for it, and moves its entries across when it gets it. A page that
// read the kind once goes on warning about data that is no longer at risk.
//
// One store rather than a subscription in each component, because the banner
// and the repl frame are rendering the SAME fact and should never be able to
// disagree about it.

import { derived, readable } from 'svelte/store';
import { browser } from '$app/environment';
import {
	initDb,
	onDurableWrite,
	onStorageChange,
	storageKind,
	type StorageKind
} from '$lib/db.js';

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

/** Whether this tab has asked for persistence yet; once per session. */
let asked = false;

/**
 * Whether the browser has promised not to evict this origin's storage
 * (da-frg2). `null` is unknown or unsupported.
 *
 * Eviction is per origin — OPFS, IndexedDB and the cache share one bucket
 * and one policy — and until the origin is granted persistence, Chromium and
 * Firefox may clear it under storage pressure. So the app asks, but only
 * once there is something to lose: after the first entry lands durably, not
 * on an empty page, where Firefox's prompt would be the first thing a visitor
 * sees. A returning visit reads `persisted()` first: an origin already
 * granted is never asked again, one not yet granted is asked once per session.
 *
 * Asked from the page, not the worker, so the answer and the banner's
 * sentence come from one place. The request is never awaited by the write:
 * `persist()` may wait on a human.
 */
export const persisted = readable<boolean | null>(null, (set) => {
	if (!browser || !('storage' in navigator && 'persist' in navigator.storage)) return;
	const manager = navigator.storage;
	let known: boolean | null = null;
	// `then` rather than `await`: a `persist` that throws synchronously is
	// caught the same way as one that rejects.
	const settle = (ask: () => Promise<boolean>) =>
		Promise.resolve()
			.then(ask)
			.then(
				(answer) => set((known = answer)),
				() => set((known = null))
			);
	settle(() => manager.persisted());
	return onDurableWrite(() => {
		if (asked || known) return;
		asked = true;
		settle(() => manager.persist());
	});
});

/**
 * Whether entries typed now would be lost on reload.
 *
 * `null` is "we do not know yet" and is deliberately NOT degraded — the
 * database is opening, and flashing a data-loss warning at every visitor for
 * the first few hundred milliseconds would teach them to ignore it.
 */
export function isDegraded(kind: StorageState): boolean {
	return kind === 'failed' || kind === 'memory';
}

/**
 * What the app says about where the entries are — the gear menu's beta note
 * (RootBar.svelte), and the bottom bar's title while storage is degraded
 * (PromptBar.svelte). One store, because the two are the same claim and
 * must never be able to disagree, wherever each of them is drawn (da-y992
 * moved the hint to the top bar and left the alarm on the bottom one).
 */
export const storageSentence = derived([storage, persisted], ([$storage, $persisted]) =>
	$storage === 'failed'
		? 'The database could not be opened, so nothing you log here will be saved.'
		: $storage === null
			? 'Checking where your entries are stored…'
			: $storage === 'memory'
				? 'Entries are held in memory only — reloading this page discards them.'
				: 'Your entries are stored in this browser only: clearing site data deletes them, and ' +
					($persisted
						? 'the browser has promised to keep them.'
						: 'the browser may clear them if space runs short — install the app to keep them.')
);
