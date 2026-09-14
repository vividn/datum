// Today's page, as data.
//
// The day frame the wireframe (da-hrz0) drew — strip across the top, the
// REPL beside a column of nowview, tail and the command pane (da-wb2n),
// the widget bar along the bottom — was a CSS grid with four fixed areas. It is a tree now, and this is
// that tree: what a reader who has never touched the `…` menu sees.
//
// The ids are fixed rather than minted: a default is the same layout
// every time, and a stable id is what lets a test (or a saved draft from
// last week) name "the tail pane". Two defaults may share ids freely;
// ids only have to be unique within one layout.
//
// The phone default is a second tree, not a media query over the first:
// vividn wants to EXPERIMENT with mobile layouts, so the phone's starting
// point has to be as reachable and editable as the desktop's. It is the
// same widgets stacked in one column, with tail and nowview as two tabs
// of one pane — a pane too short to show both is better spent showing one
// whole.

import type { Layout, Pane, Split, Tab } from './types.js';
import { LAYOUT_VERSION } from './types.js';
import { DEFAULT_DAY_HEIGHT } from '$lib/panes.js';

const bar = { edge: 'top', align: 'start' } as const;

const widgetPane = (id: string, kind: string, settings: Record<string, unknown> = {}): Pane => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }],
	active: 0,
	bar
});

/** The strip, with its one setting written down rather than implied. */
const strip = () => widgetPane('strip', 'daystrip', { dayHeight: DEFAULT_DAY_HEIGHT });

const holder = (id: string, split: Split): Pane => ({
	id,
	tabs: [{ id: `${id}-tab`, content: split }],
	active: 0,
	bar
});

const split = (
	id: string,
	direction: Split['direction'],
	ratios: number[],
	children: Pane[]
): Split => ({ id, direction, ratios, children });

/**
 * Ratios in proportion to the panes' legible heights (registry.ts's
 * `legible`, each plus its 14px frame: the outline and the inside half
 * of the title's row, Layout.svelte's `chromeOf`). The kinds have no
 * floor to hold a pane up any more (da-zh2o), so the ratios do: a screen
 * at the root's minimum draws every pane exactly legible, and a taller
 * one grows them alike. Every child of the column is in the sum — the
 * fields pane included, now that no pane sizes itself (da-mbz2).
 */
const legibly = (heights: number[]): number[] =>
	heights.map((h) => h / heights.reduce((a, b) => a + b, 0));

/**
 * The desktop: strip / [REPL | nowview / tail / command] / fields.
 *
 * The strip (one day, 134), the middle row (the side column's nowview,
 * tail and command at 122, 116 and 116, and their two borders: 370) and
 * the fields pane (63) share the column in that proportion — 0.236 /
 * 0.653 / 0.111, which at a laptop's height is today's band with a
 * little of yesterday above it (a taller strip is more past, da-zb0d)
 * and two rows of buttons. The row is the wireframe's 2fr/1fr.
 */
export const DEFAULT_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'default',
	designedFor: 'desktop',
	root: holder(
		'root',
		split('main', 'column', legibly([134, 370, 63]), [
			strip(),
			holder(
				'middle',
				split('across', 'row', [2 / 3, 1 / 3], [
					widgetPane('repl', 'repl'),
					holder(
						'side',
						split('aside', 'column', legibly([122, 116, 116]), [
							widgetPane('nowview', 'nowview'),
							widgetPane('tail', 'tail'),
							widgetPane('command', 'command')
						])
					)
				])
			),
			widgetPane('bar', 'widgetbar')
		])
	)
};

const tabbed = (id: string, kinds: string[]): Pane => ({
	id,
	tabs: kinds.map(
		(kind): Tab => ({ id: `${id}-${kind}-tab`, content: { id: `${id}-${kind}`, kind, settings: {} } })
	),
	active: 0,
	bar
});

/**
 * A phone: strip / REPL / [tail, nowview, command] as tabs / fields,
 * one column — the strip, the REPL, the tabs and the fields pane at
 * their legible 134, 85, 122 (nowview's, the tallest of the three) and
 * 63: 0.332 / 0.210 / 0.302 / 0.156.
 */
export const DEFAULT_LAYOUT_MOBILE: Layout = {
	version: LAYOUT_VERSION,
	name: 'default-mobile',
	designedFor: 'mobile',
	root: holder(
		'root',
		split('main', 'column', legibly([134, 85, 122, 63]), [
			strip(),
			widgetPane('repl', 'repl'),
			tabbed('side', ['tail', 'nowview', 'command']),
			widgetPane('bar', 'widgetbar')
		])
	)
};

/** The phone breakpoint the old grid used (da-hkpd), kept as the one number. */
export const PHONE_WIDTH = 720;

export const defaultLayoutFor = (phone: boolean): Layout =>
	phone ? DEFAULT_LAYOUT_MOBILE : DEFAULT_LAYOUT;

// ---- the built-in layouts (da-0a5o) ------------------------------------
//
// What the root bar's `+` and a URL can name without anything saved:
// `day` — today's page, the desktop or the phone tree by the screen's
// width at first load — and one layout per widget kind that is just that
// widget, named as its pane is titled (`nowview`, `tail`, `stats`, …), so
// `datum.day/nowview` is the nowview alone. A saved layout of the same
// name shadows a built-in. vividn's prebuilt layouts (collect / check /
// analyze) are his to design: one object each, added to `BUILTIN` below.

/** A layout that is one widget, filling the root. */
const only = (name: string, kind: string, settings: Record<string, unknown> = {}): Layout => ({
	version: LAYOUT_VERSION,
	name,
	root: widgetPane(`${name}-only`, kind, settings)
});

/** The built-ins by name; `day` is looked up through `builtinLayout`, since it depends on the screen. */
export const BUILTIN: Record<string, (phone: boolean) => Layout> = {
	day: (phone) => ({ ...defaultLayoutFor(phone), name: 'day' }),
	dayview: () => only('dayview', 'daystrip', { dayHeight: DEFAULT_DAY_HEIGHT }),
	repl: () => only('repl', 'repl'),
	nowview: () => only('nowview', 'nowview'),
	tail: () => only('tail', 'tail'),
	command: () => only('command', 'command'),
	stats: () => only('stats', 'stats'),
	fields: () => only('fields', 'widgetbar'),
	aliases: () => only('aliases', 'aliases'),
	input: () => only('input', 'input')
};

/**
 * Other spellings a URL may use: the old routes (`/now`) and the widget
 * kinds whose pane title differs from their kind name.
 */
export const ALIASES: Record<string, string> = {
	now: 'nowview',
	daystrip: 'dayview',
	widgets: 'fields',
	widgetbar: 'fields'
};

/** The built-in layout `name` names, if any; `phone` picks `day`'s tree. */
export function builtinLayout(name: string, phone: boolean): Layout | null {
	const make = BUILTIN[ALIASES[name] ?? name];
	return make ? make(phone) : null;
}

/** The built-in names, `day` first, in the order the chooser offers them. */
export const BUILTIN_NAMES = Object.keys(BUILTIN);
