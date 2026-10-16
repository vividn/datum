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
	 * for every field that is not; the (+) offers the ignored ones back.
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
	/** The spec's `glyph`: what an occur of the field draws instead of `●` (da-h2io6). */
	glyph?: string;
	/**
	 * The spec says the field is never off (`never_false`, da-cwieh), and
	 * its `allowed_states` do not list `false`: its ring leaves a gap where
	 * `end` would stand.
	 */
	neverFalse?: boolean;
	/**
	 * What the field is in now, by `datum now --json`: `true` for an open
	 * block, the state's name for a switch; absent when it is off. Until
	 * that answers after a write, by the write's own echo (`guess` in
	 * $lib/layout/engine.ts).
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
	/** Where the field's band sits in the pictures, 0 at the top — `fields --json`'s `y`. */
	y: number;
	/** How tall that band is, as a share of the pictures — `fields --json`'s `height`. */
	height: number;
	/** The keys a log of it cannot do without (`requiredKeys`). */
	required: string[];
	/** Every key its spec declares (`keysOf`): the input line's frames. */
	keys: Key[];
	/** A state's keys where it declares its own (`stateKeysOf`): a switch to it frames these. */
	stateKeys: Record<string, Key[]>;
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

/** A key the spec declares for the field (`keysOf`). */
export interface Key {
	name: string;
	/** What an unwritten key is filled with, as it would be typed. */
	default?: string;
	/** No default, and a log cannot do without it. */
	required: boolean;
}

/**
 * A key as the CLI parsed it (da-p3hxn): `setup F --json`'s and `fields
 * --json`'s `keys` — `FieldSpec::declared_keys`, in binding order. The web
 * never splits a spelling: `brand` of kind assign is how it learns of an
 * assign, which no spelling carries.
 */
export interface SpecKey {
	name: string;
	kind: 'required' | 'optional' | 'default' | 'assign';
	/** A default's or an assign's value. */
	value?: string;
	in_slug: boolean;
}

/** The keys a reader types: an assigned one is fixed, and gets no frame. */
const typed = (keys: SpecKey[] | undefined): Key[] =>
	(keys ?? [])
		.filter((k) => k.kind !== 'assign')
		.map(({ name, kind, value }) => (kind === 'default' ? { name, default: value, required: false } : { name, required: kind === 'required' }));

/** The keys the spec declares, in the order a log binds them (`keys`). */
export const keysOf = (spec: FieldSpec | null | undefined): Key[] => typed(spec?.keys as SpecKey[] | undefined);

/** The keys an entry in each state that declares its own takes (`keys_by_state`, da-cej7q). */
export const stateKeysOf = (spec: FieldSpec | null | undefined): Record<string, Key[]> =>
	Object.fromEntries(
		Object.entries((spec?.keys_by_state as Record<string, SpecKey[]> | undefined) ?? {}).map(([s, keys]) => [s, typed(keys)])
	);

/**
 * The keys a log of the field cannot do without (da-lxqqz): the ones of
 * kind required. A key with a default is filled from it, so it asks
 * nothing of the person.
 */
export const requiredKeys = (spec: FieldSpec | null | undefined): string[] =>
	keysOf(spec)
		.filter((k) => k.required)
		.map((k) => k.name);

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
	/** How tall the band is, as a share of the pictures' data area. */
	height: number;
	/** How much the field is used, recent use weighing more — `--sort frecency` alone carries it (da-xtkcp). */
	frecency?: number;
}

/** One row of `datum now --json`'s `active`. */
export interface NowActive {
	field: string;
	state: unknown;
}

/** `datum now --json`. */
export interface NowListing {
	active?: NowActive[];
}

/**
 * The widget's entries: every field `datum fields` lists, as high up the
 * pictures as the CLI draws it (the `y` it resolved, da-vu7pi), and the
 * CLI's alphabetical order among equals — the order a tie falls back to.
 * `active` is `datum now --json`'s — a field it does not name is off.
 *
 * An active field no longer comes first (da-xtkcp): its button already
 * shows its state, and moving it was what put `mood` in front of `coffee`
 * under a long press. Where a button STANDS in the bar is `fieldRows`.
 */
export function buttonsFor(listing: FieldListing[], active: NowActive[] = []): FieldButton[] {
	const now = new Map(active.map((a) => [a.field, a.state]));
	return [...listing]
		.sort((a, b) => a.y - b.y)
		.map(({ field, spec, states, color, state_colors, ignored, y, height }) => {
			const state = now.get(field);
			return {
				field,
				shape: shapeFor(spec ?? undefined, states),
				ignored,
				color,
				stateColors: state_colors,
				glyph: spec?.glyph ?? undefined,
				neverFalse: !!spec?.never_false && !spec.allowed_states?.includes('false'),
				active: now.has(field),
				current: state === true || (typeof state === 'string' && state !== '') ? state : undefined,
				y,
				height,
				required: requiredKeys(spec),
				keys: keysOf(spec),
				stateKeys: stateKeysOf(spec)
			};
		});
}

