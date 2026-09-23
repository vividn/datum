// Today's page, as data.
//
// The day frame the wireframe (da-hrz0) drew — strip across the top, the
// REPL beside a column of nowview, tail and the custom pane (da-wb2n),
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
import { DEFAULT_LABEL_WIDTH, HOUR_FROM, HOUR_TO } from '$lib/panes.js';

const bar = { edge: 'top', align: 'start' } as const;

const widgetPane = (id: string, kind: string, settings: Record<string, unknown> = {}): Pane => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }],
	active: 0,
	bar
});

/**
 * The strip's settings, written down rather than implied — all but the
 * DAY HEIGHT, which is written down nowhere (da-r0ydu).
 *
 * Its absence is the setting: no day height means one day fills the
 * widget, at whatever height the layout gives the pane, and follows it
 * when the pane is resized. A number here would be a height chosen at one
 * screen size for every screen size, which is the thing this replaced.
 *
 * ⚑ What used to be here was `dayHeight: DEFAULT_DAY_HEIGHT`, so every
 * tree saved before da-r0ydu carries an 85 nobody chose. `RETIRED_SETTINGS`
 * in layout/shell.ts drops it BOTH WAYS — `inline` on the way in and
 * `extract` on the way out — so an 85 cannot be read back as held and
 * cannot be saved as held either. That is where the literal lives, and why
 * it is a literal.
 */
const STRIP = {
	labelWidth: DEFAULT_LABEL_WIDTH,
	freezeToday: true,
	hourFrom: HOUR_FROM,
	hourTo: HOUR_TO
};

const strip = () => widgetPane('strip', 'daystrip', { ...STRIP });

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
 * The desktop: strip / [REPL | nowview / tail / custom] / fields.
 *
 * The strip (one SHORTEST day, 85 — da-r0ydu: the day follows the box, so
 * the strip's floor is `todayBox(MIN_DAY_HEIGHT) + 16`, plus its frame,
 * and not one day at a height written down), the middle row (the side
 * column's nowview, tail and custom at 122, 116 and 116, and their two
 * borders: 370) and the fields pane (63) share the column in that
 * proportion — 0.164 / 0.714 / 0.122. At a laptop's height that is one
 * day drawn tall rather than today with a little of yesterday above it:
 * the past is a drag of the day-height handle away, and that drag HOLDS.
 * The row is the wireframe's 2fr/1fr.
 */
