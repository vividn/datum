// The browser console's handle on datum.
//
// `window.datum` publishes the app's command line on the window, so it can
// be driven and inspected from a prompt the way the CLI is, plus two reads
// of what got stored. It adds no behaviour: both reads are CLI commands the
// app already runs, `db backup -` and `tail --json`, and `layouts` is the
// layout store ($lib/layout/store.ts), itself nothing but CLI commands.
//
// `run`/`runLine` are the page's own path, `engine.press` (da-6918): the
// command goes through the mounted REPL, is echoed into the scrollback and
// receipted on the bottom bar, and the epoch is bumped so every pane
// re-reads. A hand at the console gets what a hand at the prompt gets. The
// literal seam is still here under `runQuiet`/`runQuietLine`, for the
// caller that wants a write and no refresh — but it is the exception and
// says so in its name; it used to be `run`, and a silent `run` cost a
// false bug report on 2026-09-11.
//
// It is also what web/tests binds to. A stable, named handle is what keeps
// the tests from reaching into a module graph whose filenames change with
// every build.

import { run, runLine, setRunner, type CommandRunner, type RunResult } from './run.js';
import { press } from './layout/engine.js';
import { storedEntries, type StoredEntry } from './backup.js';
import { tailListing, type TailColumn, type TailRow } from './tail.js';
import * as layouts from './layout/store.js';
import * as shell from './layout/shell.js';
import { last, pills, removePill, setPills, type LastRun, type Pill } from './bar.js';
import { cursor, pointAt, setCursor, type Cursor } from './cursor.js';
import { shellSplit } from './shellSplit.js';
import { openSpec, openTrack, type TrackOptions } from './specModal.js';
import { get } from 'svelte/store';
import { apply, layout } from './layout/current.js';
import { keyOf, session, withSession } from './layout/session.js';
import { findTab } from './layout/tree.js';
import { isWidget } from './layout/types.js';
import type { Layout, WidgetKindMeta } from './layout/types.js';
import { KINDS } from './layout/registry.js';

export interface DatumConsole {
	/** Run one command as the page does — echoed, receipted, panes refreshed. Argv without the program name. */
	run(argv: string[], stdin?: string): Promise<RunResult>;
	/** Shell-split a line and run it, exactly as typing it into the bar does. */
	runLine(line: string): Promise<RunResult>;
	/** The seam alone: the command runs and stores, and nothing on the page moves. For a caller that wants no refresh. */
	runQuiet(argv: string[], stdin?: string): Promise<RunResult>;
	/** `runQuiet`'s line form: shell-split, run, nothing moves. */
	runQuietLine(line: string): Promise<RunResult>;
	/** Swap the active runner; returns the one replaced. */
	setRunner(next: CommandRunner): CommandRunner | null;
	/** The most recent entries as the CLI stores them, newest first. */
	entries(limit?: number): Promise<StoredEntry[]>;
	/** `datum tail --json`: the rows and the painted columns the tail pane lays out. */
	tailRows(): Promise<{ rows: TailRow[]; columns: TailColumn[] }>;
	/** The layout store: the person's own in `_internal` and a kit's in the data db (da-gg0v). */
	layouts: typeof layouts;
	/**
	 * The live tree (da-jdop): what is on screen, and the one way to change
	 * it — plus what THIS SESSION has set over it (da-vhwq), key by key, so
	 * a test can say that a tab's settings were forgotten when it closed.
	 */
	layout: {
		current(): Layout;
		apply(op: (layout: Layout) => Layout): void;
		kinds(): WidgetKindMeta[];
		session(): Record<string, Record<string, unknown>>;
		/** What the widget in tab `tabId` is DRAWN from: the tree's settings with this session's over them. */
		settings(tabId: string): Record<string, unknown> | undefined;
	};
	/** The root tabs (da-0a5o): open a layout by name, save, restore, save as, lock. */
	shell: typeof shell;
	/**
	 * The bottom bar's pills: what the time cursor will set, and a test does.
	 * And `last`, the bar's receipt: the newest command recorded and its
	 * result, whichever door it came in by.
	 */
	bar: { pills(): Pill[]; setPills(next: Pill[]): void; removePill(id: string): void; last(): LastRun | null };
	/**
	 * The time cursor (da-wnt2): the instant the page is pointed at, as a
	 * click on a day sets it. `of()` is the whole SELECTION (da-rskp) — its
	 * `rel` when it is relative and its `span` when it is a range — and
	 * `point()` is the one door that sets any of those.
	 */
	cursor: {
		at(): Date | null;
		set(at: Date | null): void;
		of(): Cursor | null;
		point(sel: (Partial<Cursor> & { at?: Date }) | null): void;
	};
	/**
	 * Open the Track modal (da-94q1) and run what it answers with, as
	 * `Track +` does — with a field or a verb already chosen, which is the
	 * door a filtered list's `+` comes in by. Null if it was dismissed.
	 */
	track(options?: TrackOptions): Promise<RunResult | null>;
	/**
	 * Open the same modal on `field`'s spec (da-ci85o) and run the one
	 * `spec set` it answers with. Null if it was dismissed.
	 */
	spec(field: string): Promise<RunResult | null>;
}

/** Publish the handle on `target` (the window, in the app). */
export function installConsole(target: Record<string, unknown>): void {
	const handle: DatumConsole = {
		// No pills (the bottom bar's time cursor is the bar's, not a
		// console line's): what is typed here is what runs.
		run: (argv, stdin) => press(argv, undefined, stdin),
		runLine: (line) => press(shellSplit(line), line),
		runQuiet: run,
		runQuietLine: runLine,
		setRunner,
		entries: storedEntries,
		tailRows: tailListing,
		layouts,
		layout: {
			current: () => get(layout),
			apply,
			kinds: () => Object.values(KINDS).map((k) => k.meta),
			session: () => get(session),
			settings: (tabId) => {
				const tree = get(layout);
				const tab = findTab(tree, tabId);
				return tab && isWidget(tab.content)
					? withSession(tab.content.settings, get(session)[keyOf(tree, tabId)])
					: undefined;
			}
		},
		shell,
		bar: { pills: () => get(pills), setPills, removePill, last: () => get(last) },
		cursor: { at: () => get(cursor)?.at ?? null, set: setCursor, of: () => get(cursor), point: pointAt },
		track: async (options) => {
			const argv = await openTrack(options);
			return argv && (await press(argv));
		},
		spec: async (field) => {
			const argv = await openSpec(field);
			return argv && (await press(argv));
		}
	};
	target.datum = handle;
}
