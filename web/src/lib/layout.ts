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
// Data in, SVG out. Everything about *how* a day is drawn — colours,
// geometry, the axis, the clip to the visible window — stays in Rust,
// where the CLI's copy of the rule already lives. The browser supplies
// only what it alone knows: which entries are in the window, and what the
// viewer's clock says.

import { jsonModule } from './wasm.js';

/** A block: a filled span. `state` picks the fill; absent → field colour. */
export interface BlockReq {
	start_ms: number;
	end_ms: number;
	state?: unknown;
}

/** A point: a dot at an instant, with a foreground and a background. */
export interface PointReq {
	time_ms: number;
	fg?: unknown;
	fg_fallback?: 'field' | 'black';
	bg?: unknown;
	bg_fallback?: 'field' | 'black';
	state_label?: unknown;
}

export interface FieldReq {
	field: string;
	blocks?: BlockReq[];
	points?: PointReq[];
}

/** The drawn part of a field spec, flattened the way Rust reads it. */
export interface FieldStyle {
	color?: string | null;
	state_colors?: Record<string, string>;
	y?: number;
	height?: number;
	z_index?: number;
}

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
			fields: FieldReq[];
			styles?: Record<string, FieldStyle>;
			panel_states?: Record<string, unknown>;
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