export const DEFAULT_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'default',
	designedFor: 'desktop',
	root: holder(
		'root',
		split('main', 'column', legibly([85, 370, 63]), [
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
							widgetPane('custom', 'custom')
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
 * A phone: strip / REPL / [tail, nowview, custom] as tabs / fields,
 * one column — the strip, the REPL, the tabs and the fields pane at
 * their legible 85 (da-r0ydu), 85, 122 (nowview's, the tallest of the
 * three) and 63: 0.240 / 0.240 / 0.344 / 0.178.
 */
export const DEFAULT_LAYOUT_MOBILE: Layout = {
	version: LAYOUT_VERSION,
	name: 'default-mobile',
	designedFor: 'mobile',
	root: holder(
		'root',
		split('main', 'column', legibly([85, 85, 122, 63]), [
			strip(),
			widgetPane('repl', 'repl'),
			tabbed('side', ['tail', 'nowview', 'custom']),
			widgetPane('bar', 'widgetbar')
		])
	)
};

/**
 * `mobile` (vividn, 2026-09-17: "the default one loaded when the site is
 * opened on a phone"): dayview / tail | a tiny nowview / fields beside a
 * nowview / commands, one column. The tiny nowview is the snapshot
 * (`history: '0'`) — each field's current state and no time axis — and
 * tail takes most of its row. His «input bar» is the app's own bottom
 * bar, which every layout already has: an `input` pane here would draw
 * the same command line twice, one above the other.
 *
 * vividn, 2026-09-21, on what it should be instead of what da-shzs built:
 * «decrease dayview height to be just 1 day / decrease fontsize of tail a
 * little bit / let's make the fields widget and nowview widget in the
 * third row a split rather than tabs so they're both visible by default».
 * All three are here (da-hkjp), and the two that are not just a number
 * are written down below because they were MEASURED, at the four screens
 * da-shzs measured: 390×844, 390×664 (Safari with its bars), 360×640 and
 * 412×563. The acceptance is da-shzs's: every command button whole at all
 * four.
 *
 * ⚑ ROW 3 IS A ROW, NOT A COLUMN — «fields BESIDE nowview». Measured: a
 * COLUMN split of the two does not fit 412×563 at ANY day height. Stacked,
 * their legible heights SUM (63 + 122 + a border = 193) where tabs took
 * their MAX (122), which puts 71px onto the root's legible minimum; the
 * shortest day the strip may draw (`MIN_DAY_HEIGHT`, panes.ts) gives back
 * only 45, and `main` does not scroll — its scrollHeight is its clientHeight
 * at every size — so the commands pane is not pushed down, it is CUT. At
 * 412×563 it measured 457…518 against a bottom bar starting at 516, and
 * that is the acceptance test failing. Side by side the row's legible
 * height is their max again, 122, exactly as the tabs' was, so the
 * column's arithmetic is the one da-shzs measured and only the strip's
 * share has moved. Their widths fit too: 160 + 160 + a border = 328,
 * inside a 360px phone.
 *
 * ⚑ EVERY SHARE HERE IS `legibly`'s AGAIN (da-r0ydu). It was not: da-hkjp
 * weighted the strip at a measured 59 instead of its legible 130, because
 * a day was a FIXED height inside a pane that is a PROPORTION — so the
 * strip showed the same NUMBER of days at every size, «exactly one day»
 * was the ~3px window between today's band showing whole (`todayBox`) and
 * yesterday's beginning to show, and the four screens' column heights
 * differ by 1.59× (734px at 390×844 against 453px at 412×563). No ratio
 * holds a 3px window across a 1.59× spread, so the number had to be
 * measured at one of them.
 *
 * The DAY follows the WIDGET now (`dayFilling`, panes.ts), so one day
 * fits at every size by construction and there is nothing left to tune.
 * The strip's floor moved with it — the shortest day it may draw, 85 with
 * its frame, where 130 was one day at 85px — and that floor is the weight
 * here, exactly as the other three are: the row max(tail 103, nowview
 * 122), row 3 max(fields 63, nowview 122), commands 63.
 *
 * ⚑ MEASURED AFTER THE CHANGE, AT THE FOUR SIZES, so it is a decision and
 * not a surprise. The strip's inner box is 141 / 102 / 97 / 80 px and the
 * day drawn in it 126 / 87 / 82 / 65 — today's band WHOLE at all four,
 * where da-hkjp's weight clipped it from the top at the three shorter
 * ones, and nothing of yesterday showing at any of them. The commands
 * pane is the thing this must not cost, and it does not: 114 / 85 / 81 /
 * 69 px against the 63 it needs, where da-hkjp measured 122 / 91 / 87 /
 * 74. It went DOWN and still clears, because the strip's floor fell 45px
 * (130 to 85) while its share of the column rose.
 *
 * ⚑ WHAT IT COSTS is the SCROLL BACK: a strip whose box is exactly one day
 * has no room above today, so at rest it holds no past at all — on a desk
 * as much as on a phone. The past is reached by shrinking the day at the
 * `+` handle, which then HOLDS, and the pane may be resized afterwards
 * without taking it away.
 */
export const MOBILE_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'mobile',
	designedFor: 'mobile',
	root: holder(
		'root',
		split('main', 'column', legibly([85, 122, 122, 63]), [
			strip(),
			holder(
				'row',
				split('glance', 'row', [0.7, 0.3], [
					widgetPane('tail', 'tail', { type: 'small' }),
					widgetPane('now', 'nowview', { history: '0' })
				])
			),
			holder(
				'fields',
				// `legibly` of their legible WIDTHS, which is a half each
				// today — written as the rule rather than as 0.5, so it
				// follows if either kind's floor moves.
				split('pair', 'row', legibly([160, 160]), [
					widgetPane('bar', 'widgetbar'),
					widgetPane('nowview', 'nowview')
				])
			),
			widgetPane('command', 'command')
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
	mobile: () => MOBILE_LAYOUT,
	dayview: () => only('dayview', 'daystrip', { ...STRIP }),
	repl: () => only('repl', 'repl'),
	nowview: () => only('nowview', 'nowview'),
	tail: () => only('tail', 'tail'),
	custom: () => only('custom', 'custom'),
	command: () => only('command', 'command'),
	stats: () => only('stats', 'stats'),
	fields: () => only('fields', 'widgetbar'),
	aliases: () => only('aliases', 'aliases'),
	input: () => only('input', 'input')
};

/**
 * Other spellings a URL may use: the old routes (`/now`) and the widget
 * kinds whose pane title differs from their kind name.
 *
 * `input` was here for the fortnight the widget was gone (da-22vg) and is
 * not any more: the widget is back (da-3zon), so `/input` is `input`
 * again and a device that kept that tab open gets what it saved.
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

// ---- the DEBUG database (da-o2wm8) -------------------------------------
//
// vividn, 2026-09-21: "A db named DEBUG, which has a layout preloaded into
// it this DEBUG widget. It only gets loaded for the DEBUG db. how about
// that. That seems safer and also easier to access for me." So the GATE is
// which database you are in, not what you typed: a debug surface that
// cannot appear in his real database is safe by construction rather than
// by care, and it is already on screen when he arrives.

/** The one database the debug widgets are offered in. Case-sensitive. */
export const DEBUG_DB = 'DEBUG';

/** The layout that database carries, and the kit that is said to ship it. */
export const DEBUG_LAYOUT = 'debug';
export const DEBUG_KIT = 'debug';

/**
 * The tree `DEBUG` is shipped: the debug widget alone, filling the root.
 *
 * The widget's whole subject is the BOTTOM BAR, which is on every screen
 * and needs room to travel — so a screen of nothing but knobs and bar is
 * the layout this instrument wants. A second debug widget (da-6ecm5)
 * makes this a split; that is a line here, not a rewrite.
 *
 * ⚑ NOT IN `BUILTIN`, and that is the point. It is installed as a KIT
 * layout into the DEBUG database itself (`shipDebugLayout`, shell.ts), so
 * the name `debug` means a tree THERE and nothing anywhere else. A
 * `debug` tab left in this device's open list by a visit to DEBUG is
 * therefore skipped in another database rather than opening onto a widget
 * whose only word would be that it is in the wrong place — and `settle`
 * writes the list back without it.
 */
export const DEBUG_LAYOUT_TREE: Layout = only(DEBUG_LAYOUT, 'debug');
