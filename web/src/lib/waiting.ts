// What the page is waiting on before the CLI answers (da-bhudz).
//
// The first command after a load can be slow — a schema upgrade after a
// deploy runs inside it, a minute on a big history — and everything queues
// behind it. The shell draws at once from the last visit regardless; these
// are what lets it say what it is waiting for (`Waiting.svelte`) and tell a
// pane's copy from the last visit when its fresh answer has landed
// (`Stale.svelte`). Set by $lib/db.ts, from the worker's own messages.

import { writable } from 'svelte/store';

/** Whether any CLI command has answered since the page loaded. */
export const first = writable(false);

/** When the running process began upgrading a schema, on `performance.now()`; null when none is. */
export const upgrading = writable<number | null>(null);

/** Since when, on `performance.now()`, another tab has kept the files from this one; null when none is (da-89kom). */
export const blocked = writable<number | null>(null);

/** Bumped each time the worker's queue empties: every command sent has answered. */
export const idle = writable(0);

/** A command answered; `drained` when it was the last one out. */
export function answered(drained: boolean): void {
	first.set(true);
	upgrading.set(null);
	if (drained) idle.update((n) => n + 1);
}
