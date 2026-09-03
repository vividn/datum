// The command seam.
//
// Every datum command the web app runs goes through `run(argv)` and comes
// back as a [`RunResult`] — an exit code plus captured stdout/stderr. The UI
// never reaches into the parser or the database directly, so swapping the
// implementation swaps nothing else: da-1vat replaces the TypeScript runner
// in `./runners/local.ts` with the real CLI compiled to wasm by calling
// `setRunner`, and the panes, widget bar and REPL keep working unchanged.
//
// `argv` carries the command words *without* the program name, matching what
// the native REPL hands the parser (`crates/datum-cli/src/repl.rs` prepends
// "datum" itself before dispatching a line).

import { shellSplit } from './shellSplit.js';

/** The outcome of one command, shaped like a process exit. */
export interface RunResult {
	/** The argv that produced this result, after shell-splitting. */
	argv: string[];
	/** 0 on success. See EXIT_* below. */
	code: number;
	stdout: string;
	stderr: string;
}

export type CommandRunner = (argv: string[]) => Promise<RunResult>;

/** Generic failure, as `datum-cli`'s `main` reports one. */
export const EXIT_ERROR = 1;
/** A duplicate / "already have this" outcome (`EXIT_DUPLICATE` in main.rs). */
export const EXIT_DUPLICATE = 11;
/** No runner knows this command — the shell convention for "not found". */
export const EXIT_UNKNOWN_COMMAND = 127;

// Null until the first `run`, which lazily imports the TypeScript runner.
// Lazily, for two reasons: it keeps the seam from depending on any one
// implementation, and installing the wasm runner up front means the
// TypeScript one is never fetched at all.
let runner: CommandRunner | null = null;

/**
 * Install `next` as the active runner and return the one it replaced (null
 * if none had run yet), so a caller can restore it. This is the whole
 * extension point: the wasm CLI arrives as one call here.
 */
export function setRunner(next: CommandRunner): CommandRunner | null {
	const previous = runner;
	runner = next;
	return previous;
}

/** Run one command. Never rejects — a thrown error becomes a nonzero result. */
export async function run(argv: string[]): Promise<RunResult> {
	if (argv.length === 0) return { argv, code: 0, stdout: '', stderr: '' };
	try {
		runner ??= (await import('./runners/local.js')).localRun;
		return await runner(argv);
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
