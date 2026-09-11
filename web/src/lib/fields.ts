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
	/** A spec exists: the field is tracked, and the bar gives it a button. */
	tracked: boolean;
	/** The spec's colour, when it declares one. */
	color?: string;
	/** The spec's per-state colours. */
	stateColors: Record<string, string>;
	/**
	 * What the field is in now, by `datum now --json`: `true` for an open
	 * block, the state's name for a switch; absent when it is off.
	 */
	current?: true | string;
}

/** The `-t` and `dur` tweaks a long press can attach before committing. */
export interface Adjust {
	/** Minutes back from now. 0 / undefined leaves the time alone. */
	minutesAgo?: number;
	/** A duration argument as typed (`30m`), not yet normalized. */
	dur?: string;
}

/**
 * Decide a field's shape.
 *
 * The spec's verb wins where it says something only a declaration can:
 * `switch`, `start`, `add`. `occur` does not — `track --color` writes it
 * as a placeholder for every field it colours — so for `occur`, as for no
 * spec, the log is the evidence: a field logged with string states is a
 * multi-state field, one logged with `true`/`false` is a block, and
 * anything else is a point. The field's *name* is never consulted.
 */
export function shapeFor(spec: FieldSpec | undefined, observed: unknown[] = []): FieldShape {
	const strings = [...new Set(observed.filter((v): v is string => typeof v === 'string'))].sort();

	switch (spec?.default_verb) {
		case 'switch':
			// A switch field with no allowed set is unconstrained, so the states
			// it has been logged with are the honest menu.
			return { kind: 'states', states: spec.allowed_states?.length ? spec.allowed_states : strings };
		case 'start':
			return { kind: 'state' };
		case 'add':
			return { kind: 'record' };
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
}

/** One row of `datum now --json`'s `active`. */
export interface NowActive {
	field: string;
	state: unknown;
}

/**
 * The widget's buttons: every field `datum fields` lists, the tracked ones
 * (a spec exists) first, each group in the CLI's order. `active` is
 * `datum now --json`'s — a field it does not name is off.
 */
export function buttonsFor(listing: FieldListing[], active: NowActive[] = []): FieldButton[] {
	const now = new Map(active.map((a) => [a.field, a.state]));
	const all: FieldButton[] = listing.map(({ field, spec, states }) => {
		const state = now.get(field);
		return {
			field,
			shape: shapeFor(spec ?? undefined, states),
			tracked: spec != null,
			color: spec?.color ?? undefined,
			stateColors: spec?.state_colors ?? {},
			current: state === true || (typeof state === 'string' && state !== '') ? state : undefined
		};
	});
	return [...all.filter((w) => w.tracked), ...all.filter((w) => !w.tracked)];
}

/** The app's neutral: what a field without a colour is drawn in. */
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
 * How a switch in a state is labelled: `field` — the field's name in the
 * field's colour over the state's fill; `state` — the state's name in the
 * state fill's ink. vividn is to pick (da-c7u8); `field` is the default.
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
 * outline. `hex` turns a colour as the spec spells it into `#rrggbb`, for
 * the ink rule.
 */
export function lookOf(button: FieldButton, variant: SwitchLabel = 'field', hex: (c: string) => string = (c) => c): Look {
	const field = hex(button.color ?? NEUTRAL);
	const { current, shape } = button;
	const off = { fill: null, line: field, text: field, label: button.field };
	if (shape.kind === 'point') return { fill: field, line: field, text: inkOver(field), label: button.field };
	if (shape.kind === 'record' || current === undefined) return off;
	if (current === true || shape.kind === 'state') return { fill: field, line: field, text: inkOver(field), label: button.field };
	const fill = hex(button.stateColors[current] ?? button.color ?? NEUTRAL);
	return variant === 'state'
		? { fill, line: field, text: inkOver(fill), label: current }
		: { fill, line: field, text: field, label: button.field };
}

/**
 * Compose the argv one press runs.
 *
 * `state` is required for a multi-state button and ignored otherwise. A
 * block that is on ends; one that is off starts. The adjustments append in
 * the order a person would type them: the time flag, then the duration as
 * a `dur=` token (CLAUDE.md — there is no `--dur` flag, and the keyed form
 * is unambiguous next to a state value in a way the trailing positional
 * isn't).
 */
export function pressArgv(button: FieldButton, adjust: Adjust = {}, state?: string): string[] {
	const { kind } = button.shape;
	const argv =
		kind === 'point'
			? ['occur', button.field]
			: kind === 'state'
				? [button.current ? 'end' : 'start', button.field]
				: kind === 'record'
					? ['add', button.field]
					: ['switch', button.field, state ?? ''];

	// `-t -15` is fifteen minutes ago; the sign is what makes it an offset
	// rather than a clock time.
	if (adjust.minutesAgo) argv.push('-t', `-${adjust.minutesAgo}`);
	if (adjust.dur) argv.push(`dur=${adjust.dur}`);
	return argv;
}

/** The argv `spec set` needs to declare a new field of the given shape. */
export function declareArgv(field: string, shape: FieldShape): string[] {
	const argv = ['spec', 'set', field, '--default-verb'];
	if (shape.kind === 'point') argv.push('occur');
	else if (shape.kind === 'state') argv.push('start');
	else if (shape.kind === 'record') argv.push('add');
	else {
		argv.push('switch');
		for (const state of shape.states) argv.push('--state', state);
	}
	return argv;
}
