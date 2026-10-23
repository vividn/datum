// The web's «which verb?» — the four answers, as data.
//
// A field cannot be born verbless (da-c8mh): `track <field>` on a field
// with neither a spec nor a log refuses and names the four choices. The
// CLI asks with that sentence; the web asks with `FieldForm.svelte`, and
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
// own render. The form is `FieldForm.svelte`, in the setup widget
// (`fieldWidget.ts`).

import type { SpecKey } from './fields.js';
import type { RunResult } from './run.js';

/** The verbs a spec may carry — `VERB_CHOICES` in datum-core's field_spec. */
export type Verb = 'occur' | 'start' | 'switch' | 'add';

/** One of the four, as the modal draws it. */
export interface VerbChoice {
	verb: Verb;
	/** The glyph the CLI prints after `TRACK` for a field declared this way. */
	glyph: string;
	/** The shape's name, the one a newcomer chooses by (da-fu29p); vividn's names of 2026-10-14, shared with the welcome. */
	what: string;
	/** What a field of this shape is, in a line, under its name. */
	line: string;
}

/**
 * The four, in the CLI's order (`VERB_CHOICES`): `occur` first, the
 * smallest claim a field can make.
 */
export const VERBS: VerbChoice[] = [
	{ verb: 'occur', glyph: '●', what: 'occurrence', line: 'a single point in time (no state)' },
	{ verb: 'start', glyph: '█', what: 'on/off', line: 'on for a while, then off' },
	{ verb: 'switch', glyph: '▞', what: 'switch', line: 'one state until the next (custom states)' },
	{ verb: 'add', glyph: '¢', what: 'card', line: 'keys and values, with no time' }
];

/** A field to start from: what an example fills the form with, never the database (da-fu29p). */
export interface Example {
	field: string;
	verb: Verb;
	/** What it shows of datum, under its name. */
	shows: string;
	color: string;
	/** Its icon in the examples, saved as the field's glyph. */
	glyph: string;
	/** Each state and its colour, in the order they stand. */
	states?: Record<string, string>;
	keys?: Pick<KeyRow, 'name' | 'kind' | 'in_slug'>[];
	/** A switch whose bare state names an entry of this field (`state_lookup_field`). */
	lookup?: string;
	neverFalse?: boolean;
}

const dataKey = (name: string, kind: KeyKind = 'optional', in_slug = false) => ({ name, kind, in_slug });
/** A card's id: required, and in the slug, as `-K id` makes it — `person:vr520`. */
const id = dataKey('id', 'required', true);

/**
 * vividn's starting catalog, a few per shape (2026-10-14), to be refined with
 * him: «a nice color (potentially symbol), any extra data, states, etc is
 * filled in for you». A spec has no range, so mood's -5 to +5 is its line.
 */
