// The renderer: datum's own action lines, in the browser.
//
// `runners/local.ts` used to carry a plain-text stand-in for the CLI's
// `render_entry_line` — time, field, state, without the badges, the
// relative-day prefix or the duration the real renderer brings, and with
// its own copy of every formatting rule. `crates/datum-render-wasm` is
// `datum-commands::render` behind the same C ABI the parser and the layout
// module use, and this file is the other half of it: one rule, in Rust,
// for the terminal and the browser alike (da-wexl's ancestor, #148).
//
// The browser supplies only what it alone knows: the clock and the
// viewer's zone. Everything about *how* a row is shown stays in Rust.

import { getFieldSpecs } from './db.js';
import { jsonModule } from './wasm.js';
import type { Entry, FieldSpec } from './types.js';

/** The CLI's outcomes, named the way `datum-commands` names them. */
export type RenderAction = 'create' | 'update' | 'delete' | 'exists' | 'nodiff' | 'failed';

interface RenderRequest {
	/** The entry exactly as the parser module produced it. */
	entry: Entry;
	/** Which outcome heads the output; `null` renders the info line alone, the way `tail` shows a row. */
	action: RenderAction | null;
	now_ms: number;
	timezone: string | null;
	/** The viewer's UTC offset in minutes, east-positive. */
	offset_minutes: number;
	/**
	 * The field's other live entries, any order. The renderer compiles the
	 * field's state from them the way the CLI does from its database: the `∅`
	 * slot, a transition's LEAD and TRAIL, the dot's ambient state. The row
	 * itself may be among them; it is excluded by hid.
	 */
	history: Entry[];
	/**
	 * Paint the lines.
	 *
	 * The CLI decides this from `DATUM_COLOR` / `NO_COLOR` / whether stdout
	 * is a terminal; the module has none of those and would answer "no" to
	 * all three however the page feels about it, so the page says. datum's
	 * truecolor escapes land in the DOM through `ansiSpans` ($lib/ansi.ts).
	 */
	color: boolean;
	/** The field specs the browser holds, so a declared colour is honoured. */
	specs: FieldSpec[];
}

/** Exactly one of the two is present. */
interface RenderResponse {
	lines?: string[];
	error?: string;
}

/** The listing the two tail surfaces share. */
interface TailRequest {
	/** The rows to list, in the order they are shown — oldest first. */
	entries: Entry[];
	/** Each listed field's live entries, keyed by field name. */
	histories: Record<string, Entry[]>;
	now_ms: number;
	timezone: string | null;
	offset_minutes: number;
	color: boolean;
	specs: FieldSpec[];
}

/** One column of the CLI's table: its header word and one cell per row. */
export interface TailColumn {
	/** `time` | `field` | `state` | `dur` | `_n` | `hid`. */
	key: string;
	/** The header word the CLI prints, painted. */
	header: string;
	/** One painted, unpadded cell per row, in the listing's order. */
	cells: string[];
}

/**
 * One row of `datum tail --json`, plus the colours the CLI would paint it
 * with. Only the keys the app reads are named; the rest of the CLI's row
 * contract rides along untouched.
 */
export interface TailRow {
	hid: string;
	field: string;
	state_label: string | null;
	quick_id: string | null;
	/** `#rrggbb` — the field badge's colour. */
	field_color: string;
	/** `#rrggbb`, or null where the state paints no chip. */
	state_color: string | null;
	[key: string]: unknown;
}

interface TailResponse {
	text?: string;
	error?: string;
}

interface TailRowsResponse {
	rows?: TailRow[];
	columns?: TailColumn[];
	error?: string;
}

const call = jsonModule<RenderRequest, RenderResponse>('datum-render.wasm', 'render');
const callTail = jsonModule<TailRequest, TailResponse>('datum-render.wasm', 'tail');
const callTailRows = jsonModule<TailRequest, TailRowsResponse>('datum-render.wasm', 'tail_rows');

/** What the module needs from the browser and from nowhere else. */
async function host(): Promise<{
	now_ms: number;
	timezone: string | null;
	offset_minutes: number;
	specs: FieldSpec[];
}> {
	return {
		now_ms: Date.now(),
		timezone: Intl.DateTimeFormat().resolvedOptions().timeZone ?? null,
		offset_minutes: -new Date().getTimezoneOffset(),
		specs: [...(await getFieldSpecs()).values()]
	};
}

/**
 * The lines the CLI would print for `entry` under `action`.
 *
 * Rejects when the module cannot be reached or refuses the entry; the
 * caller decides how much of that to show. An entry the module renders
 * comes back as it would on a terminal, one string per line, no trailing
 * newline.
 */
export async function renderLines(
	entry: Entry,
	action: RenderAction | null,
	{ history = [] }: { history?: Entry[] } = {}
): Promise<string[]> {
	const response = await call({ entry, action, history, color: true, ...(await host()) });
	if (response.error !== undefined) throw new Error(response.error);
	return response.lines ?? [];
}

/**
 * The table `datum tail` would print for these rows, right now.
 *
 * Header, column alignment, `_n` ordinals and colours are all the CLI's —
 * `format_table_with`, the same function the terminal prints from. Nothing
 * about the layout is decided here.
 */
export async function tailText(
	entries: Entry[],
	histories: Record<string, Entry[]>
): Promise<string> {
	const response = await callTail({ entries, histories, color: true, ...(await host()) });
	if (response.error !== undefined) throw new Error(response.error);
	return response.text ?? '';
}

/**
 * The same listing as data: the CLI's rows, and its columns already
 * painted.
 *
 * For the pane, which has real columns and so lays the cells out itself
 * rather than reading a terminal grid. The cells still come from datum —
 * a `<td>` renders one through `ansiSpans` exactly as the REPL renders a
 * line — so the pane is the CLI's table in HTML, not a second table.
 */
export async function tailTable(
	entries: Entry[],
	histories: Record<string, Entry[]>
): Promise<{ rows: TailRow[]; columns: TailColumn[] }> {
	const response = await callTailRows({ entries, histories, color: true, ...(await host()) });
	if (response.error !== undefined) throw new Error(response.error);
	return { rows: response.rows ?? [], columns: response.columns ?? [] };
}
