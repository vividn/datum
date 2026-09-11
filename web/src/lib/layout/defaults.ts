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
 * The desktop: strip / [REPL | nowview / tail / command] / widget bar.
 *
 * The bar takes the height its chips need (`fit` in registry.ts); the
 * strip and the middle row share the rest a fifth to seven tenths, which
 * at a laptop's height is today's band with a little of yesterday above
 * it — a taller strip is more past (da-zb0d). The row is the wireframe's
 * 2fr/1fr.
 */
export const DEFAULT_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'default',
	designedFor: 'desktop',
	root: holder(
		'root',
		split('main', 'column', [0.2, 0.7, 0.1], [
			strip(),
			holder(
				'middle',
				split('across', 'row', [2 / 3, 1 / 3], [
					widgetPane('repl', 'repl'),
					holder(
						'side',
						split('aside', 'column', [1 / 3, 1 / 3, 1 / 3], [
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

/** A phone: strip / REPL / [tail, nowview, command] as tabs / widget bar, one column. */
export const DEFAULT_LAYOUT_MOBILE: Layout = {
	version: LAYOUT_VERSION,
	name: 'default-mobile',
	designedFor: 'mobile',
	root: holder(
		'root',
		split('main', 'column', [0.2, 0.4, 0.3, 0.1], [
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
// What the root bar's picker and a URL can name without anything saved:
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
	widgets: () => only('widgets', 'widgetbar'),
	input: () => only('input', 'input')
};

/**
 * Other spellings a URL may use: the old routes (`/now`) and the widget
 * kinds whose pane title differs from their kind name.
 */
export const ALIASES: Record<string, string> = {
	now: 'nowview',
	daystrip: 'dayview',
	widgetbar: 'widgets'
};

/** The built-in layout `name` names, if any; `phone` picks `day`'s tree. */
export function builtinLayout(name: string, phone: boolean): Layout | null {
	const make = BUILTIN[ALIASES[name] ?? name];
	return make ? make(phone) : null;
}

/** The built-in names, `day` first, in the order the picker offers them. */
export const BUILTIN_NAMES = Object.keys(BUILTIN);
