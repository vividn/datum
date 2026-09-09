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

import { jsonModule } from './wasm.js';
import type { Entry } from './types.js';

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
}

/** Exactly one of the two is present. */
interface RenderResponse {
	lines?: string[];
	error?: string;
}

const call = jsonModule<RenderRequest, RenderResponse>('datum-render.wasm', 'render');

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
	const response = await call({
		entry,
		action,
		history,
		now_ms: Date.now(),
		timezone: Intl.DateTimeFormat().resolvedOptions().timeZone ?? null,
		offset_minutes: -new Date().getTimezoneOffset()
	});
	if (response.error !== undefined) throw new Error(response.error);
	return response.lines ?? [];
}