export const EXAMPLES: Example[] = [
	{ field: 'coffee', verb: 'occur', shows: 'each cup, and its size', color: '#c69c6d', glyph: '☕', keys: [dataKey('size')] },
	{ field: 'mood', verb: 'occur', shows: 'a level, -5 to +5', color: '#e8c547', glyph: '🙂', keys: [dataKey('level', 'required')] },
	{ field: 'headache', verb: 'occur', shows: 'each one, and how bad', color: '#d46a6a', glyph: '⚡', keys: [dataKey('severity')] },
	{ field: 'medication', verb: 'occur', shows: 'what you took, and how much', color: '#7fb8d4', glyph: '💊', keys: [dataKey('name', 'required'), dataKey('dose')] },
	{ field: 'sleep', verb: 'start', shows: 'on at night, off in the morning', color: '#7b68ee', glyph: '🌙' },
	{ field: 'screentime', verb: 'start', shows: 'on while you are on a screen', color: '#8fa3bf', glyph: '📱' },
	{ field: 'work', verb: 'start', shows: 'on when you start, off when you stop', color: '#d4a35a', glyph: '💼' },
	{ field: 'shower', verb: 'start', shows: 'on in, off out', color: '#5ac8d4', glyph: '🚿' },
	{ field: 'menstruation', verb: 'start', shows: 'on while it lasts, off when it ends', color: '#d64545', glyph: '🩸' },
	{
		field: 'activity',
		verb: 'switch',
		shows: 'email, meeting or deepwork, until the next',
		color: '#c8c8c8',
		glyph: '🗂',
		states: { email: '#4f8fe8', meeting: '#d48788', deepwork: '#7bd389' }
	},
	{
		field: 'environment',
		verb: 'switch',
		shows: 'where you are: home, office, car, train…',
		color: '#9acd32',
		glyph: '🌳',
		// vividn gave outside a light green and airplane an off-white (da-yqx0t).
		states: {
			home: '#e0b070',
			office: '#7f9cc4',
			inside: '#cbb994',
			outside: '#9ad48a',
			car: '#d9825b',
			bus: '#e2c35a',
			train: '#5fb3a8',
			airplane: '#ece8df'
		},
		neverFalse: true
	},
	{
		field: 'reading',
		verb: 'switch',
		shows: 'each state a book card: reading dune is book:dune',
		color: '#c39bd3',
		glyph: '📖',
		lookup: 'book'
	},
	{ field: 'person', verb: 'add', shows: 'a card with a name and a city', color: '#e59866', glyph: '👤', keys: [id, dataKey('name'), dataKey('city')] },
	{ field: 'book', verb: 'add', shows: 'a card with a title and an author', color: '#c39bd3', glyph: '📚', keys: [id, dataKey('title'), dataKey('author')] },
	{ field: 'place', verb: 'add', shows: 'a card with a name and an address', color: '#76d7c4', glyph: '📍', keys: [id, dataKey('name'), dataKey('address')] }
];

/** The action word `tail` prints for a field declared with `verb`. */
export const trackLabel = (verb: Verb): string =>
	`TRACK${VERBS.find((v) => v.verb === verb)?.glyph ?? ''}`;

/** One state row of the setup form: a state, its colour, what it means and its glyph (empty for none). */
export interface StateRow {
	name: string;
	color: string;
	description: string;
	glyph?: string;
}

/** A spec as `setup F --json` prints it; only the keys the modal edits are named. */
export interface Spec {
	field: string;
	verb: Verb;
	color: string | null;
	description: string | null;
	allowed_states: string[] | null;
	state_colors: Record<string, string>;
	/** Left out of `setup F --json` when empty, as the CLI serialises it. */
	state_descriptions?: Record<string, string>;
	/** The occur dot's glyph (da-gzowt) and its overrides; each left out when empty. */
	glyph?: string;
	state_glyphs?: Record<string, string>;
	glyph_by?: GlyphBy;
	subfields?: Record<string, Subfield>;
	derived_aliases?: DerivedAlias[];
	no_autoalias?: boolean;
	/** The field is never off (da-cwieh); left out of `setup F --json` when false. */
	never_false?: boolean;
	/** The key a line's leftover words go to, as `-R KEY` sends them; left out when none. */
	remainder?: string;
	/** Those words joined into one string, as `-SR KEY` (da-zqnt3); left out when false. */
	string_remainder?: boolean;
	/** The leftover words as one comment, as `-C`; left out when false. */
	comment_remainder?: boolean;
	/** The field a bare state names an entry of (`--state-lookup-field`); left out when none. */
	state_lookup_field?: string;
	/** The field's `<svg>` icon for the dayview (da-45wgz); the band draws `glyph` in its place. */
	icon?: string;
	[key: string]: unknown;
}

/** A glyph picked by one data key's value; a value it does not name draws the field's glyph. */
export interface GlyphBy {
	key: string;
	map: Record<string, string>;
}

/** The glyph picker's curated set, in groups (`Glyph.svelte`); any other character is typed. */
export const GLYPHS: { group: string; glyphs: string[] }[] = [
	{ group: 'shapes', glyphs: [...'●○◆◇■□▲△▼▽★☆✚✖♥♦'] },
	{ group: 'arrows', glyphs: [...'↑↓←→↗↘↙↖⇑⇓↺⟳'] },
	{ group: 'weather', glyphs: [...'☀☁☂❄☃🌧☔⚡🌙🌈⛅⭐'] },
	{ group: 'food and drink', glyphs: [...'☕🍵🍺🍷🥛🍎🍞🍕🍫🍩🍪🥗'] }
];

