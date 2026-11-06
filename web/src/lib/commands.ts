// The commands widget's logic, kept out of the markup (da-ltux): which
// commands get a button and in what order, what a press on each one
// means, which command the command line is PENDING, and what the fields
// widget shows while one is. Pure but for `watchRows` at the end, which
// is the scroll rule reading a box.
//
// vividn, 2026-09-15: «Commands widget, which lists all commands. First one
// is Undo. Then let's have occur, start, end, switch, add, then other
// commands… After clicking on occur (or writing occur in the input box —
// also occur should appear in the input box after tapping occur), filter
// the fields to only occur fields.»
//
// One source for what a command fits: a field's `shape` is its kind, read
// off its spec (da-c8mh, `kindFor` in fields.ts), so every rule below reads that and
// never the field's name. «Currently on/off» is `now --json`'s truth,
// which arrives on the button as `current` — never anything remembered
// here.

import type { Pill } from './bar.js';
import type { FieldButton } from './fields.js';

/** A command the line can be PENDING: the six a field is logged with, by the glossary's words (AGENTS.md); `allday` since da-0cf4z. */
export type LogCommand = 'moment' | 'on' | 'off' | 'change' | 'record' | 'allday';

export const LOG_COMMANDS: LogCommand[] = ['moment', 'on', 'off', 'change', 'record', 'allday'];

/** Each one's old words, which the CLI still runs as aliases: typed, they pend the same command. */
const ALIASES: Record<string, LogCommand> = { occur: 'moment', start: 'on', end: 'off', stop: 'off', switch: 'change', add: 'record', card: 'record' };

/** What a press on a command button does. */
export type Press =
	/** Run it, now, with no argument: `undo` (and its redo icon). */
	| 'run'
	/** Put `<name> ` on the command line and pend the command. */
	| 'pend'
	/** Put `<name> ` on the command line; there is nothing to pend. */
	| 'fill'
	/** Arm the page: the next tap is explained, not acted on (#lib/help.ts). */
	| 'help'
	/** Leave a submenu, or a command's subcommands or flags, for the bar before (da-803mk, da-fg4w4). */
	| 'back'
	/** Append the flag to the line (da-st2xl). */
	| 'flag'
	/** Open the «Backups» place (#lib/backups.ts, da-9ow8m, da-ea5q2): `db backup`, and `db restore` at its list. */
	| 'backups'
	/** Open the command's PLACE, a modal on a desktop and a tab on a phone (`openPlace`, da-6du3o, da-jozgv): `sync`, `conflicts`. */
	| 'place'
	/** Show one of the base row's submenus in its place (`MENUS`, da-fg4w4). */
	| 'menu';

export interface CommandButton {
	/** The CLI word — what runs, and what goes on the line; a submenu's name for a `menu`. */
	name: string;
	/** The name drawn beneath its circle. */
	label: string;
	press: Press;
	/**
	 * What the person does after a `pend` or `fill` press (da-k34p): `tap`
	 * — a field, a pill — so the line is focused WITHOUT a phone's
	 * keyboard; `type`, and the keyboard opens with it.
	 */
	next?: 'tap' | 'type';
	/** A flag's one line of help, from the CLI's own flag table. */
	help?: string;
}

/** The fillers a tap completes: an id or a time pill is what comes next. */
const TAPPED = ['retime', 'delete', 'revert', 'get', 'history'];

/** Commands that fill the line: `name `, then a tap or the keyboard. */
const fills = (names: string[]): CommandButton[] =>
	names.map((name) => ({ name, label: name, press: 'fill', next: TAPPED.includes(name) ? 'tap' : 'type' }));

const UNDO: CommandButton = { name: 'undo', label: 'undo', press: 'run' };
/**
 * It explains the next tap instead of running anything (da-vjwid), and
 * `datum help` is what it is a button for. Every drilled bar carries it, so
 * a subcommand or a flag can be explained.
 */
export const HELP: CommandButton = { name: 'help', label: 'help', press: 'help' };
/** One button for sync, which opens its place (da-6du3o): its subcommands are in that widget's Advanced section. */
const SYNC: CommandButton = { name: 'sync', label: 'sync', press: 'place' };
/** Its place too (da-6cutb): the widget each conflict is resolved in. */
const CONFLICTS: CommandButton = { name: 'conflicts', label: 'conflicts', press: 'place' };

/**
 * The submenus (da-fg4w4), vividn, 2026-11-03: «Write and read expand into a
 * submenu when clicked of the various commands inside of them», and a
 * `fields` one of what acts on a field's definition: «maybe we have a fields
 * submenu? That then has setup, ignore, track, alias, rename inside of it?».
 * Left out: `nowview` and `dayview` («don't need to have separate buttons
 * here»), `ids` («kind of useless as a top level command»), `view`.
 * `conflicts` is under `sync` («should not be top level»), which opens that
 * submenu only while there are any, and its place at once otherwise.
 *
 * `remark` is typed after its field — `remark <field> <words>` — and a tap on
 * it under a field pill writes the pill into the line first (da-8ru0m).
 */
