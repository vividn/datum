// The command seam.
//
// Every datum command the web app runs goes through `run(argv)` and comes
// back as a [`RunResult`] — an exit code plus captured stdout/stderr. The UI
// never reaches into the database directly: behind the seam is the real
// `datum` CLI compiled to wasm, one process per command ($lib/runners/
// wasi.ts, da-g8o8), so what a pane shows is what the CLI printed.
//
// `argv` carries the command words *without* the program name, matching what
// the native REPL hands the parser (`crates/datum-cli/src/repl.rs` prepends
// "datum" itself before dispatching a line).

import { currentDb, DEFAULT_DB } from './database.js';
import { dbFlag } from './dbFlag.js';
import type { Stage } from './dbProtocol.js';
import { shellSplit } from './shellSplit.js';

/** The outcome of one command, shaped like a process exit. */
export interface RunResult {
	/** The argv that produced this result, after shell-splitting. */
	argv: string[];
	/** 0 on success; otherwise the CLI's own exit code. */
	code: number;
	stdout: string;
	stderr: string;
	/**
	 * The databases the process wrote to (`datum`, `_internal`), by effect:
	 * the worker reads each file's SQLite change counter either side of it
	 * ($lib/cli.ts), so no list of verbs is consulted. Absent where nothing
	 * ran.
	 */
	changed?: string[];
	/**
	 * Nothing ran: a newer command with the same key took this one's place
	 * in the queue (da-eoq3). Not a failure — the caller ignores it.
	 */
	superseded?: boolean;
	/** `/data/stage` after the process, when the caller gave it one (`Stage`). */
	stage?: Stage;
}

/**
 * `onStderr` hears the process's stderr as it is written, before the result.
 * `key` names who is asking (a pane): a newer command with the same key drops
 * this one if it has not started yet. Without a key a command always runs.
 * `env` is added to the process's environment (`DATUM_SHOW`, $lib/echo.ts;
 * `DATUM_DB`, `dbEnv`).
 * `pane`: a pane's refresh, which waits behind every other command and is
 * run cheapest first (da-icmo1).
 */
export type CommandRunner = (
	argv: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string,
	stage?: Stage,
	env?: string[],
	pane?: boolean
) => Promise<RunResult>;

let keys = 0;
/** A `key` of the caller's own: a widget takes one, and its newer refresh drops its older, unstarted one (da-3wi18). */
export const runKey = (): string => `key:${++keys}`;

/** Generic failure, as `datum-cli`'s `main` reports one. */
export const EXIT_ERROR = 1;

/** A failed command's stderr as an Error, without the prefix `main` («Error: ») or the REPL («error: ») put on it; `fallback` when it said nothing. */
export const cliError = (stderr: string, fallback = ''): Error =>
	new Error(stderr.trim().replace(/^[Ee]rror: /, '') || fallback);

/**
 * The env that runs `argv` against the page's database (da-vk1x).
 *
 * The ONE place `?db=<name>` reaches the CLI: every command the app runs on
 * the reader's behalf — the panes, the bar's line, the widgets, the day
 * strip's bands — passes through `run`, so none of them carries the query
 * itself. A caller that names its own database keeps it, which is what
 * leaves the layout store's `--db _internal` alone ($lib/layout/store.ts).
 *
 * `DATUM_DB`, not `--db`: the CLI reads it below the flag, and a flag in
 * argv lands in every entry's `raw_input`, so `why` quoted «--db work occur
 * coffee» for a line typed as `occur coffee` (da-7xne1, as da-vx31e did for
 * `--show`). Nothing is added at the default.
 */
export function dbEnv(argv: string[], db: string = currentDb()): string[] {
	return db === DEFAULT_DB || dbFlag(argv) ? [] : [`DATUM_DB=${db}`];
}

/**
 * The database `argv` will actually run against — the one it names, else
 * the page's.
 *
 * For the caller that has to SAY which database it is about before the
 * command runs: `db purge`'s dialog stands in for the CLI's TTY prompt
 * ($lib/dbio.ts), and a prompt that names a different database than the
 * flag does is worse than no prompt at all.
 */
export function dbOf(argv: string[], db: string = currentDb()): string {
	return dbFlag(argv)?.db ?? db;
}

// Null until the first `run`, which imports the CLI runner lazily: this
// module is imported during prerendering, where there is no worker to run it.
let runner: CommandRunner | null = null;

/**
 * Install `next` as the active runner and return the one it replaced (null
 * if none had run yet), so a caller can restore it. Tests spy through it.
 */
export function setRunner(next: CommandRunner): CommandRunner | null {
	const previous = runner;
	runner = next;
	return previous;
}

/** Commands sent through `run` and not yet answered, and who waits for none. */
let out = 0;
const quiet: (() => void)[] = [];

/**
 * Resolves once every command sent through `run` has answered: the panes a
 * refresh set off as well as the bar's reads, which now run ahead of them.
 */
export function settled(): Promise<void> {
	return out ? new Promise((resolve) => quiet.push(resolve)) : Promise.resolve();
}

/**
 * Run one command, with `stdin` on its standard input if it reads any, and
 * `stage`'s files in `/data/stage` if it is given one.
 * Never rejects — a thrown error becomes a nonzero result.
 */
export async function run(
	argv: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string,
	stage?: Stage,
	env?: string[],
	pane?: boolean
): Promise<RunResult> {
	if (argv.length === 0) return { argv, code: 0, stdout: '', stderr: '' };
	out++;
	try {
		runner ??= (await import('./runners/wasi.js')).wasiRun;
		const result = await runner(argv, stdin, onStderr, key, stage, [...dbEnv(argv), ...(env ?? [])], pane);
		// `sync new-invite`, a revoke, an admin…: its leader reads the setup again (da-6kpj5).
		if (argv.includes('sync') && result.changed?.length) {
			void import('./sync.js').then(({ syncTouched }) => syncTouched(dbOf(argv)));
		}
		return result;
	} catch (e) {
		// A runner that throws is a bug in the runner, not in the caller: the
		// seam still owes the UI a result it can render.
		return {
			argv,
			code: EXIT_ERROR,
			stdout: '',
			stderr: `error: ${e instanceof Error ? e.message : String(e)}`
		};
	} finally {
		// Asked again a task later: a pane whose earlier run was still out
		// sends its next one only once that run's answer reaches it.
		if (--out === 0) setTimeout(() => out === 0 && quiet.splice(0).forEach((resolve) => resolve()));
	}
}

/** Shell-split a REPL line and run it. Blank lines are a no-op success. */
export function runLine(line: string): Promise<RunResult> {
	return run(shellSplit(line));
}