/**
 * Whether `glyph` is two columns wide, which the terminal band draws as
 * `●` (`push_cell_point` in datum-layout's nowview_terminal.rs, by
 * unicode-width): an emoji-presentation character, or a CJK or fullwidth
 * one. A glyph is one character (`check_glyph` in the CLI's spec.rs), so
 * no variation selector ever reaches this.
 */
export const wide = (glyph: string): boolean =>
	/\p{Emoji_Presentation}|[\u1100-\u115F\u2E80-\u303E\u3041-\u33FF\u3400-\u4DBF\u4E00-\u9FFF\uA000-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE30-\uFE4F\uFF00-\uFF60\uFFE0-\uFFE6\u{20000}-\u{3FFFD}]/u.test(
		glyph
	);

/** A key's type — `SubfieldType` in datum-core's field_spec. */
export type KeyType =
	| 'string'
	| 'number'
	| 'bool'
	| 'date'
	| 'datetime'
	| 'array'
	| 'any'
	| 'ref'
	| 'number[]'
	| 'string[]'
	| 'bool[]'
	| 'ref[]';
export const KEY_TYPES: KeyType[] = [
	'string',
	'number',
	'bool',
	'date',
	'datetime',
	'array',
	'any',
	'ref',
	'number[]',
	'string[]',
	'bool[]',
	'ref[]'
];

/** A key of the entry's data as the spec declares it — `SubfieldSpec`. */
export interface Subfield {
	required: boolean;
	value_type: KeyType;
	/** Whether the key's value is part of the entry's slug: it identifies, not annotates. */
	in_slug: boolean;
	default: unknown;
}

/** How a log fills a key (da-9s98k): `-k key`, `-k key=`, `-k key=default`, `key=value`. */
export type KeyKind = SpecKey['kind'];
export const KEY_KINDS: KeyKind[] = ['required', 'optional', 'default', 'assign'];

/** One key row of the modal: a key the spec declares, as the CLI parsed it, and its type. */
export interface KeyRow {
	name: string;
	kind: KeyKind;
	/** A default's or an assign's value, as typed. */
	value: string;
	value_type: KeyType;
	/** Whether the key's value is part of the entry's slug: it identifies, not annotates. */
	in_slug: boolean;
	/** Set on the one row that takes the rest of the line, which is no slot. */
	rest?: Rest;
}

/** How a row takes the rest of the line: `-R KEY` a list, `-SR KEY` one string, `-C` one comment. */
export type Rest = 'R' | 'SR' | 'C';
/** A row's mode button — the flag that declares the key on the CLI (`datum help logging`). */
export type KeyMode = 'k' | 'K' | Rest;

/** The modes in the order the button cycles them, each with the line under a row in it. */
export const KEY_MODES: { mode: KeyMode; line: string }[] = [
	{ mode: 'k', line: '' },
	{ mode: 'K', line: "goes into the entry's id: a card's id, as in book:dune" },
	{ mode: 'R', line: 'takes the rest of the line, as a list of its words' },
	{ mode: 'SR', line: 'takes the rest of the line, as one string' },
	{ mode: 'C', line: 'the rest of the line is kept as a comment' }
];

export const modeOf = (row: KeyRow): KeyMode => row.rest ?? (row.in_slug ? 'K' : 'k');

/**
 * Where a row dragged from `from` of `rows` goes with the pointer at `y`: past a neighbor once the pointer reaches the
 * neighbor's middle, so rows of different heights settle where the pointer is and do not swap back on the next move (da-ua9gv).
 */
export function draggedTo(rows: Element[], from: number, y: number): number {
	const middle = (i: number) => {
		const r = rows[i].getBoundingClientRect();
		return r.top + r.height / 2;
	};
	let to = from;
	while (to > 0 && y <= middle(to - 1)) to--;
	while (to < rows.length - 1 && y >= middle(to + 1)) to++;
	return to;
}

/** `list` with its `from`th item moved to `to`. */
export function moved<T>(list: T[], from: number, to: number): T[] {
	const next = [...list.slice(0, from), ...list.slice(from + 1)];
	next.splice(to, 0, list[from]);
	return next;
}

/**
 * Turn `row`, one of `rows`, to its next mode, in place. Only one row takes
 * the rest of the line, so a row taking it takes it from the one that had it.
 */
