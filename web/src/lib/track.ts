// The web's «which verb?» — the four answers, as data.
//
// A field cannot be born verbless (da-c8mh): `track <field>` on a field
// with neither a spec nor a log refuses and names the four choices. The
// CLI asks with that sentence; the web asks with `TrackModal.svelte`, and
// this module is the one place that knows what the four are, what they
// look like, and what answering one runs.
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
// own render. The modal itself is `trackModal.ts`.

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

/**
 * The argv that declares `field`.
 *
 * `track`, not `spec set`: declaring a field is an event in the log — the
 * TRACK document that says what the field IS (da-c8mh) — and only `track`
 * writes one. It also un-ignores, which is what the ignored list below it
 * is for. A bare `--state` names a state the field may be switched to
 * (da-94q1), so a switch field is declared whole in one command.
 */
export function trackArgv(field: string, verb: Verb, states: string[] = [], color = ''): string[] {
	const argv = ['track', field, '--verb', verb];
	if (verb === 'switch') for (const state of states) argv.push('--state', state);
	if (color) argv.push('--color', color);
	return argv;
}

/** Split the states box the way the CLI would want them, one flag each. */
export const splitStates = (typed: string): string[] =>
	typed
		.split(',')
		.map((s) => s.trim())
		.filter(Boolean);

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