/**
 * Where each field stood when the bar was last sorted (da-xtkcp): its `y`,
 * which picks its row, and its `frecency` (`datum fields --sort
 * frecency`), which places it along the row. Taken only on load and when
 * the app comes back to the foreground (`resort` in $lib/layout/engine.ts)
 * — never while it is in use, so nothing a press sets off can move a
 * button.
 */
export type Standings = Map<string, { y: number; frecency: number }>;

/** `items` cut, in order, into `rows` runs of equal count (one apart at most); none empty. */
export function bands<T>(items: T[], rows: number): T[][] {
	const n = Math.min(Math.max(1, rows), items.length);
	return Array.from({ length: n }, (_, i) =>
		items.slice(Math.floor((i * items.length) / n), Math.floor(((i + 1) * items.length) / n))
	);
}

/**
 * The bar's rows (vividn, 2026-09-24: «the frecency is left to right, the y
 * spec is top to bottom»): the fields that STOOD at the last sort, by `y`,
 * cut into `rows` bands of equal count — not fixed ranges of `y`, which
 * overflow one row and leave another empty whenever fields cluster — and
 * each band most used first, all by the `standings` and not by the
 * fields as they are now.
 *
 * A field that has appeared since (a new one tracked) moves nothing
 * either: it is not counted into the bands, and goes at the END of the row
 * its `y` falls in. With nothing standing at all — before the first sort
 * — the rows are the bands of `y` alone.
 */
export function fieldRows(buttons: FieldButton[], rows: number, standings: Standings): FieldButton[][] {
	const stood = buttons.filter((b) => standings.has(b.field));
	if (stood.length === 0) return bands(buttons, rows);
	const at = (b: FieldButton) => standings.get(b.field)!;
	const cut = bands([...stood].sort((a, b) => at(a).y - at(b).y), rows);
	const tops = cut.map((band) => at(band[0]).y);
	const lanes = cut.map((band) => band.sort((a, b) => at(b).frecency - at(a).frecency));
	for (const b of buttons)
		if (!standings.has(b.field)) lanes[Math.max(0, tops.findLastIndex((top) => top <= b.y))].push(b);
	return lanes;
}

/**
 * Where the fields widget's buttons stand: in `fieldRows`' rows, or each
 * at the height its field is drawn in the pictures (`aligned`, da-omw4n).
 */
export type Align = 'rows' | 'picture';

/**
 * The part of the pictures' 0–1 height the widget shows (da-omw4n): all of
 * it, `FULL`, until a pinch zooms in.
 */
export interface View {
	from: number;
	to: number;
}

export const FULL: View = { from: 0, to: 1 };

/** As far in as a pinch goes: an eighth of the pictures. */
const DEEPEST = 1 / 8;

/** Where `y` of the pictures lands in the box, as a share of its height. */
export const yIn = (view: View, y: number) => (y - view.from) / (view.to - view.from);

/** Which `y` of the pictures a share `at` of the box's height shows. */
export const yOut = (view: View, at: number) => view.from + at * (view.to - view.from);

/** `view` moved by `by` of the box's height, down for a positive one, and kept inside 0–1. */
export function panned(view: View, by: number): View {
	const span = view.to - view.from;
	const from = Math.min(1 - span, Math.max(0, view.from + by * span));
	return { from, to: from + span };
}

/**
 * `view` zoomed in by `factor` (out, under 1) around the share `at` of the
 * box's height, so the `y` under the pinch stays under it — as far as the
 * pictures' own edges allow, and never past `FULL` or `DEEPEST`.
 */
export function zoomed(view: View, at: number, factor: number): View {
	const span = Math.min(1, Math.max(DEEPEST, (view.to - view.from) / factor));
	const from = yOut(view, at) - at * span;
	return panned({ from, to: from + span }, 0);
}

/** A button the picture layout places: its band (`fields --json`'s `y` and `height`), and its size as drawn. */
export interface Sized {
	y: number;
	height: number;
	w: number;
	h: number;
}

/** Where a placed button stands in the box, in px. */
export interface Stand {
	x: number;
	top: number;
}

/**
 * The fields widget's second layout, «aligned to the picture» (da-omw4n):
 * each button stands where its field is drawn: centred on its band —
 * zoomed by `view` over a box `height` tall — so a button as tall as its
 * band starts where the band does and a shorter one stands in its middle,
 * as far as it stays inside the pictures. Its height is the band's at
 * `FULL`, never the zoom's: a pinch spreads the buttons, it does not
 * grow them. And it takes the LEFTMOST place
 * clear of every button already placed whose height it shares, `gap`
 * between them. So fields whose bands overlap stand side by side, a band
 * crowded past the box's width runs off its right edge, where the widget
 * scrolls, and a zoom that pulls two bands apart lets the second fall back
 * to the left.
 *
 * `items` come in the order they claim places — most used first — and the
 * stands come back in it.
 */
