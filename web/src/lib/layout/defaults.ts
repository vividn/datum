// Today's page, as data.
//
// The day frame the wireframe (da-hrz0) drew was a CSS grid with four
// fixed areas. It is a tree now, and this is that tree: what a reader who
// has never touched the `…` menu sees.
//
// The ids are fixed rather than minted: a default is the same layout
// every time, and a stable id is what lets a test (or a saved draft from
// last week) name "the tail pane". Two defaults may share ids freely;
// ids only have to be unique within one layout.
//
// The phone default is a second tree, not a media query over the first:
// vividn wants to EXPERIMENT with mobile layouts, so the phone's starting
// point has to be as reachable and editable as the desktop's.

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
 * widget (pinned, with a quarter of yesterday showing above it,
 * da-2hd21), at whatever height the layout gives the pane, and follows it
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
	freezeToday: false,
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
 * The desktop: dayview / tail | nowview | stats / fields / commands.
 *
 * vividn, 2026-09-23 (da-ncsyn): «change the default desktop interface to be
 * more similar to the mobile interface, so commands along the bottom, next
 * row up can just be fields. In the second row, let's have tail, nowview,
 * and stats. Then dayview along the top». So it is `MOBILE_LAYOUT`'s four
 * rows with stats added to the second and the third reduced to fields
 * alone. The second row is a row of three side by side, not tabs: the
 * desktop has the width, and mobile's rows are splits for the reason he
 * gave there (da-hkjp) — so all three are visible by default. No `input`
 * pane, for the reason `MOBILE_LAYOUT` gives: the bottom bar is one.
 *
 * Every share is `legibly`'s: the strip's 85 (da-r0ydu), the row's max
 * (tail 116, nowview 122, stats 154), fields 63 and commands 63 —
 * 0.233 / 0.422 / 0.173 / 0.173. Across the row, their legible WIDTHS
 * (200, 160, 240), as mobile's fields | nowview pair is.
 */
