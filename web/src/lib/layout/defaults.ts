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
import { DEFAULT_LABEL_WIDTH, HOUR_FROM, HOUR_TO } from '#lib/panes.ts';

const bar = { edge: 'top', align: 'start' } as const;

const widgetPane = (id: string, kind: string, settings: Record<string, unknown> = {}): Pane => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }],
	active: 0,
	bar
});

/**
 * The strip's settings, written down rather than implied — all but the
 * DAY HEIGHT, which only the `dayview` tab writes down
 * (`DAYVIEW_TAB_DAY_HEIGHT`, da-cp8pv) and collect's strip nowhere (da-r0ydu).
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

/**
 * The day height the `dayview` TAB holds (da-cp8pv), the one tree that
 * writes one down: collect's desktop strip keeps following its box.
 *
 * vividn, 2026-11-04: «let's make the default day height much smaller,
 * maybe showing the last two weeks», then «keep pixels consistent, so
 * however many pixels gets about 12 days on a pixel 8 pro with the output
 * section open». That pane is 578px there: (578 − 15 axis) ÷ 12 ≈ 47, and
 * the same 47 on every screen. A number in the tree, so the day line still
 * moves it and a reader's own height wins over it, as any held one does.
 * Above `MIN_DAY_HEIGHT` (40), so the date stamp keeps its three lines.
 */
export const DAYVIEW_TAB_DAY_HEIGHT = 47;

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
 * The desktop: dayview over a row of tail | fields | nowview over stats.
 *
 * vividn, 2026-10-30 (da-uzi5x): «on desktop
 * [   dayview         ] 1/3
 * |tail |fields |[nowview]| 2/3
 * |tail |       |[stats]  |
 *  1/3     1/3     1/3
 * with also a separate dayview tab». The shares are his, so they are written
 * as he gave them and not through `legibly`, and the sweep in tree.test.mjs
 * leaves this tree out; layout-desktop.test.mjs measures each pane against
 * its legible box at six screens. Nowview over stats at a half each is a
 * default of the bead's: he gave them no split.
 *
 * No `input` pane: the bottom bar is one, and the room of the commands pane
 * da-mwp4m took away is the bar's output section's (`--output-height`,
 * routes/+layout.svelte). vividn, 2026-10-27: «remove the command widget on
 * the collect tab (it can remain a widget in general)».
 */
export const DEFAULT_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'default',
	designedFor: 'desktop',
	root: holder(
		'root',
		split('main', 'column', [1 / 3, 2 / 3], [
			strip(),
			holder(
				'row',
				split('glance', 'row', [1 / 3, 1 / 3, 1 / 3], [
					widgetPane('tail', 'tail'),
					widgetPane('bar', 'widgetbar'),
					holder(
						'now',
						split('now-stats', 'column', [1 / 2, 1 / 2], [widgetPane('nowview', 'nowview'), widgetPane('stats', 'stats')])
					)
				])
			)
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
 * A phone: strip / CLI / [tail, nowview, custom] as tabs / fields,
 * one column — the strip, the CLI, the tabs and the fields pane at
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
			// The id is the REPL's still (da-mwp4m renamed the kind): an id is
			// what a draft saved before the rename names this pane by.
			widgetPane('repl', 'cli'),
			tabbed('side', ['tail', 'nowview', 'custom']),
			widgetPane('bar', 'widgetbar')
		])
	)
};

/**
 * `mobile` (vividn, 2026-09-17: "the default one loaded when the site is
 * opened on a phone"): tail / nowview / fields, one column, each the full
 * width. His «input bar» is the app's own bottom bar, which every layout
 * already has: an `input` pane here would draw the same command line twice,
 * one above the other.
 *
 * vividn, 2026-10-30 (da-uzi5x): «let's redesign the collect layout too
 * on mobile:
 * [tail] 1/3
 * [nowview] 1/4
 * [fields] 5/12
 * then have a dayview tab be a whole tab». So the day strip is the `dayview`
 * tab's now, beside this one. The shares are his, so they are written as he
 * gave them and not through `legibly`, and the sweep in tree.test.mjs leaves
 * this layout out. The tail keeps its smaller type (da-hkjp: «decrease
 * fontsize of tail a little bit»).
 *
 * vividn, 2026-10-27: «remove the command widget on the collect tab (it can
 * remain a widget in general)», and «make the output section open by default
 * … take up a bit more room, basically where the command widget is now by
 * default». So the bar's output section, above the input, has that room
 * (`--output-height`), and the three rows share what is left.
 */
export const MOBILE_LAYOUT: Layout = {
	version: LAYOUT_VERSION,
	name: 'mobile',
	designedFor: 'mobile',
	root: holder(
		'root',
		split('main', 'column', [1 / 3, 1 / 4, 5 / 12], [
			widgetPane('tail', 'tail', { type: 'small' }),
			widgetPane('nowview', 'nowview'),
			widgetPane('bar', 'widgetbar')
		])
	)
};

