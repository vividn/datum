/**
 * "The database changed" — one channel, whoever caused it.
 *
 * SQLite gives consistency, not notification: with every tab on one
 * database (see $lib/db.worker.ts), tab B's write is durable and tab A
 * does not know it happened. vividn hit that within a minute of the site
 * going up, and it reads as data loss even when nothing was lost.
 *
 * This is deliberately in FRONT of the storage layer rather than inside it,
 * because the same signal is wanted for a change that arrives from a sync
 * peer later. A listener should not have to care which it was.
 *
 * BroadcastChannel is same-origin and needs no isolation, so this works
 * whether the tabs share the owner's connection or fell back to one
 * database each.
 */
import { browser } from '$app/environment';

const CHANNEL = 'datum-db-changes';

let channel: BroadcastChannel | null = null;
/** A tab's own id, so it can ignore the echo of its own write. */
let self_ = '';

function open(): BroadcastChannel | null {
	if (channel) return channel;
	// Not "does the constructor exist": node has a global BroadcastChannel
	// too, and one opened during the server render is a live handle that
	// keeps the preview process alive after the last test has passed — the
	// Web job hung on exactly this on 2026-09-09. Outside a browser there is
	// no other tab to hear from, so nothing is opened. The constructor check
	// stays for the few older engines the app still runs on; its absence
	// costs live updates, never correctness.
	if (!browser || typeof BroadcastChannel === 'undefined') return null;
	self_ = crypto.randomUUID();
	channel = new BroadcastChannel(CHANNEL);
	return channel;
}

/** Say that this tab wrote something. Safe to call when nobody is listening. */
export function announceChange(): void {
	open()?.postMessage({ from: self_ });
}

/**
 * Run `handler` when ANOTHER tab writes. Returns an unsubscribe.
 *
 * The `from` check is what stops a write echoing back into the tab that made
 * it and refreshing it a second time — harmless but wasteful, and it would
 * make the refresh count a poor witness in any test.
 */
export function onExternalChange(handler: () => void): () => void {
	const c = open();
	if (!c) return () => {};
	const listener = (e: MessageEvent) => {
		if (e.data?.from !== self_) handler();
	};
	c.addEventListener('message', listener);
	return () => c.removeEventListener('message', listener);
}
