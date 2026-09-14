// The bottom bar's own state (da-0a5o): its pills, and the last command
// run anywhere on the page.
//
// Pills are what the bar is built to receive and nothing here sets: the
// shared time cursor vividn named next — a click on the dayview loading a
// `-t 14:30` pill, x-able — puts them here, and a line submitted from the
// bar, like a press in the fields widget, is what was typed followed by
// the pills' text (`pressWithPills` in layout/engine.ts: the CLI takes no
// flag before its verb). `setPills` and `removePill` are the whole of the
// contract.
//
// `prefill` puts a line into the bar's prompt without running it — the
// fields widget's `add <field> ` (da-c7u8); the bar takes it and clears it.
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
import type { RunResult } from './run.js';

/**
 * How long a ghost is on screen, and so how long a spent pill takes to
 * fade with it (da-xfa4). One constant: the fade is in Prompt.svelte's
 * CSS, the two timers that take the ghost and the pills away are here and
 * in PromptBar.svelte, and all three have to agree.
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
 */
export const ghost = writable<Ghost | null>(null);

/** `press` says a command ran that nobody typed. */
export const showGhost = (line: string): void => ghost.set({ line });

export interface LastRun {
	line: string;
	ranAt: number;
	result: RunResult;
}

export const last = writable<LastRun | null>(null);

/** `record` says a command ran. */
export const noteRun = (line: string, ranAt: number, result: RunResult): void =>
	last.set({ line, ranAt, result });
