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

/**
 * Say that this tab wrote to `dbs` (by name: `datum`, `_internal`), or to
 * all of them when unsaid. Safe to call when nobody is listening.
 */
export function announceChange(dbs?: string[]): void {
	open()?.postMessage({ from: self_, dbs });
}

/**
 * Run `handler` when ANOTHER tab writes to `db`. Returns an unsubscribe.
 *
 * The `from` check is what stops a write echoing back into the tab that made
 * it and refreshing it a second time — harmless but wasteful, and it would
 * make the refresh count a poor witness in any test. The `db` check is the
 * same economy across databases: a saved layout does not re-run the panes.
 *
 * COALESCED, on the handler's own promise (da-s3pt): a refresh here costs a
 * process per pane, and a BURST next door — forty entries logged in the
 * other tab, or a sync peer handing over a night of changesets — bought one
 * refresh each, so the pile of processes grew with the burst and the next
 * read the reader asked for queued behind all of it. While a refresh is
 * outstanding, further changes mark ONE more after it rather than one each,
 * so the cost of a burst is how long a refresh takes rather than how many
 * writes there were — twelve next door measured three refreshes, not twelve
 * (tests/burst.test.mjs). The last of them is after the last write, so what
 * the panes settle on is still what the database now holds.
 *
 * So the handler must be the whole refresh — `onExternalChange(ran)`, not
 * `onExternalChange(() => void ran())`, whose promise nothing can wait for.
 */
export function onExternalChange(handler: () => unknown, db = 'datum'): () => void {
	const c = open();
	if (!c) return () => {};
	let outstanding = false;
	let again = false;
	async function refresh(): Promise<void> {
		if (outstanding) return void (again = true);
		outstanding = true;
		do {
			again = false;
			// A refresh that failed is not a reason to stop hearing the next
			// change: the handler owns its own errors, this owns the beat.
			try {
				await handler();
			} catch {
				// Kept quiet, as a missed refresh is.
			}
		} while (again);
		outstanding = false;
	}
	const listener = (e: MessageEvent) => {
		if (e.data?.from !== self_ && (!e.data?.dbs || e.data.dbs.includes(db))) void refresh();
	};
	c.addEventListener('message', listener);
	return () => c.removeEventListener('message', listener);
}