export function aligned(items: Sized[], height: number, view: View, gap: number): Stand[] {
	const tall = height / (view.to - view.from);
	const placed: (Stand & Sized)[] = [];
	return items.map((item) => {
		// A button taller than the box itself starts at its top and runs off its bottom.
		const top = Math.max(0, Math.min((item.y + item.height / 2) * tall - item.h / 2, tall - item.h)) - view.from * tall;
		const beside = placed.filter((p) => p.top < top + item.h && top < p.top + p.h);
		const x = [0, ...beside.map((p) => p.x + p.w + gap)]
			.sort((a, b) => a - b)
			.find((x) => beside.every((p) => x + item.w + gap <= p.x || p.x + p.w + gap <= x))!;
		placed.push({ ...item, x, top });
		return { x, top };
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
 * The colour datum paints a name that has none stored — `md5_rgb` in
 * crates/datum-commands/src/render/colors.rs: the first three bytes of
 * `md5(name)`. A field falls back to its own name's, a state to the
 * STATE's name's (`state_rgb`), never to the field's. The field form
 * shows it on a swatch left at auto (da-jf6l2), and needs it for a name
 * still being typed, which no `fields --json` has listed yet.
 */
export function defaultColor(name: string): string {
	const bytes = new TextEncoder().encode(name);
	const words = new Uint32Array((((bytes.length + 8) >> 6) + 1) * 16);
	bytes.forEach((b, i) => (words[i >> 2] |= b << ((i % 4) * 8)));
	words[bytes.length >> 2] |= 0x80 << ((bytes.length % 4) * 8);
	words[words.length - 2] = bytes.length * 8;
	const k = Array.from({ length: 64 }, (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32));
	const r = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
	const h = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
	for (let o = 0; o < words.length; o += 16) {
		let [a, b, c, d] = h;
		for (let i = 0; i < 64; i++) {
			const round = i >> 4;
			const f = [(b & c) | (~b & d), (d & b) | (~d & c), b ^ c ^ d, c ^ (b | ~d)][round];
			const g = [i, 5 * i + 1, 3 * i + 5, 7 * i][round] % 16;
			const s = r[round * 4 + (i % 4)];
			const t = (a + f + k[i] + words[o + g]) | 0;
			[a, d, c] = [d, c, b];
			b = (b + ((t << s) | (t >>> (32 - s)))) | 0;
		}
		[a, b, c, d].forEach((v, i) => (h[i] = (h[i] + v) | 0));
	}
	return '#' + [0, 8, 16].map((s) => ((h[0] >>> s) & 255).toString(16).padStart(2, '0')).join('');
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
	/** The mark drawn beside the label, in the field's colour — a point's alone. */
	glyph?: string;
}

/**
 * How a button is painted now (vividn's rules, da-c7u8): a point is an
 * empty outline with the mark an occur of it draws beside its name — the
 * spec's glyph, else the CLI's dot — in the field's colour, and the name
 * in the app's ink (da-h2io6: «Empty button, but with the occur symbol
 * glyph next to the field name»); a block is filled while it is on and
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
 * colour as its ink, and that is over the pane, not over itself — a
 * point's name excepted, which is the app's ink beside its coloured mark.
 */
export function lookOf(button: FieldButton, variant: SwitchLabel = 'field'): Look {
	const field = button.color;
	const { current, shape } = button;
	const filled = (fill: string, label: string): Look => ({ fill, line: field, text: inkOver(fill), label });
	if (shape.kind === 'point') return { fill: null, line: field, text: 'var(--ink)', label: button.field, glyph: button.glyph ?? '●' };
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
 * How one state reads when a press opens its field (da-7q3ka), in three
 * levels — vividn: «the light tint might work … let's try it»:
 *
 *   the state the field is in NOW — solid, drawn exactly as the field's
 *     own chip draws it (`lookOf`'s switch case), and bold (the widget's);
 *   every other state — a LIGHT TINT, a quarter of its own colour, in its
 *     own outline, under the app's ink;
 *   `< Back`, `+ Add state` and `end` — hollow, and not drawn here.
 */
export function stateLook(button: FieldButton, state: string): Look {
	if (button.current === state) return lookOf(button, 'state');
	const color = button.stateColors[state] ?? button.color;
	return { fill: `color-mix(in srgb, ${color} 25%, transparent)`, line: color, text: 'var(--ink-bright)', label: state };
}

/** A button's centre, in pixels from the tap, `y` down. */
export interface Spot {
	x: number;
	y: number;
}

/** Where each button of an open field stands around the tap (`ringOf`). */
export interface Ring {
	back: Spot;
	add: Spot;
	end: Spot;
	states: Spot[];
}

/** How far the box reaches from the tap on each side, in pixels. */
export interface Room {
	up: number;
	down: number;
	left: number;
	right: number;
}

/**
 * A switch field's states RADIALLY around the tap (da-7q3ka), RANKED BY
 * DISTANCE — vividn: «make sure that the top ranked states are radially
 * close to the center». The rank is the spec's `allowed_states` order,
 * set in the States tab and never by use, so a state is always the same
 * place relative to the thumb, like a pie menu.
 *
 * The centre is the tapped field's own position, and it is `end`'s on
 * every switch field (da-cwieh) — vividn: «the button that should be
 * immediately under the cursor when it opens is "end"». A field whose
 * spec says it is `never_false` leaves a GAP there instead, `end`'s own
 * width, which is the widget's business: placed here as `end`, so every
 * other button stands where it would beside a drawn one. `< Back` stands
 * just left of the centre, on its line. Then each state in rank order
 * takes the nearest free spot — the least distance from the centre, and
 * among equals the first clockwise from twelve o'clock — where it clears
 * every button already placed by `gap`. Spots are every 2px across and
 * every half row up and down. `+ Add state` comes last, on the centre's
 * line past the rightmost button — «to the far right of all the states,
 * rather than taking up a premium spot close to the thumb».
 *
 * At the box's edges the ring is CAPPED, never shifted (da-8mk9p) — vividn:
 * «still radial, just capped on one side and spreading out further in the
 * non capped direction, but the center should still be end». A spot is
 * one whose button lies inside `room`, so the states spread along the
 * free sides. The centre moves only when it would itself run past the box
 * — as far as it must, the left edge winning in a box narrower than it,
 * which the box then scrolls. At the left edge a `< Back` that does not
 * fit beside it takes the nearest free spot inside instead, left of the
 * centre where one is, before any state (da-u9nfv) — vividn: «yes shift
 * those into another row if needed». A button no spot inside `room`
 * holds, and a `+ Add state` past the right edge, go past it, where the
 * box scrolls, rather than move anything.
 *
 * `w` is each button's width, `h` a button's height; every spot returned
 * is from the tap.
 */
export function ringOf(
	w: { back: number; add: number; end: number; states: number[] },
	h: number,
	room: Room,
	gap: number
): Ring {
	const half = (h + gap) / 2;
	const placed: { x: number; y: number; w: number }[] = [];
	const clear = (x: number, y: number, wide: number) =>
		placed.every((p) => Math.abs(p.x - x) >= (p.w + wide) / 2 + gap || Math.abs(p.y - y) >= h + gap);
	const x0 = Math.max(w.end / 2 - room.left, Math.min(0, room.right - w.end / 2));
	const y0 = Math.max(h / 2 - room.up, Math.min(0, room.down - h / 2));
	placed.push({ x: x0, y: y0, w: w.end });
	const cols = Math.ceil([w.end, w.back, ...w.states].reduce((a, b) => a + b + gap, 0) / 2);
	const turn = (p: Spot) => (Math.atan2(p.x - x0, y0 - p.y) + 2 * Math.PI) % (2 * Math.PI);
	const far = (p: Spot) => Math.hypot(p.x - x0, p.y - y0);
	const spots: Spot[] = [];
	const rows = (reach: number) => Math.max(0, Math.floor((reach - h / 2) / half));
	for (let r = -rows(room.up + y0); r <= rows(room.down - y0); r++)
		for (let c = -cols; c <= cols; c++) spots.push({ x: x0 + 2 * c, y: y0 + r * half });
	spots.sort((a, b) => far(a) - far(b) || turn(a) - turn(b));
	const inside = (p: Spot, wide: number) => p.x - wide / 2 >= -room.left && p.x + wide / 2 <= room.right;
	const put = (p: Spot, wide: number) => (placed.push({ ...p, w: wide }), p);
	const take = (wide: number, mine: (p: Spot) => boolean = () => true) => {
		const fits = (p: Spot) => p.x - wide / 2 >= -room.left && clear(p.x, p.y, wide);
		return put(spots.find((p) => fits(p) && inside(p, wide) && mine(p)) ?? spots.find((p) => fits(p) && inside(p, wide)) ?? spots.find(fits)!, wide);
	};
	const line = { x: x0 - (w.end / 2 + gap + w.back / 2), y: y0 };
	const back = (inside(line, w.back) && put(line, w.back)) || take(w.back, (p) => p.x < x0);
	const states = w.states.map((wide) => take(wide));
	const right = Math.max(...placed.map((p) => p.x + p.w / 2));
	const add = { x: right + gap + w.add / 2, y: y0 };
	return { back, add, end: { x: x0, y: y0 }, states };
}
