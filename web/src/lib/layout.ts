// The layout module: datum's own dayview/nowview render, in the browser.
//
// `crates/datum-layout` computes the geometry of a dayview and paints it
// as SVG, and it depends on nothing — `compute_dayview` takes a config
// struct, not a database. So the two visual panes do not need the CLI
// compiled to wasm (da-1vat); they need the layout crate, which is a
// ~250 KB module with no imports. `crates/datum-layout-wasm` is that
// crate behind a five-function C ABI, and this file is the other half of
// it (da-6h6p). The ABI plumbing itself lives in `./wasm.ts`, shared with
// the parser module.
//
// Rows in, SVG out. Everything about *what* a day draws and *how* —
// which mark each entry makes (da-t7ud), the colours, the geometry, the
// axis, the clip to the visible window — stays in Rust, where the CLI's
// copy of the rule already lives. The browser supplies only what it alone
// knows: which rows its database returned, and what the viewer's clock
// says.

import { jsonModule } from './wasm.js';
import type { StoredVerb } from './types.js';

/**
 * One stored entry, in as much of it as a timeline mark reads.
 *
 * These are the `entries` row's own columns — `dur` is the stored string,
 * not a number, because reading a duration is `Dur::parse`'s job and not
 * the browser's. `occur_ms` is the one derived value, because the browser
 * already holds the time and a millisecond is not a rule.
 */
export interface EntryReq {
	occur_ms: number | null;
	verb: StoredVerb;
	dur?: string;
	state?: unknown;
	following_state?: unknown;
	slug: string;
}

/** One field and the entries it draws from. */
export interface FieldReq {
	field: string;
	entries: EntryReq[];
}

/**
 * The drawn part of a field spec — which is the stored spec itself.
 * `field_specs.spec_json` is a flat blob (`PresentationSpec` is
 * `serde(flatten)`), and the Rust side reads `color`, `state_colors`, `y`,
 * `height` and `z_index` straight out of it, ignoring the rest. So a spec
 * goes across untouched rather than being reshaped here.
 */
export type FieldStyle = Record<string, unknown>;

export interface DayReq {
	label_lines?: string[];
	start_ms: number;
	end_ms: number;
	has_error?: boolean;
	fields: FieldReq[];
}

export type LayoutRequest =
	| {
			view: 'dayview';
			now_ms: number;
			days: DayReq[];
			styles?: Record<string, FieldStyle>;
			options?: Record<string, unknown>;
	  }
	| {
			view: 'nowview';
			now_ms: number;
			/** Where the query started — earlier than the visible window. */
			fetch_start_ms: number;
			fields: FieldReq[];
			styles?: Record<string, FieldStyle>;
			/** Per field, the row deciding its state at `fetch_start_ms`. */
			lead_entries?: Record<string, EntryReq>;
			/** The same at `now_ms`, for the panel beside the now-line. */
			panel_entries?: Record<string, EntryReq>;
			clock: string;
			options?: Record<string, unknown>;
	  };

const call = jsonModule<LayoutRequest, { svg?: string; error?: string }>(
	'datum-layout.wasm',
	'render'
);

/**
 * Render one request to an SVG document.
 *
 * Rejects with the module's own message when the request is malformed —
 * the Rust side answers bad JSON with `{"error": …}` rather than trapping,
 * so a shape the browser got wrong reads as a message, not a dead pane.
 */
export async function renderLayout(request: LayoutRequest): Promise<string> {
	const parsed = await call(request);
	if (parsed.error !== undefined) throw new Error(parsed.error);
	if (parsed.svg === undefined) throw new Error('layout returned neither an svg nor an error');
	return parsed.svg;
}
