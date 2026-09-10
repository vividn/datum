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
	 * Whether the process wrote to the database, by effect: the worker reads
	 * SQLite's change counter either side of it ($lib/cli.ts), so no list of
	 * verbs is consulted. Absent where nothing ran.
	 */
	changed?: boolean;
}

export type CommandRunner = (argv: string[], stdin?: string) => Promise<RunResult>;

/** Generic failure, as `datum-cli`'s `main` reports one. */
export const EXIT_ERROR = 1;

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
 * Run one command, with `stdin` on its standard input if it reads any.
 * Never rejects — a thrown error becomes a nonzero result.
 */
export async function run(argv: string[], stdin?: string): Promise<RunResult> {
	if (argv.length === 0) return { argv, code: 0, stdout: '', stderr: '' };
	try {
		runner ??= (await import('./runners/wasi.js')).wasiRun;
		return await runner(argv, stdin);
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
