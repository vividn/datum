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

import type { Pill } from './bar.js';
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
	/**
	 * What the person does after a `verb` or `fill` press (da-k34p): `tap`
	 * — a field, a pill — so the line is focused WITHOUT a phone's
	 * keyboard; `type`, and the keyboard opens with it.
	 */
	next?: 'tap' | 'type';
}

/** The fillers a tap completes: an id or a time pill is what comes next. */
const TAPPED = ['retime', 'delete', 'get', 'history'];

/**
 * The commands, in the order the widget draws them: Undo (and Redo beside
 * it, da-s6no), vividn's five verbs, then the rest by how often a hand
 * reaches for them.
 *
 * `note` is NOT here (vividn, 2026-09-15 19:08): it lived in a field's hold
 * panel (da-py5w), where the field it annotates already was. da-e78x6
 * removed that panel, so `note` is now TYPED — `note <field> <words>` —
 * and it stays off this list for a reason of its own: a `fill` press
 * would put `note ` on the line beside a field pill, and the moment the
 * comment's words are typed after it the pill no longer fits the field
 * slot (`fieldSlotOpen`, $lib/selection.ts) and is refused, so Enter
 * would run `note too strong` and log a field called `too`. A button
 * that composes the wrong command is worse than no button.
 */
export const COMMANDS: CommandButton[] = [
	{ name: 'undo', label: 'Undo', press: 'run' },
	{ name: 'redo', label: 'Redo', press: 'run' },
	...VERBS.map((name): CommandButton => ({ name, label: name, press: 'verb', next: 'tap' })),
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
	].map(
		(name): CommandButton => ({ name, label: name, press: 'fill', next: TAPPED.includes(name) ? 'tap' : 'type' })
	)
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
 * Does the bar hold a DURATION — a length waiting to be spent (da-qu16)?
 *
 * The pill a RANGE carries, whichever view made it (`SPAN_ID` in
 * $lib/cursor.ts): `dur=25m` from the nowview, `start_time=09:00` from the
 * day strip (da-ku7y). The two spellings are one pill of one KIND, so this
 * needs no clause for the second — which is what asking the kind rather
 * than reading the text buys.
 */
export const hasDuration = (all: Pill[]): boolean => all.some((p) => p.kind === 'dur');

/**
 * The fields a loaded duration leaves showing: the BLOCKS — a `start`
 * field and a `switch`, the two shapes that draw a span (vividn, 09-17: «if
 * a duration is selected, filter down to start/switch based fields»).
 *
 * A point has no length and a record is typed, so a loaded `dur=` hides
 * them and dismissing it brings them back. It COMPOSES with a pending
 * verb: this filter and `fieldsUnder` both apply, exactly as the `-t`
 * pill and the verb do.
 */
