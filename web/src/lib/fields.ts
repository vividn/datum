// The fields widget's logic, kept pure and away from the markup: which
// fields get a button, what shape each has, how it is painted, and the
// exact argv a press composes (da-nz29, da-c7u8).
//
// The argv built here is the argv that runs — FieldsWidget.svelte hands it
// straight to `run()` and echoes the result's own `argv` back into the
// REPL. Nothing reconstructs a "what you could have typed" string from the
// press, because a reconstruction can drift from the command and the whole
// point of the echo is that it teaches the truth.

import type { FieldSpec } from './types.js';

/**
 * What a press on a button means, derived from the field's declared shape.
 *
 *   point  — a thing that happened:  `occur <field>`
 *   state  — a block, on or off:     `start <field>` / `end <field>`
 *   states — a state among several:  `switch <field> <state>`
 *   record — a keyed record:         `add <field> …`, typed, never pressed
 */
export type FieldShape =
	| { kind: 'point' }
	| { kind: 'state' }
	| { kind: 'states'; states: string[] }
	| { kind: 'record' };

export interface FieldButton {
	field: string;
	shape: FieldShape;
	/**
	 * `datum fields --json`'s `ignored` — the field's latest track/ignore
	 * status, defined once in datum-core (da-d08c). The bar draws a button
	 * for every field that is not; Track + offers the ignored ones back.
	 */
	ignored: boolean;
	/**
	 * The field's RESOLVED colour (`#rrggbb`) — `datum fields --json`'s
	 * `color`, computed the same way the CLI paints it: a spec override, else
	 * `md5(field)` (da-yiyj). Always present; there is no untracked/colourless
	 * case left for the widget to fall back on.
	 */
	color: string;
	/** Every listed state's RESOLVED colour (`#rrggbb`) — `fields --json`'s `state_colors`. */
	stateColors: Record<string, string>;
	/**
	 * What the field is in now, by `datum now --json`: `true` for an open
	 * block, the state's name for a switch; absent when it is off.
	 */
	current?: true | string;
	/**
	 * Whether the field is in an ACTIVE state right now — `datum now
	 * --json` named it, and the rows it names are exactly the truthy ones
	 * (`is_truthy` over `is_inactive_state`, datum-core/src/db/now.rs:
	 * null, `false`, IGNORED, TRACKED and an all-falsy array are inactive;
	 * an empty string is a state and so active). The rule is the CLI's and
	 * is not restated here.
	 *
	 * Not the same question as `current`, which is the field's state as a
	 * COLOUR and so is absent for a field whose active state paints
	 * nothing. A press ranks by this one (`promoted` in $lib/lanes.ts).
	 */
	active: boolean;
}

/**
 * Decide a field's shape.
 *
 * The spec's verb says it: `switch`, `start`, `add`, `occur` all say what a
 * press means regardless of how the field has been logged so far, and every
 * spec has one (da-c8mh). Only a field with NO spec is read off its log —
 * a field the next log is about to write a spec for: string states make a
 * multi-state field, `true`/`false` a block, anything else a point. The
 * field's *name* is never consulted.
 */
export function shapeFor(spec: FieldSpec | undefined, observed: unknown[] = []): FieldShape {
	const strings = [...new Set(observed.filter((v): v is string => typeof v === 'string'))].sort();

	switch (spec?.verb) {
		case 'switch':
			// A switch field with no allowed set is unconstrained, so the states
			// it has been logged with are the honest menu.
			return { kind: 'states', states: spec.allowed_states?.length ? spec.allowed_states : strings };
		case 'start':
			return { kind: 'state' };
		case 'add':
			return { kind: 'record' };
		case 'occur':
			return { kind: 'point' };
	}

	if (strings.length > 0) return { kind: 'states', states: strings };
	if (observed.some((v) => typeof v === 'boolean')) return { kind: 'state' };
	return { kind: 'point' };
}

/** One row of `datum fields --json`. */
export interface FieldListing {
	field: string;
	spec: FieldSpec | null;
	/** The distinct states the field has been logged with. */
	states: unknown[];
	/** The field's resolved colour (`#rrggbb`) — spec override, else `md5(field)`. */
	color: string;
	/** Every listed state's resolved colour (`#rrggbb`), keyed by state name. */
	state_colors: Record<string, string>;
	/** The latest track/ignore status is IGNORED (da-d08c). */
	ignored: boolean;
	/** Where the field's band sits in the dayview and nowview, 0 at the top (da-vu7pi). */
	y: number;
}

/** One row of `datum now --json`'s `active`. */
export interface NowActive {
	field: string;
	state: unknown;
}

/**
 * The widget's entries: every field `datum fields` lists, in the order the
 * pictures draw them (vividn, 2026-09-22: «sorted by their spec y, so those
 * that appear higher on the nowview are also higher in the list … Active
 * start/switch fields should appear on the far left») — the ACTIVE fields
 * first, then the rest, each by the `y` the CLI resolved (da-vu7pi), and
 * the CLI's alphabetical order among equals. `active` is `datum now
 * --json`'s — a field it does not name is off.
 */
