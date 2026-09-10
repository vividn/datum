// The browser console's handle on datum.
//
// `window.datum` is the command seam (`run`/`runLine`/`setRunner` from
// $lib/run.ts) plus two reads of what got stored, published on the window
// so the app can be driven and inspected the way the CLI is — from a
// prompt. It adds no behaviour: both reads are CLI commands the app already
// runs, `db backup -` and `tail --json`, and `layouts` is the layout store
// ($lib/layout/store.ts), itself nothing but CLI commands.
//
// It is also what web/tests binds to. A stable, named handle is what keeps
// the tests from reaching into a module graph whose filenames change with
// every build.

import { run, runLine, setRunner, type CommandRunner, type RunResult } from './run.js';
import { storedEntries, type StoredEntry } from './backup.js';
import { tailListing, type TailColumn, type TailRow } from './tail.js';
import * as layouts from './layout/store.js';
import { get } from 'svelte/store';
import { apply, layout } from './layout/current.js';
import type { Layout } from './layout/types.js';

export interface DatumConsole {
	/** Run one command, argv without the program name. */
	run(argv: string[], stdin?: string): Promise<RunResult>;
	/** Shell-split a line and run it, as the REPL does. */
	runLine(line: string): Promise<RunResult>;
	/** Swap the active runner; returns the one replaced. */
	setRunner(next: CommandRunner): CommandRunner | null;
	/** The most recent entries as the CLI stores them, newest first. */
	entries(limit?: number): Promise<StoredEntry[]>;
	/** `datum tail --json`: the rows and the painted columns the tail pane lays out. */
	tailRows(): Promise<{ rows: TailRow[]; columns: TailColumn[] }>;
	/** The saved layouts in the `_internal` database: list, load, save, restore. */
	layouts: typeof layouts;
	/** The live tree (da-jdop): what is on screen, and the one way to change it. */
	layout: { current(): Layout; apply(op: (layout: Layout) => Layout): void };
}

/** Publish the handle on `target` (the window, in the app). */
export function installConsole(target: Record<string, unknown>): void {
	const handle: DatumConsole = {
		run,
		runLine,
		setRunner,
		entries: storedEntries,
		tailRows: tailListing,
		layouts,
		layout: { current: () => get(layout), apply }
	};
	target.datum = handle;
}
