// What the page used to own and the panes hang off: the epoch, readiness,
// the widget bar's buttons, and the one path a widget-bar press takes.
//
// `epoch` is a single number. The root reads it once and passes it down
// the tree as a prop; no pane subscribes to changes on its own, so there
// is one refresh path and every pane is on it (the bead's trap list).
//
// `press` is the module-level dispatcher for the fields widget (da-nz29):
// the widget composes an argv and this decides where it runs. With a REPL on
// screen the press goes THROUGH it — `runArgv` echoes the executed argv
// into the scrollback and the recall history, which is the teach loop —
// and the REPL's own `ran` bumps the epoch. With no REPL mounted (the
// reader closed it) the press runs through the seam directly and bumps
// the epoch itself, so the panes still refresh; the echo is simply
// nowhere to put, and comes back the moment a REPL is added.

import { get, writable } from 'svelte/store';
import { pills } from '$lib/bar.js';
import { initDb } from '$lib/db.js';
import { fileCommand } from '$lib/dbio.js';
import { record } from '$lib/replStore.js';
import { run, type RunResult } from '$lib/run.js';
import { shellSplit } from '$lib/shellSplit.js';
import { buttonsFor, type FieldButton, type FieldListing, type NowActive } from '$lib/fields.js';

/** Bumped after every command; the panes re-run theirs when it changes. */
export const epoch = writable(0);

/** True once `initDb()` has resolved: the REPL and the bar are usable. */
export const ready = writable(false);

/** The fields widget's entries — `datum fields --json` and `now --json`, rebuilt on every beat. */
export const fields = writable<FieldButton[]>([]);

export interface ReplLike {
	runArgv(argv: string[], line?: string): Promise<RunResult>;
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
 * The widget's entries are built from `datum fields` — what has been logged
 * and what has been declared — and painted by `datum now`, what is on: so
 * a newly tracked field has to grow one and a started block has to fill,
 * hence a rebuild on the same beat the panes refresh on.
 */
export async function refresh(): Promise<void> {
	const listed = await run(['fields', '--json']);
	if (listed.code !== 0) return;
	const now = await run(['now', '--json']);
	// Guarded like the tail pane's own listing: stdout that is not the JSON
	// asked for (a partial write, a crashed process) keeps the bar it had
	// rather than throwing out of the one refresh path every pane is on.
	try {
		fields.set(buttonsFor(JSON.parse(listed.stdout) as FieldListing[], activeOf(now)));
	} catch {
		// The bar stays as it was.
	}
}

/** `now --json`'s active fields; none when it failed, so every field is drawn off rather than lost. */
function activeOf(now: RunResult): NowActive[] {
	try {
		return now.code === 0 ? ((JSON.parse(now.stdout) as { active?: NowActive[] }).active ?? []) : [];
	} catch {
		return [];
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

/**
 * A command from anywhere but a prompt of its own — a widget-bar press,
 * the bottom bar's line (da-0a5o), which is echoed and recalled as typed.
 * Through the REPL when there is one; see above. Without one it is
 * recorded here, so the shared history and the bottom bar's receipt still
 * see it, and the epoch is bumped here.
 */
export async function press(argv: string[], line?: string): Promise<RunResult> {
	if (repl) return repl.runArgv(argv, line);
	const ranAt = Date.now();
	const result = await (fileCommand(argv) ?? run(argv));
	record(line ?? result.argv.join(' '), ranAt, result);
	await ran();
	return result;
}

/**
 * A command from the bottom bar's line or the fields widget, with the
 * bar's pills (a `-t 14:30` cursor) after it — after, because the CLI
 * takes no flag before its verb. The one path both take (da-c7u8), so a
 * pill lands on a press exactly as on a typed line. A typed `line` is
 * echoed as typed, the pills' text after it.
 */
export const pressWithPills = (argv: string[], line?: string): Promise<RunResult> =>
	press(
		[...argv, ...get(pills).flatMap((pill) => shellSplit(pill.text))],
		line === undefined ? undefined : [line, ...get(pills).map((pill) => pill.text)].join(' ')
	);