export const MENUS = {
	write: [
		...LOG_COMMANDS.map((name): CommandButton => ({ name, label: name, press: 'pend', next: 'tap' })),
		...fills(['remark', 'flag', 'update', 'retime', 'delete', 'revert'])
	],
	read: fills(['tail', 'now', 'stats', 'get', 'history', 'grep', 'fields', 'count', 'info', 'check']),
	fields: fills(['setup', 'track', 'ignore', 'alias', 'rename']),
	settings: [...fills(['config']), HELP],
	sync: [SYNC, CONFLICTS]
};

export type Menu = keyof typeof MENUS;

const menu = (name: Menu): CommandButton => ({ name, label: name, press: 'menu' });

/**
 * The base row (da-fg4w4), vividn: «undo write read setup kit db sync
 * settings», `setup` became the `fields` submenu. Eight, so a phone's width
 * holds them without a scroll.
 */
const KIT_DB = fills(['kit', 'db']);

export const BASE: CommandButton[] = [UNDO, menu('write'), menu('read'), menu('fields'), ...KIT_DB, SYNC, menu('settings')];

/**
 * Every command, flat, in the order a loaded pill's filter draws them
 * (`commandsUnder`): with a pill there are no submenus, only the commands
 * that take it. Built from the submenus, so a command added to one is here
 * too; a `Set`, as Help leads here and is settings' too.
 *
 * `redo` is not a button of its own (da-zq91a): the widget draws it as a
 * small icon joined to Undo, lit only when the CLI says there is an undo to
 * redo (`redo --dry-run`, CommandPane.svelte).
 */
export const COMMANDS: CommandButton[] = [
	...new Set([UNDO, HELP, ...MENUS.write, ...MENUS.fields, ...MENUS.read, ...KIT_DB, ...MENUS.sync, ...MENUS.settings])
];

/**
 * The mark in a command's circle (da-y6jfb, da-fg4w4), vividn, 2026-11-03:
 * «make the command buttons circular with an icon in the circle and the
 * name of the command beneath them».
 *
 * One character is a GLYPH, drawn as text: a log command's is the CLI's own, so
 * the button shows the shape the row it writes will show (`TRACK●`/`█`/
 * `▞`/`¢`/`▤` in datum-core's track.rs; `■` is the end of a `█`). No two share
 * a mark; a subcommand wears its command's. Anything longer is the path of
 * a 16×16 line DRAWING, for the shapes no font every device carries.
 */
export const ICONS: Record<string, string> = {
	undo: '↶',
	moment: '●',
	on: '█',
	off: '■',
	change: '▞',
	record: '¢',
	allday: '▤',
	remark: '✎',
	flag: '⚑',
	update: 'Δ',
	retime: 'M8 1.5a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM8 4.5V8l2.5 1.5',
	delete: 'M2.5 4.5h11M6 4.5v-2h4v2M4 4.5l.7 9h6.6l.7-9M6.7 7v4M9.3 7v4',
	revert: '↺',
	tail: 'M2.5 4h.01M5.5 4h8M2.5 8h.01M5.5 8h8M2.5 12h.01M5.5 12h8',
	now: '◉',
	stats: 'M2 13.5v-4h3v4zM6.5 13.5v-11h3v11zM11 13.5v-7h3v7z',
	get: '#',
	history: 'M2.5 8a5.5 5.5 0 1 0 1.6-3.9M2.5 2.5V5H5M8 5v3l2 1.5',
	grep: 'M7 2.5a4.5 4.5 0 1 0 0 9a4.5 4.5 0 1 0 0-9zM10.3 10.3l3.2 3.2',
	fields: '≡',
	count: 'Σ',
	info: 'i',
	check: '✓',
	setup: '⊞',
	track: '+',
	ignore: '⊘',
	alias: '≈',
	rename: '⇄',
	kit: 'M2.5 5L8 2.5L13.5 5v6.5L8 14l-5.5-2.5zM2.5 5L8 7.5L13.5 5M8 7.5V14',
	db: 'M3 4c0-1 2.2-1.7 5-1.7s5 .7 5 1.7v8c0 1-2.2 1.7-5 1.7s-5-.7-5-1.7zM3 4c0 1 2.2 1.7 5 1.7s5-.7 5-1.7M3 8c0 1 2.2 1.7 5 1.7s5-.7 5-1.7',
	sync: 'M2.5 5.5h10l-3-3M13.5 10.5h-10l3 3',
	conflicts: '!',
	config: '⚙',
	help: '?',
	back: '‹'
};

/** A submenu's mark, apart from `ICONS`: `fields` the submenu is not `fields` the command. `sync`'s is the sync button's own. */
export const MENU_ICONS: Record<string, string> = {
	write: 'M10.5 2.5l3 3L6 13H3v-3z',
	read: 'M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8zM8 6.5a1.5 1.5 0 1 0 0 3a1.5 1.5 0 1 0 0-3z',
	fields: 'M2.5 2.5h11v11h-11zM2.5 6.2h11M2.5 9.8h11M6.2 2.5v11',
	settings: 'M2.5 4h11M2.5 8h11M2.5 12h11M5 2.5v3M11 6.5v3M7 10.5v3'
};