export const DEFAULT_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'default',
	designedFor: 'desktop',
	root: holder(
		'root',
		split('main', 'column', legibly([85, 154, 63, 63]), [
			strip(),
			holder(
				'row',
				split('glance', 'row', legibly([200, 160, 240]), [
					widgetPane('tail', 'tail'),
					widgetPane('nowview', 'nowview'),
					widgetPane('stats', 'stats')
				])
			),
			widgetPane('bar', 'widgetbar'),
			widgetPane('command', 'command')
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
 * nowview / commands, one column. The tiny nowview was the snapshot
 * (`history: '0'`) — each field's current state and no time axis — and
 * went in da-2hd21, leaving tail its row alone. His «input bar» is the
 * app's own bottom bar, which every layout already has: an `input` pane
 * here would draw the same command line twice, one above the other.
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
 * ones, and nothing of yesterday showing at any of them (since da-2hd21
 * a pinned today leaves a quarter of yesterday showing: 100 / 69 / 65 /
 * 51 of today and 25 / 17 / 16 / 13 of yesterday). The commands
 * pane is the thing this must not cost, and it does not: 114 / 85 / 81 /
 * 69 px against the 63 it needs, where da-hkjp measured 122 / 91 / 87 /
 * 74. It went DOWN and still clears, because the strip's floor fell 45px
 * (130 to 85) while its share of the column rose.
 *
 * vividn, 2026-10-15 (da-2hd21): «it should definitely be always
 * scrollable. default mobile collect view, let's make the dayview at the
 * top a little narrower, maybe half the height it is right now, and yes
 * show a little bit of yesterday … get rid of the nowview on the second
 * row, so just tail for now. decrease the height of the second row, and
 * the extra height from the shortening of row 1 and 2 give to row 3». So
 * the strip's 85 was 48, about half its box at 390×844; row 2 is the
 * tail alone at its legible height in small type and its frame (103,
 * registry.ts) where nowview's 122 held it; row 3 took the 56 the two
 * gave up; the commands keep their 63, and the column's total, so their
 * pixels too. Half left the three shorter phones a strip of 52 / 49 / 40
 * px, under the shortest day it draws (40), its ruler and the seam, so
 * today was cut and no yesterday showed. vividn, 2026-10-15 (da-frewq),
 * «B»: about 70% of the old height instead, so the sliver shows on every
 * phone. MEASURED, the least share that does it at 412×563 is 70 (60, a
 * literal 70% of 85, gives that phone 53 px and no yesterday): a strip of
 * 114 / 82 / 77 / 64 px, about 80% of the old 141 / 102 / 97 / 80, drawing
 * today whole at 77 / 51 / 47 / 40 and 18 / 12 / 12 / 5 px of yesterday;
 * row 3 gives back the 22, 178 − 22 = 156. These four are NOT
 * `legibly`'s, and the sweep in tree.test.mjs leaves this layout out:
 * the strip is below its legible box by vividn's choice.
 * A pinned today leaves a quarter of yesterday over it (`dayUnderSliver`),
 * and in a box too short for both today gives way from the top, so the
 * strip scrolls into the past at every size.
 */
export const MOBILE_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'mobile',
	designedFor: 'mobile',
	root: holder(
		'root',
		split('main', 'column', legibly([70, 103, 156, 63]), [
			strip(),
			widgetPane('tail', 'tail', { type: 'small' }),
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

/**
 * A desktop tree the page was handed before it loaded, in place of the one
 * above — the web suite's, and nobody else's (da-ncsyn). The suite is
 * written against a `main` with a REPL, a custom pane and a side column,
 * so `tests/support/desk.mjs` pins THAT tree and the default can move
 * without taking two hundred tests with it; the default itself is tested
 * with the pin off. Nothing a READER can do reaches it: it is not a query,
 * not storage, not a setting — only a script that runs before the app's
 * own modules (Playwright's `addInitScript`) can put it there.
 */
const pinned = (): Layout | undefined =>
	(globalThis as { __datumDesktopLayout?: Layout }).__datumDesktopLayout;

export const defaultLayoutFor = (phone: boolean): Layout =>
	phone ? DEFAULT_LAYOUT_MOBILE : (pinned() ?? DEFAULT_LAYOUT);

// ---- the built-in layouts (da-0a5o) ------------------------------------
//
// What the root bar's `+` and a URL can name without anything saved:
// `main` — today's page, the desktop or the phone tree by the screen's
// width at the device's first load (`device`, shell.ts) — and one layout
// per widget kind that is just that widget, named as its pane is titled
// (`nowview`, `tail`, `stats`, …), so
// `datum.day/nowview` is the nowview alone. A saved layout of the same
// name shadows a built-in. And the three tabs a first visit opens
// (da-u7vz8, `fresh` in shell.ts): `welcome` and `setup`, kinds' own; and
// `collect`. vividn's
// prebuilt layouts (collect / check / analyze) are his to design: one
// object each, added to `BUILTIN` below.

/** A layout that is one widget, filling the root. */
const only = (name: string, kind: string, settings: Record<string, unknown> = {}): Layout => ({
	version: LAYOUT_VERSION,
	name,
	root: widgetPane(`${name}-only`, kind, settings)
});

/** The built-ins by name; `main` is looked up through `builtinLayout`, since it depends on the device. */
export const BUILTIN: Record<string, (phone: boolean) => Layout> = {
	main: (phone) => ({ ...defaultLayoutFor(phone), name: 'main' }),
	// The page to log on: what a phone and a desktop each opened alone until
	// da-u7vz8, `mobile`'s tree and `main`'s desktop one — by the device,
	// as `main` is (da-lsjs9).
	collect: (phone) => ({ ...(phone ? MOBILE_LAYOUT : defaultLayoutFor(false)), name: 'collect' }),
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
	sync: () => only('sync', 'sync'),
	help: () => only('help', 'help'),
	welcome: () => only('welcome', 'welcome'),
	backups: () => only('backups', 'backups'),
	conflicts: () => only('conflicts', 'conflicts'),
	setup: () => only('setup', 'setup'),
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
	widgetbar: 'fields',
	// The spec widget became the field widget (da-fu29p), the first visit's
	// `track` tab was it again, and both are the setup widget (da-rgyq8).
	spec: 'setup',
	field: 'setup',
	track: 'setup'
};

/** The built-in layout `name` names, if any; `phone` picks `main`'s tree. */
export function builtinLayout(name: string, phone: boolean): Layout | null {
	const make = BUILTIN[ALIASES[name] ?? name];
	return make ? make(phone) : null;
}

/** The built-in names, `main` first, in the order the chooser offers them. */
export const BUILTIN_NAMES = Object.keys(BUILTIN);

/**
 * Each NEW DEFAULT datum has shipped since this list began, as the sentence
 * that says what changed (da-cr0w2). A device that started on an earlier
 * one is offered the new one once, with the sentences it has not seen
 * (`offerDefaults`, shell.ts), so a change to what a first visit opens —
 * its tabs (`fresh`, shell.ts) or a tree above — that a reader should hear
 * of is one sentence APPENDED here. Never edited or removed: a device
 * remembers how many it has seen.
 */
const NEW_DEFAULTS: string[] = [
	'A first visit now opens three tabs: welcome, which says what datum is and how the page is handled; setup, where you choose what to track; and collect, the page you log on.',
	"On a phone, collect's day strip is half as tall and shows a sliver of yesterday to scroll up into, the tail has its row alone, and the fields and nowview below it have the room.",
	"On a phone, collect's day strip is a little taller again, so the sliver of yesterday shows on shorter phones too."
];

/** `NEW_DEFAULTS`, or the web suite's in its place, handed over as `pinned`'s tree is. */
export const newDefaults = (): string[] =>
	(globalThis as { __datumNewDefaults?: string[] }).__datumNewDefaults ?? NEW_DEFAULTS;

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
 * The tree `DEBUG` is shipped: the bar's knobs over the sample seeder
 * (da-6ecm5), one column.
 *
 * The knobs' subject is the BOTTOM BAR, which is on every screen and
 * needs room to travel, so the page is nothing but these two and the bar.
 * The knobs get their legible 94px and its frame (108) against the
 * seeder's 250 — its buttons, its report and a log worth scrolling.
 *
 * ⚑ NOT IN `BUILTIN`, and that is the point. It is installed as a KIT
 * layout into the DEBUG database itself (`shipDebugLayout`, shell.ts), so
 * the name `debug` means a tree THERE and nothing anywhere else. A
 * `debug` tab left in this device's open list by a visit to DEBUG is
 * therefore skipped in another database rather than opening onto a widget
 * whose only word would be that it is in the wrong place — and `settle`
 * writes the list back without it.
 */
export const DEBUG_LAYOUT_TREE: Layout = {
	version: LAYOUT_VERSION,
	name: DEBUG_LAYOUT,
	root: holder(
		'debug',
		split('debug-tools', 'column', legibly([108, 250]), [widgetPane('debug-bar', 'debug'), widgetPane('debug-seed', 'seed')])
	)
};
