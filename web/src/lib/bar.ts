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
// `last` is every command run anywhere — a REPL line, a widget-bar press,
// the input widget, a command pane's ▶, the bar itself — because every one
// of them goes through `record` in $lib/replStore.ts, which tells this
// store. The bar shows the line for a beat, as if typed, and the receipt
// beside it: the CLI's own first line, reduced to its action word and hid.
// Not restored across a reload, unlike the scrollback: a receipt is about
// the command just run, and on a fresh page none has been.

import { writable } from 'svelte/store';
import { stripAnsi } from './ansi.js';
import type { RunResult } from './run.js';

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

/** After a press SUCCEEDS: unlocked pills clear, locked ones stay for the next line. */
export const clearUnlockedPills = (): void => pills.update((all) => all.filter((p) => p.locked));

export const prefill = writable<string | null>(null);

export interface LastRun {
	line: string;
	ranAt: number;
	result: RunResult;
}

export const last = writable<LastRun | null>(null);

/** `record` says a command ran. */
export const noteRun = (line: string, ranAt: number, result: RunResult): void =>
	last.set({ line, ranAt, result });

export interface Receipt {
	text: string;
	error: boolean;
}

const firstLine = (text: string): string =>
	stripAnsi(text)
		.split('\n')
		.find((l) => l.trim())
		?.trim() ?? '';

/**
 * What the bar says about a result: on failure the CLI's first line of
 * stderr; otherwise its first line of stdout, cut to the action word and
 * the hid when it is the CLI's echo (`CREATE 8vcb6 coffee:…` → `CREATE
 * 8vcb6`) — the truth as the CLI printed it, never rebuilt (da-nz29).
 */
export function receiptOf(result: RunResult): Receipt {
	if (result.code !== 0) return { text: firstLine(result.stderr) || `exit ${result.code}`, error: true };
	const line = firstLine(result.stdout);
	const echo = /^([A-Z]+)\s+(\S+)/.exec(line);
	return { text: echo ? `${echo[1]} ${echo[2]}` : line, error: false };
}