export function buttonsFor(listing: FieldListing[], active: NowActive[] = []): FieldButton[] {
	const now = new Map(active.map((a) => [a.field, a.state]));
	const ordered = [...listing].sort(
		(a, b) => Number(now.has(b.field)) - Number(now.has(a.field)) || a.y - b.y
	);
	return ordered.map(({ field, spec, states, color, state_colors, ignored }) => {
		const state = now.get(field);
		return {
			field,
			shape: shapeFor(spec ?? undefined, states),
			ignored,
			color,
			stateColors: state_colors,
			active: now.has(field),
			current: state === true || (typeof state === 'string' && state !== '') ? state : undefined
		};
	});
}

/**
 * The app's neutral: what a field without a colour is drawn in.
 *
 * `--accent`'s value, written out rather than read: it is handed to
 * `inkOver` below, which parses the hex, and to SVG attributes, where
 * `var()` does not reach. Change one and change the other (app.css).
 */
export const NEUTRAL = '#7c7cff';

/**
 * The CLI's label ink over a fill — `contrast_fg` in
 * crates/datum-commands/src/render/colors.rs, datum's YIQ rule: black over
 * a fill brighter than 128, white otherwise. `hex` is `#rrggbb`.
 */
export function inkOver(hex: string): string {
	const n = parseInt(hex.slice(1, 7), 16);
	const yiq = Math.floor((((n >> 16) & 255) * 299 + ((n >> 8) & 255) * 587 + (n & 255) * 114) / 1000);
	return yiq > 128 ? '#000000' : '#ffffff';
}

/**
 * Which WORD a switch in a state is labelled with: `field` — the field's
 * name; `state` — the state's. vividn is to pick (da-c7u8); `field` is the
 * default. It chooses the word only: the ink is the fill's either way.
 */
export type SwitchLabel = 'field' | 'state';

export interface Look {
	/** The background, or null for none. */
	fill: string | null;
	/** The outline (the underline, for a record). */
	line: string;
	/** The text colour. */
	text: string;
	label: string;
}

/**
 * How a button is painted now (vividn's rules, da-c7u8): a point is always
 * filled with its field's colour; a block is filled while it is on and
 * outlined otherwise; a switch in a state is filled with that state's
 * colour (the field's, where the state has none) inside the field's
 * outline. `button.color` / `button.stateColors` are already resolved
 * `#rrggbb` — `datum fields --json` computes them the same way the CLI
 * paints them (da-yiyj), so there is nothing left here to parse or fall
 * back to a neutral for.
 *
 * A label's ink comes from the FILL it lands on, once, for every kind
 * (`filled` below) — never from the field's colour. A switch used to take
 * the field's colour as its text over a fill that falls back to that same
 * colour, so the default case — a switch tracked with no colours set at
 * all — printed the field's name in exactly its own background: contrast
 * 1.00, an invisible label (da-qnvh). Setting a field colour only changed
 * which colour was duplicated. Only an unfilled button keeps the field's
 * colour as its ink, and that is over the pane, not over itself.
 */
export function lookOf(button: FieldButton, variant: SwitchLabel = 'field'): Look {
	const field = button.color;
	const { current, shape } = button;
	const filled = (fill: string, label: string): Look => ({ fill, line: field, text: inkOver(fill), label });
	if (shape.kind === 'point') return filled(field, button.field);
	if (shape.kind === 'record' || current === undefined) return { fill: null, line: field, text: field, label: button.field };
	if (current === true || shape.kind === 'state') return filled(field, button.field);
	const fill = button.stateColors[current] ?? button.color;
	return filled(fill, variant === 'state' ? current : button.field);
}

/**
 * Compose the argv one press runs.
 *
 * `state` is required for a multi-state button and ignored otherwise. A
 * block that is on ends; one that is off starts.
 *
 * The COMMAND and nothing else. It used to take an `Adjust` as well — a
 * `-t` and a `dur=` the hold panel's bubbles set — and da-e78x6 removed
 * that panel, leaving the bar's own pills as the one way anything is
 * added to a press (`withPills` in $lib/selection.ts). A press composes
 * what it means; the line composes what it carries.
 */
export function pressArgv(button: FieldButton, state?: string): string[] {
	const { kind } = button.shape;
	return kind === 'point'
		? ['occur', button.field]
		: kind === 'state'
			? [button.current ? 'end' : 'start', button.field]
			: kind === 'record'
				? ['add', button.field]
				: ['switch', button.field, state ?? ''];
}

/**
 * How one state reads in the list a press fills the widget with (da-g635).
 *
 * The state the field is in NOW is marked exactly the way the field's own
 * chip marks it — filled with that state's colour inside the field's
 * outline, which is `lookOf`'s switch case and so cannot drift from it.
 * Every other state is outlined in its own resolved colour: the list says
 * what each state is painted as without claiming any of them is on.
 */
export function stateLook(button: FieldButton, state: string): Look {
	if (button.current === state) return lookOf(button, 'state');
	const color = button.stateColors[state] ?? button.color;
	return { fill: null, line: color, text: color, label: state };
}