/** The phone breakpoint the old grid used (da-hkpd), kept as the one number. */
export const PHONE_WIDTH = 720;

/**
 * A desktop tree the page was handed before it loaded, in place of the one
 * above — the web suite's, and nobody else's (da-ncsyn). The suite is
 * written against a `main` with a CLI, a custom pane and a side column,
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
// name shadows a built-in. And the tabs a first visit opens (da-u7vz8,
// `fresh` in shell.ts): `welcome` and `setup`, kinds' own; `collect`; and
// `dayview`, the day strip alone (da-uzi5x). vividn's
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
	dayview: () => only('dayview', 'daystrip', { ...STRIP, dayHeight: DAYVIEW_TAB_DAY_HEIGHT }),
	cli: () => only('cli', 'cli'),
	nowview: () => only('nowview', 'nowview'),
	tail: () => only('tail', 'tail'),
	custom: () => only('custom', 'custom'),
	command: () => only('command', 'command'),
	stats: () => only('stats', 'stats'),
	chart: () => only('chart', 'chart'),
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
	// The CLI widget was the REPL until da-mwp4m; `/repl` and `/cli` are both it.
	repl: 'cli',
	daystrip: 'dayview',
	// The collect widget alone (da-w6437; the fields widget until then).
	// `/fields` is its old name, kept as its route for now: it will belong
	// to the fields widget vividn plans (every field alphabetically, the
	// ignored at the bottom) when that lands, and this one moves.
	widgets: 'fields',
	widgetbar: 'fields',
	// The spec widget became the field widget (da-fu29p), the first visit's
	// `track` tab was it again, and both are the setup widget (da-rgyq8).
	spec: 'setup',
	field: 'setup',
	track: 'setup',
	// The tutorial's first tab was `introduction` until da-uzyih.4898 (vividn: «make the name of the tab intro»).
	introduction: 'intro'
};

/**
 * What a built-in layout is CALLED where it is not its name (da-jzica): the
 * root tab, the `+`. `collect` is the widget's title, so the page of that
 * name reads «collection» (vividn, 2026-11-03: «let's name the layout
 * collection, and the widget collect»). The name stays its identity, as the
 * widget kind stayed `widgetbar`, so nothing saved or open migrates; a
 * layout saved under another name is called that.
 */
export const LAYOUT_TITLES: Record<string, string> = { collect: 'collection' };

/** What the layout `name` is called on the page (`LAYOUT_TITLES`). */
export const layoutTitle = (name: string): string => LAYOUT_TITLES[name] ?? name;

/** The built-in layout `name` names, if any; `phone` picks `main`'s tree. */
export function builtinLayout(name: string, phone: boolean): Layout | null {
	const make = BUILTIN[ALIASES[name] ?? name];
	return make ? make(phone) : null;
}

/** The built-in names, `main` first, in the order the chooser offers them. */
export const BUILTIN_NAMES = Object.keys(BUILTIN);

/**
 * Each NEW DEFAULT datum has shipped since this list began (da-cr0w2): the
 * sentence that says what changed, and the shipped tabs it changed (da-3eegv).
 * A device that started on an earlier one takes each new one tab by tab
 * (`offerDefaults`, shell.ts), and is asked only about a tab it changed, with
 * that tab's sentences. So a change to what a first visit opens — its tabs
 * (`fresh`, shell.ts) or a tree above — that a reader should hear of is one
 * entry APPENDED here. Never edited or removed: a device remembers how many it
 * has seen.
 */
export interface NewDefault {
	tabs: string[];
	text: string;
}

const NEW_DEFAULTS: NewDefault[] = [
	{
		tabs: ['welcome', 'setup', 'collect'],
		text: 'A first visit now opens three tabs: welcome, which says what datum is and how the page is handled; setup, where you choose what to track; and collect, the page you log on.'
	},
	{
		tabs: ['collect'],
		text: "On a phone, collect's day strip is half as tall and shows a sliver of yesterday to scroll up into, the tail has its row alone, and the fields and nowview below it have the room."
	},
	{
		tabs: ['collect'],
		text: "On a phone, collect's day strip is a little taller again, so the sliver of yesterday shows on shorter phones too."
	},
	{
		tabs: ['collect'],
		text: "collect has no command pane now: the command line's output stands open above it, where the commands were. The commands are still a widget to add."
	},
	{
		tabs: ['collect', 'dayview'],
		text: "collect is laid out anew. On a phone: the tail, the nowview and the fields, one under the other, and no day strip. On a desktop: the day strip over the tail, the fields, and the nowview over the stats. And a new tab, dayview, is the day strip alone."
	},
	{
		tabs: ['collect', 'setup', 'dayview'],
		text: 'The commands are back at the bottom of collect, setup and dayview, the same pane on each: round buttons, and the command your pills make ready leading them, larger.'
	},
	{
		tabs: ['collect', 'setup', 'dayview'],
		text: 'The command pane at the bottom of collect, setup and dayview is gone again: the commands open from the ❯ on the command line, the command your pills make ready leading them.'
	},
	{
		tabs: ['dayview'],
		text: 'The dayview tab draws its days smaller, the same size on every screen: about the last two weeks on a phone.'
	}
];