export function cycle(rows: KeyRow[], row: KeyRow): void {
	const modes = KEY_MODES.map((m) => m.mode);
	const mode = modes[(modes.indexOf(modeOf(row)) + 1) % modes.length];
	if (mode !== 'k' && mode !== 'K') for (const other of rows) delete other.rest;
	if (mode === 'k' || mode === 'K') delete row.rest;
	else row.rest = mode;
	row.in_slug = mode === 'K';
}

/**
 * An alias the spec carries — `DerivedAlias`, whose NAME is stored under
 * `state` (a serde rename), whatever kind of alias it is.
 */
export interface DerivedAlias {
	state: string;
	expands_to: string;
	kind?: 'bare_field' | 'terminal_end' | 'state';
}

/**
 * The modal's key rows for `spec`: `setup F --json`'s `keys`, parsed by the CLI
 * and in binding order (da-p3hxn) — every key the spec declares, not only
 * its subfields — each with its subfield's type where it has one; then the
 * row that takes the rest of the line, if one does.
 */
export function keyRows(spec: Spec): KeyRow[] {
	const rows: KeyRow[] = ((spec.keys as SpecKey[] | undefined) ?? []).map(({ name, kind, value, in_slug }) => ({
		name,
		kind,
		value: value ?? '',
		value_type: spec.subfields?.[name]?.value_type ?? 'any',
		in_slug
	}));
	const rest: Rest | null = spec.comment_remainder ? 'C' : spec.remainder ? (spec.string_remainder ? 'SR' : 'R') : null;
	if (rest) rows.push({ name: rest === 'C' ? 'comment' : spec.remainder ?? '', kind: 'optional', value: '', value_type: 'any', in_slug: false, rest });
	return rows;
}

/** A typed default as it is stored: blank is none, a string key's is its text, any other's is JSON if it parses. */
function storedDefault({ value: typed, value_type }: KeyRow): unknown {
	if (typed.trim() === '') return null;
	if (value_type === 'string') return typed;
	try {
		return JSON.parse(typed);
	} catch {
		return typed;
	}
}
/**
 * Where a field's lane is (da-btyvj): free, `y` and `height` as fractions
 * of the day; or tied to another field by the anchor triple. A key the
 * spec does not set is absent — «nobody chose this» — not a default.
 */
export interface Place {
	y?: number;
	height?: number;
	relative_to?: string;
	anchor_pos_ratio?: number;
	anchor_size_ratio?: number;
	/** An occur dot's height over its lane's and its width over its height (da-c05v1, da-c1b6m); absent is 1. */
	glyph_scale?: number;
	glyph_aspect?: number;
}

const PLACE_KEYS = [
	'y',
	'height',
	'relative_to',
	'anchor_pos_ratio',
	'anchor_size_ratio',
	'glyph_scale',
	'glyph_aspect'
] as const;

/** The placement keys `spec` sets, and only those. */
export const placeOf = (spec: Spec): Place =>
	Object.fromEntries(PLACE_KEYS.filter((k) => spec[k] != null).map((k) => [k, spec[k]]));

/** The state rows for `spec`: its allowed states, each with its colour and description. */
export const stateRows = (spec: Spec): StateRow[] =>
	(spec.allowed_states ?? []).map((name) => ({
		name,
		color: spec.state_colors[name] ?? '',
		description: spec.state_descriptions?.[name] ?? '',
		glyph: spec.state_glyphs?.[name] ?? ''
	}));

/**
 * Where the key rows are saved — a key lives in ONE place (da-ulc0v): an
 * assign in `assign`, `id_assign` in the slug; any other key a slot, spelt
 * `key`, `key=` or `key=default`, in `data_keys` or `id_keys` in the slug,
 * in the rows' order, which is the order the words bind in (da-zqnt3). A
 * slot has no type, so a typed key also keeps a subfield for it, which
 * binds in its slot's place (`declared_keys`). The row that takes the rest
 * of the line is the remainder. The maps are absent when empty, as `setup
 * show` prints them.
 */
