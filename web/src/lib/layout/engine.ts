// What the page used to own and the panes hang off: the epoch, readiness,
// the widget bar's buttons, and the one path a widget-bar press takes.
//
// `epoch` is a single number. The root reads it once and passes it down
// the tree as a prop; no pane subscribes to changes on its own, so there
// is one refresh path and every pane is on it (the bead's trap list).
//
// `press` is the module-level dispatcher for the widget bar (da-nz29):
// the bar composes an argv and this decides where it runs. With a REPL on
// screen the press goes THROUGH it — `runArgv` echoes the executed argv
// into the scrollback and the recall history, which is the teach loop —
// and the REPL's own `ran` bumps the epoch. With no REPL mounted (the
// reader closed it) the press runs through the seam directly and bumps
// the epoch itself, so the panes still refresh; the echo is simply
// nowhere to put, and comes back the moment a REPL is added.

import { writable } from 'svelte/store';
import { initDb } from '$lib/db.js';
import { run, type RunResult } from '$lib/run.js';
import { widgetsFor, type FieldListing, type Widget as FieldWidget } from '$lib/widgets.js';

/** Bumped after every command; the panes re-run theirs when it changes. */
export const epoch = writable(0);

/** True once `initDb()` has resolved: the REPL and the bar are usable. */
export const ready = writable(false);

/** The widget bar's buttons — `datum fields --json`, rebuilt on every beat. */
export const fields = writable<FieldWidget[]>([]);

export interface ReplLike {
	runArgv(argv: string[]): Promise<RunResult>;
}

let repl: ReplLike | null = null;

/** The REPL on screen announces itself; the newest one mounted wins. */
export function registerRepl(instance: ReplLike): () => void {
	repl = instance;
	return () => {
		if (repl === instance) repl = null;
	};
}

export async function boot(): Promise<void> {
	await initDb();
	ready.set(true);
	await refresh();
}

/**
 * The bar's buttons are built from `datum fields` — what has been logged
 * and what has been declared — so a newly tracked field has to grow one:
 * hence a rebuild on the same beat the panes refresh on.
 */
export async function refresh(): Promise<void> {
	const listed = await run(['fields', '--json']);
	if (listed.code !== 0) return;
	// Guarded like the tail pane's own listing: stdout that is not the JSON
	// asked for (a partial write, a crashed process) keeps the bar it had
	// rather than throwing out of the one refresh path every pane is on.
	try {
		fields.set(widgetsFor(JSON.parse(listed.stdout) as FieldListing[]));
	} catch {
		// The bar stays as it was.
	}
}

/**
 * A command ran, here or in another tab. After *every* command, not only
 * the ones that mutate: re-running three read-only commands against a
 * local database costs nothing, and a list of which verbs write would
 * drift out of step with the CLI and leave a pane showing stale data.
 */
export async function ran(): Promise<void> {
	epoch.update((n) => n + 1);
	await refresh();
}

/** A widget-bar press. Through the REPL when there is one; see above. */
export async function press(argv: string[]): Promise<RunResult> {
	if (repl) return repl.runArgv(argv);
	const result = await run(argv);
	await ran();
	return result;
}