export function fieldsWithDuration(buttons: FieldButton[], duration: boolean): FieldButton[] {
	if (!duration) return buttons;
	return buttons.filter((b) => b.shape.kind === 'state' || b.shape.kind === 'states');
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

// ---- the pills a command takes (da-r13j) --------------------------------
//
// vividn, 2026-09-16: «loaded pills and interactions with the app should
// filter all other widgets. So when a -t pill is loaded in, only commands
// that take -t will available to click … when a doc is clicked or an entry
// in tail, it's hid is loaded as a pill … and then only commands that take
// an hid should be active». Which command takes what is never written down
// here: it is read off `datum help --json`, the CLI's own command table, so
// a new flag or a new id argument reaches the widget without a change here.

/** One command of `datum help --json`. */
export interface GrammarEntry {
	name: string;
	aliases: string[];
	usage: string[];
	flags: { short: string | null; long: string }[];
}

/** What a command's grammar takes of the pills the bar can hold. */
export interface Takes {
	/**
	 * Its first argument names entries: `one` (`get <ID_OR_SLUG>`), or a
	 * comma-batched `list` (`delete [ID_OR_SLUG[,ID_OR_SLUG]...]`); `false`
	 * when it names none. Several ids only ever go as ONE comma-joined word:
	 * `update A B` takes B as a state (measured, vremya 09-17).
	 */
	id: false | 'one' | 'list';
	/** `-t` is among its flags or in a usage line. */
	time: boolean;
	/**
	 * WHICH POSITIONAL its FIELD argument is, or `false` when it names no
	 * field (da-8810n): `1` for `tail [FIELD]` and `occur <FIELD>`, `2` for
	 * `spec show <FIELD>`, where a subcommand comes first.
	 *
	 * The slot and not a flag, because a field pill's word has to land IN
	 * it: datum's line is field-first, so a field appended to an argv that
	 * already carries one is read as that field's STATE — silently, as data
	 * (`fieldSlotOpen` in $lib/selection.ts is the other half of this).
	 */
	field: false | 1 | 2;
}

/** Commands by name AND by alias (`end` is `stop`'s). */
export type Grammar = Map<string, Takes>;

/** An id right after the command's name, and a `[,` after it when it takes a list. */
const ID_ARG = /^datum \S+ [<[](?:ID_OR_SLUG|SLUG|HID)(\[,)?/;

/**
 * A FIELD as the command's first positional, with at most a SUBCOMMAND
 * before it: `datum tail [FIELD]`, `datum spec show <FIELD>`.
 *
 * The middle word must be a bare one, so `datum stats <FIELD[:STATE]>`
 * reads as the field at slot 1 rather than as a subcommand. A field behind
 * a FLAG — `datum grep <PATTERN>... [-f FIELD]`, `retime --field F` — is
 * not this: a bare word put in its place would be a second pattern, not a
 * field, so those commands take no field pill.
 */
const FIELD_ARG = /^datum \S+(?: ([a-z][\w-]*))? [<[]FIELD/;

export function takesOf(entry: GrammarEntry): Takes {
	const ids = entry.usage.map((u) => ID_ARG.exec(u.trim())).filter((m) => m !== null);
	const id = ids.some((m) => m[1]) ? 'list' : ids.length ? 'one' : false;
	const time = entry.flags.some((f) => f.short === 't') || entry.usage.some((u) => /[\s[]-t[\s\]]/.test(u));
	// The shallowest slot any usage line puts it in: `meta <FIELD> <TEXT>`
	// and `meta show <FIELD>` are one command, and 1 is the one a bare
	// `meta` leaves open.
	const slots = entry.usage
		.map((u) => FIELD_ARG.exec(u.trim()))
		.filter((m) => m !== null)
		.map((m): 1 | 2 => (m[1] === undefined ? 1 : 2));
	const field = slots.length ? (Math.min(...slots) as 1 | 2) : false;
	return { id, time, field };
}

export function grammarOf(entries: GrammarEntry[]): Grammar {
	return new Map(entries.flatMap((e) => [e.name, ...e.aliases].map((name) => [name, takesOf(e)] as const)));
}

/** The hids an `hid` pill holds, in the order they were selected. */
export const hidsOf = (pill: Pill): string[] => pill.text.split(',').filter(Boolean);

/**
 * Can a command whose grammar is `takes` spend this pill? A `time` pill
 * wants `-t`; an `hid` pill wants an id, and more than one hid wants a
 * command that takes several; a `field` pill wants a command that names a
 * field. A kind this does not know is let through.
 *
 * A command the grammar does not know — a field-first line, an alias, or no
 * grammar at all — takes every pill but an hid and a FIELD: `coffee <hid>`
 * would log the hid as coffee's STATE (vremya, review 1498), and `coffee`
 * typed under a `mood` pill would log `mood` as coffee's state the same
 * way — the same shape, so the same refusal (da-8810n). The time cursor it
 * takes, as it always has.
 */
export function fits(takes: Takes | undefined, pill: Pill): boolean {
	if (!takes) return pill.kind !== 'hid' && pill.kind !== 'field';
	if (pill.kind === 'time') return takes.time;
	if (pill.kind === 'hid') return hidsOf(pill).length > 1 ? takes.id === 'list' : !!takes.id;
	if (pill.kind === 'field') return takes.field !== false;
	return true;
}

/**
 * The commands the widget draws under `pills`: the ones whose grammar takes
 * every pill, in their usual order. The rest are HIDDEN (vividn, 09-17: «let's
 * do hidden»), as the fields widget hides what a pending verb cannot be
 * spent on. No pills is the whole list; with no grammar, an hid pill is
 * taken by nothing and hides every command until the grammar is read.
 */
export function commandsUnder(grammar: Grammar | null, pills: Pill[]): CommandButton[] {
	return COMMANDS.filter((command) => {
		const takes = grammar?.get(command.name);
		return pills.every((p) => fits(takes, p));
	});
}

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