function placed(rows: KeyRow[]): Partial<Spec> {
	const named = rows.map((r) => ({ ...r, name: r.name.trim() })).filter((r) => r.name || r.rest === 'C');
	const slots = named.filter((r) => !r.rest && r.kind !== 'assign');
	const spell = ({ name, kind, value }: KeyRow) => (kind === 'required' ? name : `${name}=${kind === 'default' ? value : ''}`);
	const listed = (slug: boolean) => slots.filter((r) => r.in_slug === slug).map(spell);
	const assigned = (slug: boolean) =>
		Object.fromEntries(named.filter((r) => !r.rest && r.kind === 'assign' && r.in_slug === slug).map((r) => [r.name, r.value]));
	const [assign, id_assign] = [assigned(false), assigned(true)];
	const route = named.find((r) => r.rest);
	return {
		id_keys: listed(true),
		data_keys: listed(false),
		subfields: Object.fromEntries(
			slots
				.filter((r) => r.value_type !== 'any')
				.map((r) => [
					r.name,
					{ required: r.kind === 'required', value_type: r.value_type, in_slug: r.in_slug, default: r.kind === 'default' ? storedDefault(r) : null }
				])
		),
		...(Object.keys(assign).length ? { assign } : {}),
		...(Object.keys(id_assign).length ? { id_assign } : {}),
		...(route?.rest === 'C' && { comment_remainder: true }),
		...(route && route.rest !== 'C' && { remainder: route.name }),
		...(route?.rest === 'SR' && { string_remainder: true })
	};
}

/**
 * `spec` with the modal's edits laid over it — everything it does not
 * edit (placement, tags, id keys) carried as it was, and so are the keys
 * and aliases when the edit leaves them out.
 *
 * The states are the WHOLE list, which is `--from-json`'s shape: a
 * state removed takes its colour, description and glyph
 * with it, and an empty one clears the entry. A glyph goes only for a
 * state that LEFT the list — `state_glyphs` may name a state the list
 * never did — or a row that carries an empty one; a renamed row carries
 * its glyph to the new name.
 *
 * `glyph_by` null removes it, and left out carries it (the setup form
 * never edits it, da-rpfws); its map rows with no value or no glyph are
 * dropped.
 *
 * An `icon` given replaces the spec's and an empty one clears it; one
 * not given is carried (da-45wgz).
 */
export function editedSpec(
	spec: Spec,
	edit: {
		verb: Verb;
		color: string;
		description: string;
		states: StateRow[];
		glyph?: string;
		glyph_by?: GlyphBy | null;
		keys?: KeyRow[];
		/** Whether datum makes the aliases (`!no_autoalias`); when not, `aliases` is the list. */
		autoalias?: boolean;
		aliases?: DerivedAlias[];
		/** The field is never off (`never_false`, da-cwieh); left out, the spec's is kept. */
		neverFalse?: boolean;
		place?: Place;
		icon?: string;
		/** The field a bare state names an entry of; empty clears it, left out the spec's is kept. */
		lookup?: string;
	}
): Spec {
	const rows = edit.states.map((s) => ({ ...s, name: s.name.trim() })).filter((s) => s.name);
	const names = [...new Set(rows.map((s) => s.name))];
	const state_colors = { ...spec.state_colors };
	const state_descriptions = { ...spec.state_descriptions };
	const state_glyphs = { ...spec.state_glyphs };
	for (const gone of spec.allowed_states ?? []) {
		delete state_colors[gone];
		delete state_descriptions[gone];
		if (!names.includes(gone)) delete state_glyphs[gone];
	}
	for (const { name, color, description, glyph } of rows) {
		if (color) state_colors[name] = color;
		if (description.trim()) state_descriptions[name] = description.trim();
		if (glyph) state_glyphs[name] = glyph;
		else if (glyph === '') delete state_glyphs[name];
	}
	const key = edit.glyph_by?.key.trim() ?? '';
	const map = Object.fromEntries(
		Object.entries(edit.glyph_by?.map ?? {})
			.map(([v, g]) => [v.trim(), g])
			.filter(([v, g]) => v && g)
	);
	const glyph = edit.glyph ?? spec.glyph ?? '';
	const glyph_by = edit.glyph_by === undefined ? spec.glyph_by : key ? { key, map } : undefined;
	const icon = edit.icon === undefined ? spec.icon : edit.icon.trim();
	const never_false = edit.neverFalse ?? (spec.never_false === true);
	const state_lookup_field = (edit.lookup ?? spec.state_lookup_field ?? '').trim();
	// `keys` and `keys_by_state` are the CLI's reading of the spec, not the spec: never sent (da-p3hxn).
	const { state_descriptions: _d, glyph: _g, state_glyphs: _s, glyph_by: _b, icon: _i, keys: _k, keys_by_state: _ks, never_false: _n, state_lookup_field: _l, ...rest } = spec;
	// The key rows are the whole answer: every place a key can live is written from them.
	if (edit.keys)
		for (const k of ['id_keys', 'data_keys', 'assign', 'id_assign', 'remainder', 'string_remainder', 'comment_remainder']) delete rest[k];
	// The placement is the form's whole answer: free drops the anchor, tied drops y/height.
	if (edit.place) for (const k of PLACE_KEYS) delete rest[k];
	return {
		...rest,
		...edit.place,
		verb: edit.verb,
		color: edit.color || null,
		description: edit.description.trim() || null,
		allowed_states: spec.allowed_states === null && names.length === 0 ? null : names,
		state_colors,
		// Each absent when empty, as `setup F --json` prints it, so what is sent is what is read back.
		...(Object.keys(state_descriptions).length ? { state_descriptions } : {}),
		...(glyph ? { glyph } : {}),
		...(Object.keys(state_glyphs).length ? { state_glyphs } : {}),
		...(glyph_by ? { glyph_by } : {}),
		...(icon ? { icon } : {}),
		...(never_false ? { never_false } : {}),
		...(state_lookup_field ? { state_lookup_field } : {}),
		...(edit.keys && placed(edit.keys)),
		...(edit.autoalias !== undefined && { no_autoalias: !edit.autoalias }),
		// Only a list datum does not make is the person's to send; the CLI
		// rebuilds a made one on save whatever it is sent.
		...(edit.autoalias === false &&
			edit.aliases && {
				derived_aliases: edit.aliases
					.map((a) => ({ ...a, state: a.state.trim(), expands_to: a.expands_to.trim() }))
					.filter((a) => a.state && a.expands_to)
			})
	};
}

