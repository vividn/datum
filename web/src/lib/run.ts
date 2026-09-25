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
 */
export type CommandRunner = (
	argv: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string,
	stage?: Stage
) => Promise<RunResult>;

/** Generic failure, as `datum-cli`'s `main` reports one. */
export const EXIT_ERROR = 1;

/**
 * The database `argv` names for itself: whether it names one at all, and
 * which — `--db X` or `--db=X`, the first of them, stopping at `--`, which
 * is the CLI's own pre-scan to the token (`extract_global_db_flag`).
 *
 * `--db` with nothing after it names no database but is still a flag the
 * caller wrote, so `named` and `db` are separate answers.
 *
 * `-D` is NOT here, and reading it as a database flag was a bug, not a
 * cosmetic disagreement (vremya on #22): the CLI's `-D` is `--full-day`
 * (`crates/datum-parse/src/alias/scope.rs`), so `occur coffee -D` on a
 * `?db=other` page looked to this scanner like a command that had already
 * chosen its database, got no `--db other`, and logged the day to `datum`.
 * Only `--db` selects a database, here and in the worker's own scanner
 * ($lib/cli.ts `dbNamed`).
 */
function namedDb(argv: string[]): { named: boolean; db?: string } {
	for (let i = 0; i < argv.length; i += 1) {
		const word = argv[i];
		if (word === '--') break;
		if (word === '--db') return { named: true, db: argv[i + 1] };
		if (word.startsWith('--db=')) return { named: true, db: word.slice(5) };
	}
	return { named: false };
}

/**
 * `argv` against the page's database (da-vk1x).
 *
 * The ONE place `?db=<name>` becomes a flag: every command the app runs on
 * the reader's behalf — the panes, the bar's line, the widgets, the day
 * strip's bands — passes through `run`, so none of them carries the query
 * itself. A caller that names its own database keeps it, which is what
 * leaves the layout store's `--db _internal` alone ($lib/layout/store.ts).
 *
 * Prefixed, because `--db` is a global flag the CLI reads before it looks
 * for a command word, and a multi-side `a , b` run has one of them for both
 * sides. Nothing is added at the default: `--db datum` and no flag at all
 * resolve to the same file, and the argv the runner sees is then exactly
 * the argv the caller asked for.
 */
export function dbArgv(argv: string[], db: string = currentDb()): string[] {
	if (db === DEFAULT_DB || argv.length === 0 || namedDb(argv).named) return argv;
	return ['--db', db, ...argv];
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
	return namedDb(argv).db ?? db;
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
	stage?: Stage
): Promise<RunResult> {
	if (argv.length === 0) return { argv, code: 0, stdout: '', stderr: '' };
	const against = dbArgv(argv);
	try {
		runner ??= (await import('./runners/wasi.js')).wasiRun;
		return await runner(against, stdin, onStderr, key, stage);
	} catch (e) {
		// A runner that throws is a bug in the runner, not in the caller: the
		// seam still owes the UI a result it can render.
		return {
			argv,
			code: EXIT_ERROR,
			stdout: '',
			stderr: `error: ${e instanceof Error ? e.message : String(e)}`
		};
	}
}

/** Shell-split a REPL line and run it. Blank lines are a no-op success. */
export function runLine(line: string): Promise<RunResult> {
	return run(shellSplit(line));
}
