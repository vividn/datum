// The browser console's handle on datum.
//
// `window.datum` is the command seam (`run`/`runLine`/`setRunner` from
// $lib/run.ts) plus a raw read of what got stored, published on the window
// so the app can be driven and inspected the way the CLI is — from a
// prompt. It adds no behaviour: every function here is one the UI already
// calls.
//
// It is also what web/tests binds to. Those tests are written against the
// seam rather than the TypeScript behind it precisely so they survive the
// wasm swap (da-1vat), and a stable, named handle is what keeps them from
// reaching into a module graph whose filenames change with every build.

import { run, runLine, setRunner, type CommandRunner, type RunResult } from './run.js';
import { getRecentEntries } from './db.js';
import type { Entry } from './types.js';

export interface DatumConsole {
	/** Run one command, argv without the program name. */
	run(argv: string[]): Promise<RunResult>;
	/** Shell-split a line and run it, as the REPL does. */
	runLine(line: string): Promise<RunResult>;
	/** Swap the active runner; returns the one replaced. */
	setRunner(next: CommandRunner): CommandRunner | null;
	/** The most recent entries as stored, newest first. */
	entries(limit?: number): Promise<Entry[]>;
}

/** Publish the handle on `target` (the window, in the app). */
export function installConsole(target: Record<string, unknown>): void {
	const handle: DatumConsole = { run, runLine, setRunner, entries: getRecentEntries };
	target.datum = handle;
}
