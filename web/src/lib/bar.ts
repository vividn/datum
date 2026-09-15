// The page's ONE command line, as state (da-0a5o, da-3zon): the text
// being typed, the pills riding after it, the ghost over it, the submit
// that runs it, and the last command run anywhere on the page.
//
// Called `bar.ts` because the bottom bar was its only view. It is not any
// more: an `input` WIDGET is the same command line drawn in a pane
// (da-3zon), so the buffer lives here and both views bind to it — type in
// either and both show it, Enter in either runs it. Two local strings
// synced by effects is how two boxes drift by a keystroke; there is one
// string and it is `text`.
//
// Pills are what the bar is built to receive and nothing here sets: the
// shared time cursor vividn named next — a click on the dayview loading a
// `-t 14:30` pill, x-able — puts them here, and a line submitted from the
// bar, like a press in the fields widget, is what was typed followed by
// the pills' text (`pressWithPills` in layout/engine.ts: the CLI takes no
// flag before its verb). `setPills` and `removePill` are the whole of the
// contract.
//
// `prefill` puts a line into the command line without running it — the
// fields widget's `add <field> ` (da-c7u8); the bar takes the signal and
// clears it, and `focusLine` decides which box the caret lands in.
//
// `last` is every command run anywhere — a widget-bar press, a command
// pane's ▶, the bar itself — because every one
// of them goes through `record` in $lib/replStore.ts, which tells this
// store. It is what the bar's result area is drawn from ($lib/BarResult
// .svelte): the CLI's own documents, or its own lines when it echoed
// none. Not restored across a reload, unlike the scrollback: a result is
// about the command just run, and on a fresh page none has been.
//
// `ghost` is the narrower thing the prompt itself shows (da-jprg) — see
// below.

import { get, writable } from 'svelte/store';
import { pressWithPills } from './layout/engine.js';
import type { RunResult } from './run.js';
import { shellSplit } from './shellSplit.js';

/**
 * How long a ghost is on screen, and so how long a spent pill takes to
 * fade with it (da-xfa4). One constant: the fade is in Prompt.svelte's
 * CSS and the two timers that take the ghost and the pills away are both
 * below, and all three have to agree.
 */
export const GHOST_MS = 1000;

export interface Pill {
	id: string;
	/** What goes on the command line, verbatim (`-t 14:30`). */
	text: string;
	/** What kind of thing it is (`time`, `range`), for the bar to colour. */
	kind: string;
	/** A locked pill survives a successful press instead of clearing. */
	locked?: boolean;
}

export const pills = writable<Pill[]>([]);

export const setPills = (next: Pill[]): void => pills.set(next);

export const removePill = (id: string): void => pills.update((all) => all.filter((p) => p.id !== id));

export const toggleLockPill = (id: string): void =>
	pills.update((all) => all.map((p) => (p.id === id ? { ...p, locked: !p.locked } : p)));

/**
 * The pills a press has just spent, on their way out (da-xfa4).
 *
 * A SEPARATE store, not a flag on `pills`: a spent pill is no longer in
 * the argv a press builds and no longer the time cursor's other face
 * ($lib/cursor.ts) — it is a receipt fading beside the ghost of the
 * command it went into. Keeping it out of `pills` is what lets both of
 * those readers stay exactly as they were.
 */
export const spent = writable<Pill[]>([]);

/**
 * After a press SUCCEEDS: unlocked pills clear, locked ones stay for the
 * next line.
 *
 * The cleared ones move to `spent` for the length of a ghost — vividn: "The
 * pills should be used with the ghost text, so they can be part of it and
 * fade with it" — and are gone after it.
 */
export const clearUnlockedPills = (): void => {
	const doomed = get(pills).filter((p) => !p.locked);
	if (!doomed.length) return;
	pills.update((all) => all.filter((p) => p.locked));
	spent.set(doomed);
	setTimeout(() => spent.update((all) => (all === doomed ? [] : all)), GHOST_MS);
};

/**
 * What is typed at the command line, wherever it is drawn. THE buffer:
 * every `Prompt.svelte` binds its box straight to this store.
 */
export const text = writable('');

/**
 * Run a line the reader typed — the one submit path, for the bar and for
 * every input widget (da-3zon).
 *
 * `pressWithPills` is the rest of it: the pills go into the argv after the
 * verb, the line is echoed as typed, the receipt lands on `last` and in
 * the shared history, and a success spends the unlocked pills. Clearing
 * the box is the caller's, because the box is `text` and both views empty
 * together the moment it is set.
 *
 * The import back into `layout/engine.ts` closes a cycle — engine reads
 * the pills from here — which ESM allows because neither module CALLS the
 * other while it is loading.
 */
export const submitLine = (line: string): Promise<RunResult> =>
	pressWithPills(shellSplit(line), line);

/**
 * The boxes the command line is drawn in, in the order they mounted: the
 * bar's, and one per input widget on screen.
 *
 * `focusLine` is where a focus that is not a click goes — a `prefill` from
 * the fields widget today. It lands in the last WIDGET mounted, and in the
 * bar when there is none: the box a reader put on the page is the one they
 * are looking at, and the bar is the fallback that is always there.
 */
const lines: { el: HTMLInputElement; widget: boolean }[] = [];

export function registerLine(el: HTMLInputElement, widget: boolean): () => void {
	const entry = { el, widget };
	lines.push(entry);
	return () => {
		const i = lines.indexOf(entry);
		// Guarded: a `splice(-1, 1)` on a second unregister would drop
		// somebody else's box instead of nothing.
		if (i >= 0) lines.splice(i, 1);
	};
}

export const focusLine = (): void => {
	const widgets = lines.filter((l) => l.widget);
	(widgets.at(-1) ?? lines.find((l) => !l.widget))?.el.focus();
};

export const prefill = writable<string | null>(null);

export interface Ghost {
	/** The command, as the scrollback echoes it. */
	line: string;
}

/**
 * The last command the reader did NOT type into a prompt — a widget-bar
 * press, the console (`press` in layout/engine.ts, da-jprg).
 *
 * The bar shows it where the placeholder goes, dim, and lets it fade: the
 * GUI teaches the CLI, so a press has to say what it ran, and the REPL's
 * scrollback is behind a tab on a phone. Never the input's value — a box
 * that refills itself after Enter reads as text the reader typed and may
 * edit, which is the bug this store replaced. A line the reader typed
 * never comes back to the box at all.
 *
 * A fresh object every time, identical line or not, so a second press
 * restarts the fade instead of riding out the first one's.
 *
 * The timer is HERE, as `spent`'s is, rather than in a view: there is more
 * than one prompt on the page now (da-3zon) and a ghost taken by whichever
 * one mounted first would be a ghost the others never saw.
 */
export const ghost = writable<Ghost | null>(null);

let haunting: ReturnType<typeof setTimeout> | undefined;

/** `press` says a command ran that nobody typed. */
export const showGhost = (line: string): void => {
	const next: Ghost = { line };
	ghost.set(next);
	clearTimeout(haunting);
	haunting = setTimeout(() => ghost.update((g) => (g === next ? null : g)), GHOST_MS);
};

export interface LastRun {
	line: string;
	ranAt: number;
	result: RunResult;
}

export const last = writable<LastRun | null>(null);

/** `record` says a command ran. */
export const noteRun = (line: string, ranAt: number, result: RunResult): void =>
	last.set({ line, ranAt, result });