/** The mark `command` wears: its own, else its command's (`db backup` wears db's). */
export const iconOf = (command: CommandButton): string =>
	(command.press === 'menu' ? MENU_ICONS[command.name] : (ICONS[command.name] ?? ICONS[command.name.split(' ')[0]])) ?? '';

/**
 * The command the line is pending, from what is typed in it.
 *
 * The FIRST WORD, whole: a tap on `moment` puts `moment ` in the box and a
 * person typing the same word, or an old word for it (`occur`), reaches the
 * same state, because both are read back off the one buffer (#lib/bar.ts's
 * `text`) rather than from a second store a tap would have to remember to set.
 */
export function pendingCommand(line: string): LogCommand | null {
	const [typed] = line.trimStart().split(/\s/, 1);
	const word = ALIASES[typed] ?? typed;
	return (LOG_COMMANDS as string[]).includes(word) ? (word as LogCommand) : null;
}

/** `true` among a change field's states: the state that turns it on or off. */
const hasTrue = (button: FieldButton): boolean =>
	button.shape.kind === 'change' && button.shape.states.includes('true');

/** Is the field on now — an on/off field on, or a change field in a state? */
const on = (button: FieldButton): boolean => button.current !== undefined;

/**
 * Can `command` be spent on `button` (vividn's rules above)? ONE rule, read
 * both ways (da-57j14): a pending command keeps the fields it fits
 * (`fieldsUnder`), and a loaded field keeps the commands that fit it
 * (`commandsUnder`) — so the two lists can never disagree.
 */
export function commandFits(command: LogCommand, button: FieldButton): boolean {
	switch (command) {
		case 'moment':
		case 'record':
		case 'change':
		case 'allday':
			return button.shape.kind === command;
		// An on/off field takes both whatever its state (da-vpzbv, vividn: «this
		// would be how a user might enter in a repeated state because of
		// forgetting»): an on or an off re-logged is one that was missed.
		case 'on':
			return button.shape.kind === 'on/off' || hasTrue(button);
		case 'off':
			return button.shape.kind === 'on/off' || (button.shape.kind === 'change' && on(button));
		default:
			return true;
	}
}

/**
 * The fields a pending command leaves showing: the ones it can actually be
 * spent on. No pending command leaves the list whole.
 */
export const fieldsUnder = (buttons: FieldButton[], command: LogCommand | null): FieldButton[] =>
	command ? buttons.filter((b) => commandFits(command, b)) : buttons;

/**
 * Does the bar hold a DURATION — a length waiting to be spent (da-qu16)?
 *
 * The pill a RANGE carries, whichever view made it (`SPAN_ID` in
 * #lib/cursor.ts): `25m` from the nowview, `start_time=09:00` from the
 * day strip (da-ku7y). The two spellings are one pill of one KIND, so this
 * needs no clause for the second — which is what asking the kind rather
 * than reading the text buys.
 */
export const hasDuration = (all: Pill[]): boolean => all.some((p) => p.kind === 'dur');

/**
 * The word a pill puts on the command line: its text, but a bare length
 * (`25m`, `1h30m`) goes as `dur=25m` (da-g2ruk). vividn, 11-03: «let's
 * unify on not using dur= and rather the 12m format for duration», so the
 * pill SAYS `25m`; the line keeps the key because a bare one is the
 * duration only where no slot comes first — `on F 25m` on a field
 * declaring a key fills the key, `change F 25m` with no state is the state,
 * and `record F 25m` is the slug. A text with its key already (`start_time=`,
 * an old session's `dur=25m`) goes as it is.
 */
export const pillWord = (p: Pick<Pill, 'kind' | 'text'>): string => (isLength(p) ? `dur=${p.text}` : p.text);

/** Is it a bare length, `25m`, rather than a keyed text? The pill draws its arrow then (#lib/LengthArrow.svelte). */
export const isLength = (p: Pick<Pill, 'kind' | 'text'>): boolean => p.kind === 'dur' && !p.text.includes('=');

/**
 * Does a length pill SAY its key, `dur=25m`, rather than `←25m` (da-wdcbo)?
 * vividn, 11-03: «if there are positional data, make the data pill say
 * dur=». So where the field pill on the bar declares positional keys — the
 * spec's `keys` (`fields --json`, `declared_keys` as the CLI binds them),
 * where a bare `25m` would fill one — the pill says what the line gets.
 */
export const saysKey = (all: Pill[], fields: FieldButton[]): boolean => {
	const field = argumentsOf(all).find((p) => p.kind === 'field');
	return !!fields.find((b) => b.field === field?.text)?.keys?.length;
};

/**
 * The fields a loaded duration leaves showing: an on/off field and a change
 * field, the two kinds that draw a period (vividn, 09-17: «if a duration is
 * selected, filter down to start/switch based fields»).
 *
 * A moment has no length and a record is typed, so a loaded length hides
 * them and dismissing it brings them back. It COMPOSES with a pending
 * command: this filter and `fieldsUnder` both apply, exactly as the `-t`
 * pill and the command do.
 */
