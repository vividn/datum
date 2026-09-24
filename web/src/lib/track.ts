// The web's «which verb?» — the four answers, as data.
//
// A field cannot be born verbless (da-c8mh): `track <field>` on a field
// with neither a spec nor a log refuses and names the four choices. The
// CLI asks with that sentence; the web asks with `SpecModal.svelte`, and
// this module is the one place that knows what the four are, what they
// look like, and what answering one runs — or, on a field that already
// has a spec, what saving the edited one runs (da-ci85o).
//
// The GLYPHS are the CLI's, not the web's: `track_action_label` in
// crates/datum-core/src/track.rs paints `TRACK●`/`█`/`▞`/`¢` beside the
// declaration on every surface that has an action column, so a pictogram
// here that drifted from it would teach the wrong shape for the row the
// press is about to write. `tests/track.test.mjs` runs all four through
// the real CLI and holds this table to what it renders.
//
// Pure, and away from the markup as `fields.ts` is: node loads this file
// as it is, which is what lets the glyph table be compared with the CLI's
// own render. The modal itself is `specModal.ts`.

import type { RunResult } from './run.js';

/** The verbs a spec may carry — `VERB_CHOICES` in datum-core's field_spec. */
export type Verb = 'occur' | 'start' | 'switch' | 'add';

/** One of the four, as the modal draws it. */
export interface VerbChoice {
	verb: Verb;
	/** The glyph the CLI prints after `TRACK` for a field declared this way. */
	glyph: string;
	/** One line of what the verb is, for the person choosing. */
	what: string;
}

/**
 * The four, in the CLI's order (`VERB_CHOICES`): `occur` first, the
 * smallest claim a field can make.
 */
export const VERBS: VerbChoice[] = [
	{ verb: 'occur', glyph: '●', what: 'a thing that happened' },
	{ verb: 'start', glyph: '█', what: 'a block that begins and ends' },
	{ verb: 'switch', glyph: '▞', what: 'one of several states' },
	{ verb: 'add', glyph: '¢', what: 'a record you type' }
];

/** The action word `tail` prints for a field declared with `verb`. */
export const trackLabel = (verb: Verb): string =>
	`TRACK${VERBS.find((v) => v.verb === verb)?.glyph ?? ''}`;

/** One row of the States tab: a state, its colour and what it means (empty for none). */
export interface StateRow {
	name: string;
	color: string;
	description: string;
}

/**
 * The argv that declares `field`.
 *
 * `track`, not `spec set`: declaring a field is an event in the log — the
 * TRACK document that says what the field IS (da-c8mh) — and only `track`
 * writes one. It also un-ignores, which is what the ignored list below it
 * is for. A bare `--state` names a state the field may be switched to
 * (da-94q1) and `--state NAME=COLOR` paints it, so a switch field is
 * declared whole in one command. A state's description is not something
 * `track` carries, and so the modal does not ask for one there.
 */
export function trackArgv(field: string, verb: Verb, states: StateRow[] = [], color = ''): string[] {
	const argv = ['track', field, '--verb', verb];
	if (verb === 'switch') {
		const named = states.filter((s) => s.name.trim());
		for (const { name } of named) argv.push('--state', name.trim());
		for (const { name, color } of named) if (color) argv.push('--state', `${name.trim()}=${color}`);
	}
	if (color) argv.push('--color', color);
	return argv;
}

/** A spec as `spec show` prints it; only the keys the modal edits are named. */
export interface Spec {
	field: string;
	verb: Verb;
	color: string | null;
	unit: string | null;
	description: string | null;
	allowed_states: string[] | null;
	state_colors: Record<string, string>;
	/** Left out of `spec show` when empty, as the CLI serialises it. */
	state_descriptions?: Record<string, string>;
	[key: string]: unknown;
}

/** The States tab's rows for `spec`: its allowed states, each with its colour and description. */
export const stateRows = (spec: Spec): StateRow[] =>
	(spec.allowed_states ?? []).map((name) => ({
		name,
		color: spec.state_colors[name] ?? '',
		description: spec.state_descriptions?.[name] ?? ''
	}));

/**
 * `spec` with the modal's edits laid over it — everything it does not
 * edit (subfields, placement, tags, keys) carried as it was.
 *
 * The states are the WHOLE list, which is `spec set`'s shape and not
 * `add-state`'s: a state removed takes its colour and description with
 * it, and an empty colour or description clears the entry.
 */
export function editedSpec(
	spec: Spec,
	edit: { verb: Verb; color: string; unit: string; description: string; states: StateRow[] }
): Spec {
	const rows = edit.states.map((s) => ({ ...s, name: s.name.trim() })).filter((s) => s.name);
	const names = [...new Set(rows.map((s) => s.name))];
	const state_colors = { ...spec.state_colors };
	const state_descriptions = { ...spec.state_descriptions };
	for (const gone of spec.allowed_states ?? []) {
		delete state_colors[gone];
		delete state_descriptions[gone];
	}
	for (const { name, color, description } of rows) {
		if (color) state_colors[name] = color;
		if (description.trim()) state_descriptions[name] = description.trim();
	}
	const { state_descriptions: _, ...rest } = spec;
	return {
		...rest,
		verb: edit.verb,
		color: edit.color || null,
		unit: edit.unit.trim() || null,
		description: edit.description.trim() || null,
		allowed_states: spec.allowed_states === null && names.length === 0 ? null : names,
		state_colors,
		// Absent when empty, as `spec show` prints it, so what is sent is what is read back.
		...(Object.keys(state_descriptions).length ? { state_descriptions } : {})
	};
}

/**
 * The ONE command that saves an edited spec: `spec set --from-json` with
 * the whole of it, so it lands entire or not at all — never a stream of
 * `add-state`s half applied — and so a colour, a unit or a state can be
 * cleared, which the one-value flags cannot say.
 */
export const specArgv = (spec: Spec): string[] => [
	'spec',
	'set',
	spec.field,
	'--from-json',
	JSON.stringify(spec)
];

/**
 * The field a «which verb?» refusal is about, or null when the result is
 * not one.
 *
 * The sentence is `VERB_CHOICES_TEXT`'s companion in
 * crates/datum-commands/src/commands/track.rs — the ONE refusal, so
 * matching it here is matching every surface that can raise it.
 */
export function verbAsked(argv: string[], result: RunResult): string | null {
	if (result.code === 0 || argv[0] !== 'track') return null;
	const asked = /'([^']+)' is new — say what a press of it means/.exec(result.stderr);
	return asked ? asked[1] : null;
}