/** `NEW_DEFAULTS`, or the web suite's in its place, handed over as `pinned`'s tree is. */
export const newDefaults = (): NewDefault[] =>
	(globalThis as { __datumNewDefaults?: NewDefault[] }).__datumNewDefaults ?? NEW_DEFAULTS;

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
 * layout into the DEBUG database itself (`shipLayouts`, shell.ts), so
 * the name `debug` means a tree THERE and nothing anywhere else: a
 * `debug` tab in another database's list is skipped rather than opening
 * onto a widget whose only word would be that it is in the wrong place.
 */
export const DEBUG_LAYOUT_TREE: Layout = {
	version: LAYOUT_VERSION,
	name: DEBUG_LAYOUT,
	root: holder(
		'debug',
		split('debug-tools', 'column', legibly([108, 250]), [widgetPane('debug-bar', 'debug'), widgetPane('debug-seed', 'seed')])
	)
};

// ---- the tutorial database (da-6cvrd) ----------------------------------
//
// Like DEBUG, a database whose name is the gate: its tabs are KIT layouts
// installed into it (`shipLayouts`, shell.ts), so they mean nothing in any
// other, and the `tutorial` widget is offered there alone. What is done in
// it — the kit's install, a game's Reset, the way in and out — is
// `#lib/tutorial.ts`.

/** The database the tutorial is. */
export const TUTORIAL_DB = 'tutorial';

/** One tab of the tutorial: its layout's name, its title, and its fields. */
export interface Lesson {
	name: string;
	title: string;
	/** The fields its game's Reset ends the active states of: the lesson's own, by name (`coffee` …). */
	fields: string[];
}

/**
 * The lessons in order, which is the tabs' order: the Introduction, the three
 * that collect data (TU2 da-nl1co, TU3 da-rru2t, TU4 da-6xeqy), then pages of
 * text with no fields, what a user needs only once they log (da-uzyih.4869).
 * The field shapes were a tab of their own after the Introduction (da-uzyih.4898)
 * until the tutorial went to playing first (da-uzyih.4908): their words are
 * `docs/basics.md`, and each game says what its shape is.
 * Each game's fields are the kit's (`kits/tutorial.toml`), the ones its
 * scene logs (da-uzyih.4865, .4866, .4867).
 */
export const LESSONS: Lesson[] = [
	{ name: 'intro', title: 'Introduction', fields: [] },
	{
		name: 'moment',
		title: 'moment: record points in time when something happens',
		fields: ['croissant', 'cake', 'bagel', 'coffee', 'tea']
	},
	{
		name: 'on-off',
		title: 'on/off: record when and how long things are happening',
		fields: ['tv', 'phone', 'reading']
	},
	{
		name: 'switch',
		title: 'switch: record how the state of something changes over time',
		fields: ['room']
	},
	{ name: 'input-bar', title: 'The input bar', fields: [] },
	{ name: 'tabs', title: 'Tabs and widgets', fields: [] }
];

/**
 * A game under a two-minute nowview, where its taps land as they are made:
 * what is watched while tapping, right above what is tapped (vividn, 2026-10-22).
 * Over both, its title and what its shape of data is, read before playing;
 * the game and its fields are panes of their own (da-uzyih.4908, vividn,
 * 2026-10-29: «Maybe 1/3 for the description, 1/6 for nowview, … 1/4 for game
 * and 1/4 for field widget. Let's have the game and the fields widgets be
 * separate widgets here»).
 */
const withNowview = (name: string): Layout => ({
	version: LAYOUT_VERSION,
	name,
	root: holder(
		name,
		split(`${name}-split`, 'column', [1 / 3, 1 / 6, 1 / 4, 1 / 4], [
			widgetPane(`${name}-about`, 'tutorial', { lesson: name, part: 'about' }),
			widgetPane(`${name}-now`, 'nowview', { history: '2m', orientation: 'horizontal' }),
			widgetPane(`${name}-lesson`, 'tutorial', { lesson: name }),
			widgetPane(`${name}-fields`, 'tutorial', { lesson: name, part: 'fields' })
		])
	)
});

/** Each lesson's tab: the `tutorial` widget on its page, alone but for the games, four panes (the moment lesson's, da-nl1co, «On and off»'s, da-rru2t, and the switch lesson's, da-6xeqy). */
export const TUTORIAL_LAYOUTS: Layout[] = LESSONS.map(({ name }) =>
	name === 'moment' || name === 'on-off' || name === 'switch' ? withNowview(name) : only(name, 'tutorial', { lesson: name })
);