export function fieldsWithDuration(buttons: FieldButton[], duration: boolean): FieldButton[] {
	if (!duration) return buttons;
	return buttons.filter((b) => b.shape.kind === 'on/off' || b.shape.kind === 'change');
}

/**
 * What a tap on `button` runs while `command` is pending — `<command> <field>`,
 * at once (C1).
 *
 * `null` where that argv is not a whole command: `change <field>` has no
 * state to change to, so the field's own state list asks which first
 * (da-g635) and runs `change <field> <state>`; `record <field>` has nothing
 * in it, so the field does what it always does and leaves the line for a
 * person to finish typing. Both still spend the pending command — it is the
 * command they end in.
 */
export function pendingPress(command: LogCommand, button: FieldButton): string[] | null {
	if (command === 'change' && button.shape.kind === 'change') return null;
	if (command === 'record' && button.shape.kind === 'record') return null;
	return [command, button.field];
}

/**
 * What a tap on a field button DOES (da-z8z40): `run` — something is
 * logged at once — `fill` — a line is left in the prompt — or `show` —
 * the field opens in the collect widget (da-w6437).
 *
 * The collect widget's press is decided here and its outline is drawn from
 * the same answer (the `fires` pattern): a `run` is the double outline, so
 * a button cannot act without looking as if it does. A pending command runs
 * wherever `pendingPress` has an argv, but fills where it has keys to type
 * (`typesKeys`); otherwise a record fills, and `oneTap` decides the rest.
 */
export type PressKind = 'run' | 'fill' | 'show';
export function pressKind(button: FieldButton, pending: LogCommand | null, oneTap = false): PressKind {
	if (pending !== null && pendingPress(pending, button)) return typesKeys(pending, button) ? 'fill' : 'run';
	if (button.shape.kind === 'record') return 'fill';
	// One click (`clicks`, da-gra0z) logs what
	// needs nothing more: a moment, or a block started or ended. vividn,
	// 2026-11-04: «you can still record with 1 tap unless there is some extra
	// data or whatever needed», so a change field opens to its states, which it needs.
	if (oneTap && button.shape.kind !== 'change') return typesKeys(button.current ? 'off' : 'moment', button) ? 'fill' : 'run';
	// Otherwise a bare tap opens the field in the collect widget, to set its
	// time, state and data before it is logged (da-w6437) — a running block
	// too: ending it is the commands' (vividn, 2026-11-03 16:07: «"off" be one
	// of the commands on the bottom»), or `off` pending on the line.
	return 'show';
}

/**
 * Does a log by `command` leave its line for keys to be typed rather than run
 * (da-chml5)? vividn, 09-27: «occur/start/end run at once with a double
 * outline, unless there are required keys only, in which case fill the
 * line and focus it» — so any log of a field with a key that has no
 * default, but `off`, which takes none (da-o4jaa). A field tap, a state
 * in its ring and a command under its pill (`fieldCommand`) all ask this.
 */
export const typesKeys = (command: string, button: FieldButton): boolean => command !== 'off' && button.required.length > 0;

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
	flags: Flag[];
	/** The words that pick a subcommand (`db backup`), from the CLI's own command tree (da-803mk). */
	subcommands?: { name: string }[];
	/** Does it run with nothing after its name (`sync` does, `db` does not)? */
	bare?: boolean;
}

/** One flag of `datum help --json`: its spellings, its help line, and whether a value follows it (da-st2xl). */
export interface Flag {
	short: string | null;
	long: string;
	takes_value?: boolean;
	/** Its value only ever comes attached (`-w=2`): a bare one takes no word after it (da-e9e5g). */
	value_optional?: boolean;
	help?: string;
}