/** The form's edits as `spec` reads: what `editedSpec(spec, formOf(spec))` leaves as it was. */
export const formOf = (spec: Spec): Parameters<typeof editedSpec>[1] => ({
	verb: spec.verb,
	color: spec.color ?? '',
	description: spec.description ?? '',
	states: stateRows(spec),
	glyph: spec.glyph ?? '',
	glyph_by: spec.glyph_by ?? null,
	keys: keyRows(spec),
	autoalias: !spec.no_autoalias,
	aliases: spec.derived_aliases ?? [],
	neverFalse: !!spec.never_false,
	place: placeOf(spec),
	icon: spec.icon ?? '',
	lookup: spec.state_lookup_field ?? ''
});

/** A field nothing has declared yet: what a new field's form is laid over. */
const blank = (field: string, verb: Verb): Spec => ({ field, verb, color: null, description: null, allowed_states: null, state_colors: {} });

/**
 * A new field's spec as its form holds it, what `declareArgv` declares.
 * The states belong to `switch` alone: a field given one and then changed
 * to another verb would otherwise smuggle it out.
 */
export const draftSpec = (field: string, edit: Parameters<typeof editedSpec>[1]): Spec =>
	editedSpec(blank(field, edit.verb), edit.verb === 'switch' ? edit : { ...edit, states: [] });

/**
 * The ONE command that declares a new field, which one Undo takes back
 * whole (da-2pbvd): `setup F --from-json` with the whole of its form, and
 * `--track` or `--ignore`, which write its TRACK or IGNORED row in the same
 * transaction.
 */
export const declareArgv = (field: string, edit: Parameters<typeof editedSpec>[1], ignore = false): string[] => [
	...specArgv(draftSpec(field, edit)),
	ignore ? '--ignore' : '--track'
];

/** What a save runs: ONE command, which one Undo takes back (da-2pbvd). */
export interface Save {
	argv: string[];
}

/**
 * The ONE command that saves an edited spec: `setup F --from-json` with
 * the whole of it, so it lands entire or not at all — never a stream of
 * one-state changes half applied — and so a colour or a state can be
 * cleared, which the one-value flags cannot say.
 */
export const specArgv = (spec: Spec): string[] => [
	'setup',
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
