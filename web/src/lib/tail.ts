// The rows `datum tail` lists — the part the CLI does against SQLite.
//
// ONE function, because the REPL's `tail` and the tail pane are two
// surfaces of one row set (da-6ec2): the REPL gets the CLI's text table
// and the pane gets the same rows laid out in a real `<table>`, and they
// must be the same rows in the same order or the two disagree on screen.
//
// Nothing here decides how a row LOOKS. The rows go to
// `crates/datum-render-wasm`, which renders them with the CLI's own
// `format_table_with` / `table_columns_with` — see $lib/renderer.ts. This
// file is only the query, the same way $lib/runners/views.ts is only the
// query behind the drawn views.

import { getEntriesForField, getRecentEntries } from './db.js';
import type { Entry } from './types.js';

/** `datum tail`'s default row count (`tail_state.rs`: `count.unwrap_or(10)`). */
const TAIL_COUNT = 10;

/** How much of a field's history the renderer is handed to compile state from. */
const HISTORY_LIMIT = 500;

/**
 * The rows to list, and the history each one renders against.
 *
 * Oldest at the top, newest at the bottom, the way `tail` prints.
 */
export async function tailListing(): Promise<{
	entries: Entry[];
	histories: Record<string, Entry[]>;
}> {
	const entries = await getRecentEntries(TAIL_COUNT);
	entries.reverse();
	// Each row renders against its field's history — one query per field
	// shown, the renderer excluding the row itself by hid. Bounded; a field
	// logged more than this many times still renders, from its most recent
	// stretch.
	const fields = [...new Set(entries.map((e) => e.field))];
	return {
		entries,
		histories: Object.fromEntries(
			await Promise.all(
				fields.map(async (field) => [field, await getEntriesForField(field, HISTORY_LIMIT)] as const)
			)
		)
	};
}
