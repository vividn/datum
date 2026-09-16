// The commands widget's logic, kept out of the markup (da-ltux): which
// commands get a button and in what order, what a press on each one
// means, which verb the command line is PENDING, and what the fields
// widget shows while one is. Pure but for `watchRows` at the end, which
// is the scroll rule reading a box.
//
// vividn, 2026-09-15: «Commands widget, which lists all commands. First one
// is Undo. Then let's have occur, start, end, switch, add, then other
// commands… After clicking on occur (or writing occur in the input box —
// also occur should appear in the input box after tapping occur), filter
// the fields to only occur fields.»
//
// One source for the verb's shape: a field's `shape` is the spec's verb
// (da-c8mh, `shapeFor` in fields.ts), so every rule below reads that and
// never the field's name. «Currently on/off» is `now --json`'s truth,
// which arrives on the button as `current` — never anything remembered
// here.

import type { FieldButton } from './fields.js';
import type { Verb as TrackVerb } from './track.js';

/** A verb the command line can be pending: the five a field is logged with. */
export type Verb = 'occur' | 'start' | 'end' | 'switch' | 'add';

export const VERBS: Verb[] = ['occur', 'start', 'end', 'switch', 'add'];

/** What a press on a command button does. */
export type Press =
	/** Run it, now, with no argument: `undo`, `redo`. */
	| 'run'
	/** Put `<name> ` on the command line and pend the verb. */
	| 'verb'
	/** Put `<name> ` on the command line; there is nothing to pend. */
	| 'fill';

export interface CommandButton {
	/** The CLI word — what runs, and what goes on the line. */
	name: string;
	/** What the button says. `Undo` and `Redo` are named, not typed. */
	label: string;
	press: Press;
}

/**
 * The commands, in the order the widget draws them: Undo (and Redo beside
 * it, da-s6no), vividn's five verbs, then the rest by how often a hand
 * reaches for them.
 *
 * `note` is NOT here (vividn, 2026-09-15 19:08): it lives in a field's
 * long-press menu (da-py5w), where the field it annotates already is.
 */
export const COMMANDS: CommandButton[] = [
	{ name: 'undo', label: 'Undo', press: 'run' },
	{ name: 'redo', label: 'Redo', press: 'run' },
	...VERBS.map((name): CommandButton => ({ name, label: name, press: 'verb' })),
	...[
		'retime',
		'update',
		'delete',
		'track',
		'ignore',
		'flag',
		'tail',
		'now',
		'stats',
		'fields',
		'count',
		'ids',
		'history',
		'get',
		'grep',
		'dayview',
		'nowview',
		'spec',
		'alias',
		'kit',
		'check',
		'db',
		'info',
		'config',
		'sync'
	].map((name): CommandButton => ({ name, label: name, press: 'fill' }))
];

/**
 * The verb the command line is pending, from what is typed in it.
 *
 * The FIRST WORD, whole: a tap on `occur` puts `occur ` in the box and a
 * person typing the same word reaches the same state, because both are
 * read back off the one buffer ($lib/bar.ts's `text`) rather than from a
 * second store a tap would have to remember to set.
 */
export function pendingVerb(line: string): Verb | null {
	const [word] = line.trimStart().split(/\s/, 1);
	return (VERBS as string[]).includes(word) ? (word as Verb) : null;
}

/** `true` among a switch's states: the state that makes it startable/endable. */
const hasTrue = (button: FieldButton): boolean =>
	button.shape.kind === 'states' && button.shape.states.includes('true');

/** Is the field on now — an open block, or a switch in a state? */
const on = (button: FieldButton): boolean => button.current !== undefined;

/**
 * The fields a pending verb leaves showing (vividn's rules above): the ones
 * that verb can actually be spent on. No verb — or a verb that is not one
 * of the five — leaves the list whole.
 */
export function fieldsUnder(buttons: FieldButton[], verb: Verb | null): FieldButton[] {
	switch (verb) {
		case 'occur':
			return buttons.filter((b) => b.shape.kind === 'point');
		case 'add':
			return buttons.filter((b) => b.shape.kind === 'record');
		case 'switch':
			return buttons.filter((b) => b.shape.kind === 'states');
		case 'start':
			return buttons.filter((b) => (b.shape.kind === 'state' && !on(b)) || hasTrue(b));
		case 'end':
			return buttons.filter((b) => (b.shape.kind === 'state' || b.shape.kind === 'states') && on(b));
		default:
			return buttons;
	}
}

/**
 * What a tap on `button` runs while `verb` is pending — `<verb> <field>`,
 * at once (C1).
 *
 * `null` where that argv is not a command: `switch <field>` has no state
 * to switch to, so the field's own state list asks which first (da-g635)
 * and runs `switch <field> <state>`; `add <field>` has no record in it, so
 * the field does what it always does and leaves the line for a person to
 * finish typing. Both still spend the pending verb — it is the verb they
 * end in.
 */
export function verbPress(verb: Verb, button: FieldButton): string[] | null {
	if (verb === 'switch' && button.shape.kind === 'states') return null;
	if (verb === 'add' && button.shape.kind === 'record') return null;
	return [verb, button.field];
}

/**
 * The verb a pending one DECLARES a field with, for the `+` at the end of
 * a filtered list (da-94q1's modal takes one of four).
 *
 * Identity but for `end`, which is not a verb a field can be born with —
 * the four are `occur | start | switch | add` (datum-core's
 * `VERB_CHOICES`). A block you are ending is a block, so `end` declares
 * `start`.
 */
export const declaredVerb = (verb: Verb): TrackVerb => (verb === 'end' ? 'start' : verb);

// ---- the scroll rule (both widgets) ------------------------------------
//
// vividn: «When the bar is one or 2 rows high, scroll left and right; when
// it is 3 or more high, then scroll up and down. Same with the fields
// widget.» So the rows are counted from the BOX the pane's layout gave
// the bar and the height of one button — never from what the bar is
// drawing, which is the answer and would make the question circular.

/** The gap between two buttons, and so between two rows (0.4rem). */
export const ROW_GAP = 6.4;

/** Rows of `rowHeight` buttons that fit in a box `height` tall, at least one. */
export function rowsIn(height: number, rowHeight: number): number {
	if (!(rowHeight > 0)) return 1;
	return Math.max(1, Math.floor((height + ROW_GAP) / (rowHeight + ROW_GAP)));
}

/** At one or two rows the bar is a single row that scrolls sideways. */
export const scrollsSideways = (rows: number): boolean => rows <= 2;

/**
 * Tell `onRows` how many rows fit, now and whenever the box changes.
 *
 * `bar` is the box the LAYOUT gave the widget — a flex child with no
 * height of its own — and `probe` one of the widget's own buttons, drawn
 * and hidden, so the row height is the height the buttons really have
 * rather than a number written down twice. The bar's own height is never
 * asked: it is the answer, and asking it would make the question circular.
 */
export function watchRows(
	bar: HTMLElement,
	probe: HTMLElement,
	onRows: (rows: number) => void
): () => void {
	const measure = () => onRows(rowsIn(bar.clientHeight, probe.offsetHeight));
	const observer = new ResizeObserver(measure);
	observer.observe(bar);
	observer.observe(probe);
	measure();
	return () => observer.disconnect();
}
