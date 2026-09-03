// The widget bar's logic, kept pure and away from the markup: which fields
// get a widget, what shape each widget has, and the exact argv a press
// composes (da-nz29).
//
// The argv built here is the argv that runs — WidgetBar.svelte hands it
// straight to `run()` and echoes the result's own `argv` back into the
// REPL. Nothing reconstructs a "what you could have typed" string from the
// press, because a reconstruction can drift from the command and the whole
// point of the echo is that it teaches the truth.

import type { FieldSpec } from './types.js';

/**
 * What a press on a widget means, derived from the field's declared shape.
 *
 *   point  — a thing that happened:  `occur <field>`
 *   state  — a state that begins:    `start <field>`
 *   states — a state among several:  `switch <field> <state>`
 */
export type WidgetShape =
	| { kind: 'point' }
	| { kind: 'state' }
	| { kind: 'states'; states: string[] };

export interface Widget {
	field: string;
	shape: WidgetShape;
	/** The spec's colour, when it declares one. */
	color?: string;
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
 * The spec is the declaration and wins outright. Where no spec exists, the
 * log is the only other evidence: a field logged with string states is a
 * multi-state field, one logged with `true`/`false` is a state field, and
 * anything else is a point. The field's *name* is never consulted — a
 * field called "sleep" is whatever the user has actually recorded, not
 * whatever the word suggests.
 */
export function shapeFor(spec: FieldSpec | undefined, observed: unknown[] = []): WidgetShape {
	const strings = [...new Set(observed.filter((v): v is string => typeof v === 'string'))].sort();

	switch (spec?.default_verb) {
		case 'switch':
			// A switch field with no allowed set is unconstrained, so the states
			// it has been logged with are the honest menu.
			return { kind: 'states', states: spec.allowed_states?.length ? spec.allowed_states : strings };
		case 'start':
			return { kind: 'state' };
		case undefined:
			break;
		default:
			return { kind: 'point' };
	}

	if (strings.length > 0) return { kind: 'states', states: strings };
	if (observed.some((v) => typeof v === 'boolean')) return { kind: 'state' };
	return { kind: 'point' };
}

/**
 * The bar's widgets: one per field the user has actually touched.
 *
 * Fields come from the log and from declared specs — the union, so a field
 * declared through (+) has a widget before its first entry and a field
 * logged in the REPL has one without ever needing a spec. An empty
 * database yields an empty bar, which is the honest first-run state.
 */
export function widgetsFor(
	fields: Iterable<string>,
	specs: Map<string, FieldSpec>,
	observed: Map<string, unknown[]>
): Widget[] {
	const names = new Set<string>([...fields, ...specs.keys()]);
	return [...names].sort().map((field) => {
		const spec = specs.get(field);
		return {
			field,
			shape: shapeFor(spec, observed.get(field)),
			color: spec?.color ?? undefined
		};
	});
}

/**
 * Compose the argv one press runs.
 *
 * `state` is required for a multi-state widget and ignored otherwise. The
 * adjustments append in the order a person would type them: the time flag,
 * then the duration as a `dur=` token (CLAUDE.md — there is no `--dur`
 * flag, and the keyed form is unambiguous next to a state value in a way
 * the trailing positional isn't).
 */
export function pressArgv(widget: Widget, adjust: Adjust = {}, state?: string): string[] {
	const argv =
		widget.shape.kind === 'point'
			? ['occur', widget.field]
			: widget.shape.kind === 'state'
				? ['start', widget.field]
				: ['switch', widget.field, state ?? ''];

	// `-t -15` is fifteen minutes ago; the sign is what makes it an offset
	// rather than a clock time.
	if (adjust.minutesAgo) argv.push('-t', `-${adjust.minutesAgo}`);
	if (adjust.dur) argv.push(`dur=${adjust.dur}`);
	return argv;
}

/** The argv `spec set` needs to declare a new field of the given shape. */
export function declareArgv(field: string, shape: WidgetShape): string[] {
	const argv = ['spec', 'set', field, '--default-verb'];
	if (shape.kind === 'point') argv.push('occur');
	else if (shape.kind === 'state') argv.push('start');
	else {
		argv.push('switch');
		for (const state of shape.states) argv.push('--state', state);
	}
	return argv;
}