/** One row of `datum alias list --json`. */
export interface Alias {
	name: string;
	expansion: string;
	source: 'user' | 'derived';
	field?: string;
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
	 * `meta show <FIELD>`, where a subcommand comes first.
	 *
	 * The slot and not a flag, because a field pill's word has to land IN
	 * it: the field is a line's first positional, so a field appended to an argv that
	 * already carries one is read as that field's STATE — silently, as data
	 * (`fieldSlotOpen` in #lib/selection.ts is the other half of this).
	 */
	field: false | 1 | 2;
	/**
	 * Some usage line puts a POSITIONAL after the id (da-3lxol): a VALUE
	 * (`update <ID> [VALUE | KEY=VALUE]...`) or a time (`retime [SLUG]
	 * [T]`) the command is waiting for. Measured: `retime <hid>` alone
	 * moves the entry to NOW and `update <hid>` alone rewrites nothing —
	 * both run, neither is what a tap meant.
	 */
	more: boolean;
	/** Its subcommands' names (`setup`, `backup`, … of `db`), and whether it runs without one. */
	subcommands: string[];
	bare: boolean;
	/** Its flags, as the CLI lists them. */
	flags: Flag[];
}

/** Commands by name AND by alias (`end` is `stop`'s). */
export type Grammar = Map<string, Takes>;

/** An id right after the command's name, and a `[,` after it when it takes a list. */
const ID_ARG = /^datum \S+ [<[](?:ID_OR_SLUG|SLUG|HID)(\[,)?/;

/**
 * A FIELD as the command's first positional, with at most a SUBCOMMAND
 * before it: `datum tail [FIELD]`, `datum meta show <FIELD>`.
 *
 * The middle word must be a bare one, so `datum stats <FIELD[:STATE]>`
 * reads as the field at slot 1 rather than as a subcommand. A field behind
 * a FLAG — `datum grep <PATTERN>... [-f FIELD]`, `retime --field F` — is
 * not this: a bare word put in its place would be a second pattern, not a
 * field, so those commands take no field pill.
 */
const FIELD_ARG = /^datum \S+(?: ([a-z][\w-]*))? [<[]FIELD/;

/** A usage line's words, a bracketed group (`[--version N | --diff A B]`) being one. */
function words(usage: string): string[] {
	const out: string[] = [];
	let depth = 0;
	let word = '';
	for (const c of usage) {
		if (/\s/.test(c) && !depth) {
			if (word) out.push(word);
			word = '';
			continue;
		}
		if ('[<('.includes(c)) depth++;
		else if (']>)'.includes(c)) depth = Math.max(0, depth - 1);
		word += c;
	}
	if (word) out.push(word);
	return out;
}

export function takesOf(entry: GrammarEntry): Takes {
	const idLines = entry.usage.map((u) => u.trim()).filter((u) => ID_ARG.test(u));
	const ids = idLines.map((u) => ID_ARG.exec(u)!);
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
	// Past `datum <name> <ID>`, anything but a flag is a word still to come.
	const more = idLines.some((u) => words(u).slice(3).some((w) => !/^\[?-/.test(w)));
	return { id, time, field, more, subcommands: (entry.subcommands ?? []).map((s) => s.name), bare: !!entry.bare, flags: entry.flags };
}

export function grammarOf(entries: GrammarEntry[]): Grammar {
	return new Map(entries.flatMap((e) => [e.name, ...e.aliases].map((name) => [name, takesOf(e)] as const)));
}

/** The hids an `hid` pill holds, in the order they were selected. */
export const hidsOf = (pill: Pill): string[] => pill.text.split(',').filter(Boolean);

// ---- a pill's ROLE (da-tu30m) -------------------------------------------
//
// vividn, 2026-09-25: «we need to somehow maintain the field filter, but have
// active in the input bar only the hid». A field pill did two jobs — it
// narrowed the panes AND named the field in the command — and with an hid
// beside it no command took both, so every button but Help went.
//
// So a pill is an ARGUMENT (in the argv, the preview and the command
// filter) or CONTEXT (it narrows the panes and nothing else). Only
// `commandsUnder`, `fires` and `withPills` read pills for a command, and
// each reads `argumentsOf` itself, so no caller can hand one a context pill.

export type Role = 'argument' | 'context';

/**
 * The rules that make a pill CONTEXT; any that holds does. A new rule is a
 * line here, never a case at a call site.
 */
const CONTEXT: ((pill: Pill, all: Pill[]) => boolean)[] = [
	// An entry already fixes its field.
	(pill, all) => pill.kind === 'field' && all.some((p) => p.kind === 'hid')
];

export const roleOf = (pill: Pill, all: Pill[]): Role =>
	CONTEXT.some((rule) => rule(pill, all)) ? 'context' : 'argument';

/** The pills a command sees. */
export const argumentsOf = (all: Pill[]): Pill[] => all.filter((p) => roleOf(p, all) === 'argument');

/**
 * Can a command whose grammar is `takes` spend this pill? A `time` pill
 * wants `-t`; `-D`'s `allday` wants a command listing `--full-day` (da-us1n1); an `hid` pill wants an id, and more than one hid wants a
 * command that takes several; a `field` pill wants a command that names a
 * field. A kind this does not know is let through.
 *
 * A command the grammar does not know — an alias, or no
 * grammar at all — takes every pill but an hid and a FIELD: `coffee <hid>`
 * would log the hid as coffee's STATE (vremya, review 1498), and `coffee`
 * typed under a `mood` pill would log `mood` as coffee's state the same
 * way — the same shape, so the same refusal (da-8810n). The time cursor it
 * takes, as it always has.
 */
export function fits(takes: Takes | undefined, pill: Pill): boolean {
	if (!takes) return pill.kind !== 'hid' && pill.kind !== 'field';
	if (pill.kind === 'time') return takes.time;
	if (pill.kind === 'allday') return takes.flags.some((f) => f.long === 'full-day');
	if (pill.kind === 'hid') return hidsOf(pill).length > 1 ? takes.id === 'list' : !!takes.id;
	if (pill.kind === 'field') return takes.field !== false;
	return true;
}

/**
 * The commands the widget draws under `pills`: the ones whose grammar takes
 * every pill, in their usual order. The rest are HIDDEN (vividn, 09-17: «let's
 * do hidden»), as the collect widget hides what a pending command cannot be
 * spent on. No pills is the whole list; with no grammar, an hid pill is
 * taken by nothing and hides every command until the grammar is read.
 *
 * A field pill narrows the LOG_COMMANDS too (da-57j14, vividn: «When an occur field
 * is selected, start, end, and switch should be filtered out»): `fields`
 * are the collect widget's buttons, and one stays only if it `commandFits`
 * the pill's field — an ARGUMENT field: a context one narrows no command. A field not among them — one not logged yet — keeps
 * every log command, as it has no kind to narrow by.
 */
export function commandsUnder(grammar: Grammar | null, pills: Pill[], fields: FieldButton[] = []): CommandButton[] {
	const name = argumentsOf(pills).find((p) => p.kind === 'field')?.text;
	const field = fields.find((b) => b.field === name);
	return COMMANDS.filter((command) => {
		// Help takes no pill and explains every one, so no pill hides it.
		if (command.press === 'help') return true;
		if (field && command.press === 'pend' && !commandFits(command.name as LogCommand, field)) return false;
		const takes = grammar?.get(command.name);
		return argumentsOf(pills).every((p) => fits(takes, p));
	});
}

/**
 * Does a tap on a command with this grammar RUN, with `pills` loaded,
 * rather than fill the line (da-3lxol)? The one predicate: the press path
 * and the button's `runs` class both read it, so a button never looks as
 * if it fires when it does not.
 *
 * vividn, 09-22: «if I select a document, and then hit delete, it should be
 * deleted. Same with all the commands except for update». Derived rather
 * than listed: a selected hid completes an id command that waits for
 * nothing after the id (`delete`, `get`, `history`, `revert`), and
 * a command that does wait (`more`) is completed by a loaded time only
 * when `-t` is what it takes instead — `retime`, whose `[T]` and
 * `-t T` are two lines of one grammar (vividn, 09-17: «retime should just
 * fire when you tap it»). `update`'s VALUE no pill holds, so it fills.
 * With no hid loaded, nothing here fires: `undo` is `press: 'run'` on
 * its own, and its redo icon runs whenever it is lit.
 */
export function fires(takes: Takes | undefined, all: Pill[]): boolean {
	const pills = argumentsOf(all);
	if (!takes?.id || !pills.some((p) => p.kind === 'hid') || !pills.every((p) => fits(takes, p))) return false;
	return !takes.more || (takes.time && pills.some((p) => p.kind === 'time'));
}

/**
 * A log command under a FIELD pill (da-lxqqz), vividn, 09-26: «when a field is
 * selected, then the start/end/occur buttons should … auto trigger entry
 * (unless there are extra keys to be filled in, in which case the input
 * line should automatically be selected for typing in those fields». So
 * `moment`, `on`, `off` and `allday` with a known field's pill as an argument
 * RUN, unless that field has a key with no default (`required`); then the
 * tap writes `<command> <field> ` for the keys to be typed after. `off`
 * always runs (da-o4jaa), vividn, 09-27: «generally speaking I don't save
 * any extra_data in this one, so I think that should be the default» —
 * its keys are never prompted, only typed. `null` where there is no such
 * pill, and the press is what it always was.
 */
export function fieldCommand(name: string, all: Pill[], fields: FieldButton[]): { field: Pill; runs: boolean } | null {
	const field = argumentsOf(all).find((p) => p.kind === 'field');
	const button = fields.find((b) => b.field === field?.text);
	if (!field || !button || !['moment', 'on', 'off', 'allday'].includes(name)) return null;
	return { field, runs: !typesKeys(name, button) };
}

/**
 * The command the loaded pills make ready (da-h3jcu), which leads the
 * command pane as its suggested action. vividn, 2026-11-03: «when pills make
 * a command ready, it's the pane's suggested action: a larger circle, dashed
 * while something's missing, filled in its field's color once it can run».
 *
 * An ARGUMENT field pill of a known field names it, by the field's kind as
 * a bare tap on it reads it: a moment is `moment`, an on/off field `on` or,
 * while on, `off`, a change field `change` to the state pill's state, a
 * record `record`. `ready` is whether it runs as it stands: a change with no state, a record
 * and a field with a key to type (`typesKeys`) are missing something, so the
 * circle is dashed and a tap writes the line instead. No field pill, an
 * unknown field or a field that is context (under an hid) suggests nothing.
 *
 * A moment field with its entry exactly at the selected time (`here`, which
 * collect in 2-click mode lends the pane) leads with `delete` of that entry
 * instead (da-s8gr7), vividn, 2026-11-05: «maybe the default action should
 * be delete instead of log, since something is already logged there»; its
 * `moment` stays among the commands beside it.
 */
export interface Suggestion {
	command: LogCommand | 'delete';
	field: string;
	/** The change's state, from the state pill. */
	state?: string;
	/** What the circle says: `moment coffee`, `change → reading`, `change → ?`. */
	label: string;
	/** Its fill: the state's color where there is one, else the field's. */
	color: string;
	ready: boolean;
	/** What it runs: the command, the field pill and a change's state pill (collect's, right after it) joining it as a press places them. */
	argv: string[];
}

export function suggestionOf(all: Pill[], fields: FieldButton[]): Suggestion | null {
	const field = argumentsOf(all).find((p) => p.kind === 'field');
	const button = fields.find((b) => b.field === field?.text);
	if (!field || !button) return null;
	if (button.here) return { command: 'delete', field: field.text, label: `delete ${field.text}`, color: button.color, ready: true, argv: ['delete', button.here] };
	const { kind } = button.shape;
	const command: LogCommand = kind === 'on/off' ? (button.current ? 'off' : 'on') : kind;
	const state = command === 'change' ? all.find((p) => p.kind === 'state') : undefined;
	return {
		command,
		field: field.text,
		state: state?.text,
		label: command === 'change' ? `change → ${state?.text ?? '?'}` : `${command} ${field.text}`,
		color: (state && (state.color ?? button.stateColors[state.text])) || button.color,
		ready: command !== 'record' && (command !== 'change' || !!state) && !typesKeys(command, button),
		argv: [command]
	};
}

// ---- subcommands (da-803mk) ---------------------------------------------
//
// vividn, 2026-09-25: «for commands with subcommands, like db, clicking on
// them should then show the subcommands in the commands widget instead.»
// A tap on `db` DRILLS: the bar shows back and db's subcommands, and a
// tap on one fills `db backup ` — so `db `, which runs nothing, is never
// what a tap leaves on the line. A command that also runs bare is offered
// among its subcommands as itself.

/** The subcommands whose press opens the «Backups» place. */
const BACKUPS = ['db backup', 'db restore'];

/**
 * Does a tap on `name` open the setup widget? `setup` does (da-ybai9,
 * vividn: «no need for sub commands or pressing enter»), on a new field or
 * the pilled one: a field is made and edited in a place (da-hbrfg), which
 * the pane opens with the fill. `track` writes its word as any command does
 * (da-fg4w4, vividn: «track shouldn't open the setup widget, it should just
 * put the word track in the command line»).
 */
export const opensField = (name: string): boolean => name === 'setup';

/** Does a tap on `name` drill into its subcommands? Not where it opens the setup widget instead. */
export const drills = (grammar: Grammar | null, name: string): boolean =>
	!!grammar?.get(name)?.subcommands.length && !opensField(name);

/** Back to the bar before: out of a submenu, a command's subcommands or its flags. */
export const BACK: CommandButton = { name: 'back', label: 'back', press: 'back' };

/**
 * The bar while `name` is drilled into: back, Help (which no bar is
 * without, so a subcommand can be explained), `name` itself if it runs
 * bare, then its subcommands.
 *
 * `db backup` and `db restore` OPEN the «Backups» place rather than fill
 * the line (da-9ow8m): vividn, «the "backup" command in the commands widget
 * … should that open this backups menu directly? It could». A typed `db
 * backup …` keeps the CLI's meaning.
 */
export function subcommandsOf(grammar: Grammar | null, name: string): CommandButton[] {
	const takes = grammar?.get(name);
	return [
		BACK,
		HELP,
		...(takes?.bare ? [{ name, label: name, press: 'fill' as const, next: 'type' as const }] : []),
		...(takes?.subcommands ?? []).map(
			(sub): CommandButton =>
				BACKUPS.includes(`${name} ${sub}`)
					? { name: `${name} ${sub}`, label: sub, press: 'backups' }
					: { name: `${name} ${sub}`, label: sub, press: 'fill', next: 'type' }
		)
	];
}

// ---- a command's flags (da-st2xl) ---------------------------------------
//
// vividn, 2026-09-25: «long pressing on a command, like occur should add it
// to the input box as well, and then display various flags that you can
// add, like -q, -t, etc, sorted in a useful order … for -t keyboard should
// be opened on mobile so that you can type something in.» The flags, their
// help and whether a value follows are the CLI's (`help --json`); only the
// order is written here.

/**
 * The flags a hand reaches for first, in that order: when (`-t`, `-d`,
 * `-y`, `-q`), then a comment, a moment, a key. Every other flag follows
 * in the CLI's own order.
 */
const FIRST_FLAGS = ['-t', '-d', '-y', '-q', '-c', '-m', '-k'];

/** The word that goes on the line: the short spelling where there is one. */
export const flagWord = (flag: Flag): string => (flag.short ? `-${flag.short}` : `--${flag.long}`);

/** Does holding `name` open its flags? Only a command that has some. */
export const hasFlags = (grammar: Grammar | null, name: string): boolean => !!grammar?.get(name)?.flags.length;

/**
 * The bar while `name`'s flags are open: back, Help, then its flags. A
 * flag that takes a value is typed next, so its tap opens the keyboard.
 */
export function flagsOf(grammar: Grammar | null, name: string): CommandButton[] {
	const rank = (flag: Flag) => {
		const i = FIRST_FLAGS.indexOf(flagWord(flag));
		return i < 0 ? FIRST_FLAGS.length : i;
	};
	// A stable sort, so the rest keep the CLI's order.
	const flags = [...(grammar?.get(name)?.flags ?? [])].sort((a, b) => rank(a) - rank(b));
	return [
		BACK,
		HELP,
		...flags.map(
			(flag): CommandButton => ({
				name: flagWord(flag),
				label: flagWord(flag),
				press: 'flag',
				next: flag.takes_value ? 'type' : 'tap',
				help: flag.help
			})
		)
	];
}

/**
 * The line after a tap on `command`'s `flag`: appended, with a space for
 * what comes next, when the line is that command's; otherwise the command
 * and the flag afresh. The bar outlives the line — a run spends it, a
 * reader types another command — so every line a flag builds starts with
 * the command whose flags are showing (vremya, PR #216).
 */
export function withFlag(line: string, command: string, flag: string): string {
	const [word] = line.trimStart().split(/\s/, 1);
	return word === command ? `${line.trimEnd()} ${flag} ` : `${command} ${flag} `;
}

/**
 * The line's words when everything after its command is a flag, each one
 * that takes a value followed by it (`moment -q`, `moment -t 1400`): a field
 * tap under that command runs them with the field after, and spends the line
 * (da-bbmpx). Null for anything else — a positional (`moment half-typed`)
 * is a line being typed, which a press never takes, and a trailing `-t`
 * waits for its value, which the field would otherwise become. A flag
 * whose value only ever comes attached (`value_optional`) is not waiting.
 */
/** The flags of `takes` whose value is the next word (`-t 1400`); none without a grammar. */
export const valuedFlags = (takes: Takes | undefined): string[] =>
	(takes?.flags ?? []).filter((f) => f.takes_value && !f.value_optional).flatMap((f) => [`--${f.long}`, f.short ? `-${f.short}` : '']).filter(Boolean);

export function flagLine(words: string[], takes: Takes | undefined): string[] | null {
	// No grammar: a value flag cannot be told from a boolean one, so only a bare command.
	if (!takes) return words.length > 1 ? null : words;
	const valued = valuedFlags(takes);
	for (let i = 1; i < words.length; i++) {
		if (!words[i].startsWith('-')) return null;
		if (valued.includes(words[i]) && ++i === words.length) return null;
	}
	return words;
}

// ---- the scroll rule (every button bar) --------------------------------
//
// vividn: «we should always maintain horizontal scrolling in the commands
// widget, so when it is tall enough, do three rows that scroll
// horizontally» — and the collect widget the same (da-vu7pi). So a bar is
// always reached along ONE axis, and a taller box buys it more rows, up
// to `MAX_ROWS` unless the widget lifts it (`maxRows` on ButtonRows: the
// collect widget takes every row its box fits, da-arms4), never a wrap
// onto the other axis. The rows are counted
// from the BOX the pane's layout gave the bar and the size of one button
// — never from what the bar is drawing, which is the answer and would
// make the question circular.

/** The gap between two buttons, and so between two rows (0.4rem). */
export const ROW_GAP = 6.4;

/** The most rows a bar draws, however much room it has, unless its widget says otherwise. */
export const MAX_ROWS = 3;

/** Rows of `rowHeight` buttons that fit in a box `height` tall, at least one. */
export function rowsIn(height: number, rowHeight: number): number {
	if (!(rowHeight > 0)) return 1;
	return Math.max(1, Math.floor((height + ROW_GAP) / (rowHeight + ROW_GAP)));
}

/**
 * `items` dealt into `rows` lines by index — item `i` to line `i % rows`
 * (vividn, 2026-09-21: «each row gets independent spacing but buttons
 * automatically flow into the left most space»). Each line packs its own
 * buttons at their own widths, so no button is padded out to a
 * neighbour's column, while the first `rows` items still stand at the
 * start of the lines — the order is the caller's and survives the split.
 * No line is left empty: fewer items than rows draw fewer lines, and no
 * items draw none.
 */
export function lanesOf<T>(items: T[], rows: number): T[][] {
	const lanes: T[][] = Array.from({ length: Math.min(Math.max(1, rows), items.length) }, () => []);
	items.forEach((item, i) => lanes[i % lanes.length].push(item));
	return lanes;
}

/**
 * The axis a bar is reached along: `x`, rows reached left and right, is
 * every bar today, the ❯'s commands over the bottom bar's output among them
 * (da-qzzox); `y`, columns reached up and down, is what a pivoted
 * dayview/nowview will want of the collect widget (da-a6guc).
 */
export type Axis = 'x' | 'y';

/**
 * Tell `onRows` how many lines fit, now and whenever the box changes —
 * all that fit: the cap is the widget's (`maxRows` on ButtonRows).
 *
 * `bar` is the box the LAYOUT gave the widget — a flex child with no
 * size of its own across `axis` — and `probe` one of the widget's own
 * buttons, drawn and hidden, so the line's size is the size the buttons
 * really have rather than a number written down twice. The bar's own
 * content is never asked: it is the answer, and asking it would make the
 * question circular.
 */
export function watchRows(
	bar: HTMLElement,
	probe: HTMLElement,
	onRows: (rows: number) => void,
	axis: Axis = 'x'
): () => void {
	const measure = () =>
		onRows(
			axis === 'x'
				? rowsIn(bar.clientHeight, probe.offsetHeight)
				: rowsIn(bar.clientWidth, probe.offsetWidth)
		);
	const observer = new ResizeObserver(measure);
	observer.observe(bar);
	observer.observe(probe);
	measure();
	return () => observer.disconnect();
}
