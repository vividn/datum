// `datum tail --json`: the rows the tail pane lists, and the CLI's own
// cells for them.
//
// The REPL's `tail` and the tail pane are two surfaces of one row set
// (da-6ec2): the REPL shows the CLI's text table, and the pane lays the
// same cells out in a real `<table>`. The cells come from the CLI —
// `table_columns_with`, the cell-building `format_table_with` pads, painted
// because the page runs it with DATUM_COLOR=1 — so nothing about what a cell
// SAYS is decided in the browser.

import { run } from './run.js';

/** One column of the CLI's table: its header word and one cell per row. */
export interface TailColumn {
	/** `time` | `field` | `state` | `dur` | `_n` | `hid`. */
	key: string;
	/** The header word the CLI prints, painted. */
	header: string;
	/** One painted, unpadded cell per row, in the listing's order. */
	cells: string[];
}

/** One row of `datum tail --json`. Only the keys the app reads are named. */
export interface TailRow {
	hid: string;
	field: string;
	/** The stamp the `time` column shows, RFC 3339; `null` for a timeless row. */
	time: string | null;
	state_label: string | null;
	quick_id: string | null;
	[key: string]: unknown;
}

/**
 * Which page of the listing: how many rows (`-n`), and — for a page above
 * the one on screen — the stamp they must land at or before (`-t`).
 *
 * `before` is a row's own `time` handed straight back: `tail -t` takes an
 * RFC 3339 instant at the CLI's full precision, and its bound is inclusive
 * (`<=`, `list.rs`), so the row it names comes back too — the caller drops
 * what it already holds by hid. Rounding the stamp first would drop rows
 * between the rounded instant and the real one, so it is not rounded.
 */
export interface TailPage {
	count?: number;
	before?: string;
}

/** The listing, oldest first, the way `tail` prints. Rejects on a failed command. */
export async function tailListing(page: TailPage = {}): Promise<{ rows: TailRow[]; columns: TailColumn[] }> {
	const argv = ['tail', '--json'];
	if (page.count !== undefined) argv.push('-n', String(page.count));
	if (page.before !== undefined) argv.push('-t', page.before);
	const result = await run(argv);
	if (result.code !== 0) throw new Error(result.stderr || `tail exited ${result.code}`);
	const doc = JSON.parse(result.stdout) as { rows?: TailRow[]; columns?: TailColumn[] };
	return { rows: doc.rows ?? [], columns: doc.columns ?? [] };
}
