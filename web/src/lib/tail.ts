// `datum tail --json`: the rows the tail pane lists, and the CLI's own
// cells for them.
//
// The REPL's `tail` and the tail pane are two surfaces of one row set
// (da-6ec2): the REPL shows the CLI's text table, and the pane lays the
// same cells out in a real `<table>`. The cells come from the CLI —
// `table_columns_with`, the cell-building `format_table_with` pads, painted
// because the page runs it with FORCE_COLOR=1 — so nothing about what a cell
// SAYS is decided in the browser.

import { tailArgv } from './calls.js';
import { cliError, run } from './run.js';

/** One column of the CLI's table: its header word and one cell per row. */
export interface TailColumn {
	/** `_n` | `action` | `time` | `field` | `state` | `dur` | `hid`. */
	key: string;
	/** The header word the CLI prints, painted. */
	header: string;
	/** One painted, unpadded cell per row, in the listing's order. */
	cells: string[];
}

/** One row of `datum tail --json`. Only the keys the app reads are named. */
export interface TailRow {
	hid: string;
	/** `field:<ident>`: the row's place among rows of one time, and of none. */
	slug: string;
	field: string;
	/** `point` | `switch` | `block` | `block-switch` | `point-switch`. */
	verb: string;
	/** The stamp the `time` column shows, RFC 3339: a timeless row's is the one it was created at. */
	time: string | null;
	/** Which stamp `time` is — `occur`, or `create` for a timeless row. */
	time_metric: string | null;
	state_label: string | null;
	quick_id: string | null;
	/*
	 * The row's state as the three values a transition is made of
	 * (da-fknn8). On a terminal the previous and following states are the
	 * one-character LEAD and TRAIL slots that bracket the chip; here they
	 * are words, and `state_body` is the CLI's own painted chip WITHOUT
	 * them — so `StateCell.svelte` draws the trio without drawing any part
	 * of it twice.
	 */
	/**
	 * The field's state just before this row; `"IGNORED"` where it was not
	 * tracked, and `null` on a row that names no transition at all (a
	 * timeless row sits at no instant, so it has no before).
	 */
	previous_state: unknown;
	previous_state_label: string | null;
	/** The hex the terminal's LEAD slot paints; `null` only for "field ended". */
	previous_state_color: string | null;
	/** The terminal's LEAD slot itself, painted — one character, all shapes resolved. */
	previous_state_cell: string;
	/** The state that continues forward past this row. */
	following_state: unknown;
	following_state_label: string | null;
	following_state_color: string | null;
	/** The terminal's TRAIL slot itself, painted; empty where there is none. */
	following_state_cell: string;
	/** The painted state chip (or occur dot) alone, without the two slots. */
	state_body: string;
	/** This row's state is the one it was already in — the CLI's `!` (da-qfv0). */
	state_repeat: boolean;
	/** An occur dot's declared keys' values, as typed (`25 15`), drawn after the cell (da-545c9); absent before it. */
	keys?: string;
	/** How long a block lasted, ending at `time`; null for a row with no duration (DayStrip.svelte's first entry). */
	dur_seconds: number | null;
	[key: string]: unknown;
}

/** `datum tail --json`, and `head --json`. */
export interface TailListing {
	rows: TailRow[];
	columns: TailColumn[];
}

/**
 * Which page of the listing: how many rows (`-n`), and — for a page above
 * the one on screen — the row they are listed above (`--above`).
 *
 * `above` is a row's hid, and the CLI's bound is that row's place in the
 * listing's own order, `occur_time ?? create_time` (da-2vgj1), so a page
 * is exactly the rows above it, each of two rows at one instant once
 * (da-mhcla).
 */
export interface TailPage {
	count?: number;
	above?: string;
	/**
	 * Only this field's rows — `datum tail [FIELD]`, its first positional
	 * (da-8810n). What the bar's field pill narrows the pane to; absent is
	 * every field, which is what `tail` prints on its own.
	 */
	field?: string;
}

/**
 * The listing, oldest first, the way `tail` prints. Rejects on a failed command.
 * With a `key` (`run`'s), null when a newer listing of that key took its place unrun.
 */
export function tailListing(page?: TailPage): Promise<TailListing>;
export function tailListing(page: TailPage, key: string): Promise<TailListing | null>;
export async function tailListing(page: TailPage = {}, key?: string): Promise<TailListing | null> {
	const result = await run(tailArgv(page), undefined, undefined, key, undefined, undefined, true);
	if (result.superseded) return null;
	if (result.code !== 0) throw cliError(result.stderr, `tail exited ${result.code}`);
	const doc = JSON.parse(result.stdout) as Partial<TailListing>;
	return { rows: doc.rows ?? [], columns: doc.columns ?? [] };
}
