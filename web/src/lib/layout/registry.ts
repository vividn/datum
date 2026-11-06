// The widget kinds: what the tree can name, and what draws each one.
//
// Everything on the page is a widget (vividn: "They are widgets like
// anything else yes") — the day strip, the CLI and the collect widget
// included. A kind is its `WidgetKindMeta` (the contract in types.ts:
// title and default settings) plus the Svelte component that draws
// it and, where the component wants something other than `settings`, the
// props to hand it. The chooser lists `pickableIn(db)` — every kind but
// the ones another database gates (`db` below); the tree's empty tab
// (`EMPTY_KIND`, tree.ts) is not a kind here — Layout.svelte draws the
// button that opens the chooser for it.
//
// A kind has no floor (vividn's third look, da-zh2o: "the hard limits kind
// of interferes with setting stuff up"): a pane shrinks until its content
// box is gone and only its frame is left. What a kind may declare instead
// is SNAPS — content-box pixels a border drag is drawn to when it passes
// within a few pixels of one, and carries on past. The strip snaps at one
// whole day: today's band with its axis plus the body's padding.
//
// What the PAGE keeps is `legible`, content-box pixels measured on the
// built app (2026-09-10, Chromium, 0.85rem monospace): the pane's body at
// the size below which the kind stops being legible. The strip: one whole
// day at its drawn height (`todayBox(85)`, 100px at the default) plus the
// body's 16px of padding — its snap. The CLI: three lines of scrollback
// (18.4px each) plus padding, 71px — the bottom bar is its prompt
// (da-rfs9), so it has no line of its own to keep. The tail,
// and a custom pane showing one: its header row and three rows (21.6px
// each) plus padding, 102px — and the tail's follows its `type` option
// (da-hkjp), the 16px of padding standing still while the text in it
// scales. The collect widget: one row, as tall as the «Track +» it began
// with (33px; a field's button is no taller, da-y6jfb), plus padding. The aliases list: three of its lines
// (18.4px each) plus padding, 72px. The input widget: its prompt row
// (36.4px) and one document line of receipt (21.6px), 58px. nowview: datum draws it no shorter than a 50px band plus its
// axis, 92px of picture (da-wb2n), plus padding. Widths are where each
// stops making sense rather than where it stops rendering: an hour of the
// strip per 10px, a CLI line, the tail's four columns, four chips. The
// frame's own chrome is added by the renderer, not counted. One reader
// only: the root's minimum (Layout.svelte), so a screen too small for
// every pane scrolls the page rather than squeezing them — a drag never
// sees these numbers.

import type { Component } from 'svelte';
import type { Tab, WidgetKindMeta, WidgetOption } from './types.js';
import { nameOf, type Floor } from './tree.js';
import CliPane from '#lib/CliPane.svelte';
import CommandPane from './CommandPane.svelte';
import CustomPane from '#lib/CustomPane.svelte';
import DebugPane from '#lib/DebugPane.svelte';
import Stats from '#lib/Stats.svelte';
import {
	DEFAULT_LABEL_WIDTH,
	HOUR_FROM,
	HOUR_TO,
	MAX_DAY_HEIGHT,
	MIN_DAY_HEIGHT,
	NOWVIEW_HISTORY,
	NOWVIEW_STEPS,
	TAIL,
	todayBox
} from '#lib/panes.ts';
import { REACH_PX } from '#lib/lanes.ts';
import { DEBUG_DB, layoutTitle, TUTORIAL_DB } from './defaults.js';
import { DEFAULT_STATS, type StatsRange } from '#lib/stats.ts';
import { BUCKETS, DEFAULT_CHART, FIELDS_OFFER, RANGES, SHOWS, chartSettings, keysOffer } from '#lib/chart.ts';
import AliasesWidget from './AliasesWidget.svelte';
import ChartWidget from './ChartWidget.svelte';
import BackupsWidget from './BackupsWidget.svelte';
import ConflictsWidget from './ConflictsWidget.svelte';
import DayviewWidget from './DayviewWidget.svelte';
import CollectPane from './CollectPane.svelte';
import DocsWidget from './DocsWidget.svelte';
import { COLLECT_FIELDS_OFFER } from '#lib/fields.ts';
import HelpWidget from './HelpWidget.svelte';
import InputWidget from './InputWidget.svelte';
import Notice from './Notice.svelte';
import NowviewWidget from './NowviewWidget.svelte';
import CliWidget from './CliWidget.svelte';
import FieldWidget from './FieldWidget.svelte';
import KitWidget from './KitWidget.svelte';
import PremadeWidget from './PremadeWidget.svelte';
import SettingsWidget from './SettingsWidget.svelte';
import SyncWidget from './SyncWidget.svelte';
import TutorialWidget from './TutorialWidget.svelte';
import GameWidget from './GameWidget.svelte';
import WelcomeWidget from './WelcomeWidget.svelte';

/** What a tour's step invites the person to do on its element (#lib/tour.ts). */
export type Invite = 'tap' | 'hold' | 'drag' | 'scroll' | 'type';

/**
 * A step of a kind's tour: `at` inside its widget, `WHOLE` the widget itself,
 * or, `with` a kind, inside that widget where one is drawn on the page too, and
 * left out where none is.
 */
export interface Step {
	at: string;
	say: string;
	do?: Invite;
	with?: string;
}

/** A tour step's widget itself: its pane's section under the host. */
export const WHOLE = ':scope > section';

export interface WidgetKind {
	meta: WidgetKindMeta;
	component: Component<any>;
	/** Props for the component, when it wants more than `settings`. */
	props?: (settings: Record<string, unknown>) => Record<string, unknown>;
	/**
	 * What the component's `onchange` means for its settings: the
	 * argument folded into them. A kind without one has nothing to keep.
	 *
	 * It is given the settings as SHOWN and what it returns lands in THIS
	 * SESSION (da-vhwq, Layout.svelte), not in the tree — so a widget
	 * moving one of its own settings cannot modify the layout, in either
	 * mode.
	 */
	onChange?: (settings: Record<string, unknown>, moved: unknown) => Record<string, unknown>;
	/**
	 * How to use it (da-oefd0), in Markdown, the person's words and not the
	 * CLI's: what it is for, then each of its features — what you tap, hold,
	 * drag or pinch and what happens, the lines of its menu, every one of
	 * its options by its label. A feature that runs a command says what it
	 * does first and names the command after. vividn, 2026-11-05: «Each
	 * widget should carry with it in the code how to use it, and document
	 * all of its features». The Help button on the widget opens it, and the
	 * docs widget lists it under the contents (#lib/guides.ts); required, and
	 * `guides-pure.test.mjs` holds every option to being named in it.
	 */
	guide: string;
	/** The `datum help` page its guide folds at its end, for a terminal. */
	commands?: string;
	/**
	 * Its tour (da-7dflk, #lib/tour.ts): steps on its parts, each a sentence,
	 * one that invites (`do`) going on once the person has done it there, and
	 * steps `with` a kind drawn beside it on how the two work together. The
	 * runner ends it on the widget whole, with «read more», this guide.
	 * `widget-tour.test.mjs` runs every kind's to its end.
	 */
	tour: Step[];
	/** The pane's title from the settings, where it is not the kind's own. */
	title?: (settings: Record<string, unknown>) => string | undefined;
	/** Content-box sizes a border drag snaps to, from the settings: see the note above. */
	snap?: (settings: Record<string, unknown>) => { width?: number[]; height?: number[] };
	/** Not offered by the chooser: a kind the app puts in a tab itself. */
	hidden?: boolean;
	/**
	 * Drawn from the engine's stores and asking the CLI nothing itself
	 * (da-ch6oc): mounted from the first paint, with no copy of the last
	 * visit laid over it (Layout.svelte), so it is never dimmed waiting on
	 * the CLI's queue.
	 */
	live?: boolean;
	/**
	 * Offered ONLY when the open database is this one (da-o2wm8).
	 *
	 * vividn's own reason for the DEBUG widget: "The DEBUG widget is only
	 * available from the DEBUG db". The gate is which database you are
	 * in, not what you typed — safe by construction rather than by care.
	 * A gated kind is left out of `PICKABLE` as well; see the note there.
	 */
	db?: string;
	/** The content box the kind is legible in, fixed or from the settings: see the note above. */
	legible: Floor | ((settings: Record<string, unknown>) => Floor);
}

/**
 * The outer margin `datum nowview` insets its bands by, each side — the
 * CLI's own `--margin` default, and what a pane's `--width` loses to the
 * canvas before any band is drawn.
 */
const SVG_MARGIN_PX = 2;
/** The collect widget's floor, and a tutorial game's fields part's, which draws it. */
const FIELDS_LEGIBLE: Floor = { width: 160, height: 49 };

/**
 * How narrow nowview's snapshot (`showHistory: false`) stays usable.
 *
 * It was 90, for reading: «at a 92px body each chip still reads about
 * eleven characters — mood: happ, a field and the head of its state»
 * (da-shzs). da-7q8r took the chips away — they are the CLI's `--labels`
 * now, which nothing here passes — so there is nothing left to READ. The
 * snapshot is one full-width block per field, and along a block nothing
 * varies: a press at its left end means what a press at its right end
 * means. The floor is therefore what a labelless block needs to be SEEN
 * and AIMED AT, not read.
 *
 * Aiming is the larger of the two, and `#lib/lanes.ts` already has its
 * number: `REACH_PX` is «a fingertip's half-width», and `NOW_PX` in
 * #lib/nowpoint.ts is the same 24 for the same reason. A block one whole
 * fingertip across is a target a finger can land on; narrower and the
 * press is aimed at something smaller than the thing pressing it. The
 * pane hands its content box to the CLI as `--width` (`wantsBox`,
 * #lib/panes.ts) and the CLI insets by `SVG_MARGIN_PX` each side, so the
 * box must carry the fingertip plus both margins.
 *
 * Which is 52 — well under the 84–99px the shipped `mobile` row gives a
 * phone. Deliberately not derived from that ratio: at 360×640 it measured
 * 84 against the old floor of 90, so the row was already missing the
 * floor it declared. A number that shipped is not thereby a number that
 * worked.
 */
const SNAPSHOT_WIDTH = 2 * REACH_PX + 2 * SVG_MARGIN_PX;

/**
 * The pivot (da-a6guc, da-78evn): time down the page, as a calendar draws
 * it. ONE option for both drawn views, so the dayview and the nowview
 * cannot come to disagree about what it is called or what it offers.
 */
const ORIENTATION: WidgetOption = {
	key: 'orientation',
	label: 'orientation',
	type: 'choice',
	choices: ['horizontal', 'vertical'].map((value) => ({ value, label: value }))
};

/** The strip body's padding, top and bottom, as its snap counts it. */
const STRIP_PADDING = 16;

/**
 * The tail's type, by name, as a scale on the pane body's own 0.85rem
 * (da-hkjp — vividn: "decrease fontsize of tail a little bit").
 *
 * A DECLARED option of the kind rather than a size written into a layout:
 * the size a layout asks for has to be reachable from the widget's name
 * (da-vhwq), and a number hardcoded in `defaults.ts` is reachable from
 * nowhere. What it sets is `Pane`'s `scale`, since the 0.85rem is one rule
 * over every pane's body and not the tail's to override from inside.
 */
const TAIL_TYPE: Record<string, number> = { normal: 1, small: 0.85 };

const tailScale = ({ type }: Record<string, unknown>): number =>
	TAIL_TYPE[typeof type === 'string' ? type : 'normal'] ?? 1;

/**
 * One whole day at the height the strip's days are DRAWN — today's band
 * with its axis, the setting clamped as the strip clamps it — plus the
 * body's padding.
 *
 * NO day height is the strip FOLLOWING ITS BOX (da-r0ydu), and then the
 * shortest day it may draw is the honest floor: whatever box it is given
 * above that holds exactly one day, because the day is derived from the
 * box. Reading `DEFAULT_DAY_HEIGHT` here instead would claim 116px for a
 * widget that stays legible at 71 — and `legibly` in defaults.ts turns
 * that claim straight into the share of the column the strip takes, which
 * is what the number has to be true about.
 */
const oneDay = ({ dayHeight }: Record<string, unknown>): number => {
	const asked = typeof dayHeight === 'number' ? dayHeight : MIN_DAY_HEIGHT;
	return todayBox(Math.min(MAX_DAY_HEIGHT, Math.max(MIN_DAY_HEIGHT, asked))) + STRIP_PADDING;
};

const meta = (
	kind: string,
	title: string,
	defaultSettings: Record<string, unknown> = {},
	options?: WidgetOption[]
): WidgetKindMeta => ({ kind, title, defaultSettings, ...(options ? { options } : {}) });

/** A widget of the DEBUG database: `db`-gated, and named as it is titled. */
const debugKind = (kind: 'debug' | 'seed', legible: Floor, guide: string): WidgetKind => ({
	meta: meta(kind, kind),
	guide,
	tour: [{ at: WHOLE, say: kind === 'debug' ? "The DEBUG database's knobs, to tune the bar's glide." : 'A week of sample entries, logged and cleared in a tap.' }],
	component: DebugPane,
	props: () => ({ kind }),
	db: DEBUG_DB,
	legible
});

export const KINDS: Record<string, WidgetKind> = {
	// The strip fills its box and shows as much of the past as fits above
	// today (da-zb0d). Its settings are what the reader has moved on it:
	// the day height (the day line) and the date column (the spine), each
	// dragged or moved as the keyboard's slider, whether today is frozen
	// at the bottom, and the hour range it is zoomed onto — so a saved
	// layout keeps a zoomed strip zoomed, and `Reset widget` is the way
	// back to the whole day (da-ukcf).
	daystrip: {
		// No `dayHeight` among the defaults: its default is DERIVED from the
		// pane's height (da-r0ydu), and a declared one would be what a
		// layout falls back to and what `Reset widget` restores — both of
		// which must be the follow.
		meta: meta('daystrip', 'dayview', {
			labelWidth: DEFAULT_LABEL_WIDTH,
			freezeToday: false,
			hourFrom: HOUR_FROM,
			hourTo: HOUR_TO,
			orientation: 'horizontal'
		}, [
			{ key: 'freezeToday', label: 'today frozen at the bottom', glyph: '⤓', type: 'boolean' },
			// A vertical dayview is a week of columns.
			ORIENTATION
		]),
		component: DayviewWidget,
		guide: `Your days as bands of color, a field a lane: today at the bottom, the days before it above, scrolled back as far as your entries go. Hover an entry for its time and what it is.

## A tap, a hold

Nothing here logs anything: what you tap becomes a pill on the bar, for the next command.

- Tap an entry to select it (an id pill); tap it again to let it go, and ctrl or ⌘ to add or take one from the selection.
- Tap the ground between entries, or right-click, for that time: a \`-t\` pill, so the next entry is logged then. Hold still, a ring closing over the wait, for the time even over an entry. Double tap for the time and keep the selection.
- Hold, then drag sideways, for a stretch of that day (shift and a drag needs no hold). Press or drag along the hour ruler at the foot to scrub a time.

## Moving around

Drag up and down, or the wheel, to scroll back through the days. Pinch sideways, or ctrl and the wheel, to zoom onto hours; drag sideways, or shift and the wheel, to pan them. The magnifier with a 0 in it, while zoomed, brings back the whole day. Pinch up and down to make each day taller or shorter, or drag a day's line where it crosses the dates; drag the line beside the dates to widen them (or the arrow keys on either). Scrolled up, the jagged line under the days brings back today.

## Its options

- **today frozen at the bottom**: today stays at the foot while the days above it scroll.
- **orientation**: **horizontal**, or **vertical**, a week of columns, time down the page.

The strip keeps the hours and sizes you move it to, until **Reset widget**.`,
		props: ({ dayHeight, labelWidth, freezeToday, hourFrom, hourTo, orientation }) => ({
			orientation: orientation === 'vertical' ? 'vertical' : 'horizontal',
			// Undefined on purpose: the strip derives its day from its own
			// box while nothing is set (da-r0ydu), and a default here would
			// be a number nobody chose standing in the way of that.
			dayHeight: typeof dayHeight === 'number' ? dayHeight : undefined,
			labelWidth: typeof labelWidth === 'number' ? labelWidth : DEFAULT_LABEL_WIDTH,
			freezeToday: typeof freezeToday === 'boolean' ? freezeToday : false,
			hourFrom: typeof hourFrom === 'number' ? hourFrom : HOUR_FROM,
			hourTo: typeof hourTo === 'number' ? hourTo : HOUR_TO
		}),
		// The strip's `onchange` names the settings it moved — a resize names
		// the day height and the label width, a zoom both ends of its hour
		// range — so the fold is the same one for all of them.
		onChange: (settings, moved) => ({ ...settings, ...(moved as Record<string, unknown>) }),
		// A notch, not a wall: shorter, the strip clips today's band from
		// the top and keeps its axis. Only while the reader HOLDS a day
		// height — following its box (da-r0ydu) every height is one whole
		// day, so there is no one height to be drawn to.
		snap: (settings) =>
			typeof settings.dayHeight === 'number' ? { height: [oneDay(settings)] } : {},
		tour: [
			{ at: WHOLE, say: 'Your days as bands of color, a field a lane: today at the bottom, the days before it above.' },
			{ at: '.row.now .band', say: "Tap the ground between entries in today's band: that time becomes a pill on the bar, and the next entry is logged then. Tap an entry to select it.", do: 'tap' },
			{ at: '.row.now .band', say: 'Hold still for that time even over an entry: a ring closes over the wait.', do: 'hold' },
			{ at: '.stack', say: 'Scroll up, a drag or the wheel, for the days before.', do: 'scroll' },
			{ at: '.axis', say: 'The hour ruler: press or drag along it to scrub a time. Pinch, or ctrl and the wheel, zooms onto hours.' },
			{ at: WHOLE, say: 'The tail follows what you choose here: an entry selected is scrolled to, and a time with a dotted line at it.', with: 'tail' }
		],
		legible: (settings) => ({ width: 240, height: oneDay(settings) })
	},
	// The REPL until da-mwp4m (vividn, 2026-10-27: «let's rename the REPL to
	// be CLI»); a layout naming `repl` opens as this (`RETIRED_KINDS`,
	// shell.ts). `reversed` (da-gvdc): the newest entry first, and the view
	// pinned to the top; off, it reads as a terminal does.
	cli: {
		meta: meta('cli', 'cli', { reversed: false }, [{ key: 'reversed', label: 'newest first', type: 'boolean' }]),
		component: CliWidget,
		guide: `Every command run on this page and what datum answered, a frame each: the command on its top edge, the time it ran on its bottom one. Entries stand side by side as many as fit; a table, a picture or an error takes a whole row. There is no box to type in: the bar is its prompt.

## A tap

- Tap an entry's answer to select it (an id pill on the bar); shift adds a run of them, ctrl or ⌘ one more, and a tap on the one selected lets it go.
- Tap an entry's short id for its whole id, and again for the short.
- **↶** beside a command that wrote takes it back (\`revert\` of what it wrote), and the answer shows struck through; **↷** then puts it back. Where something changed it since, it says so: **more recent changes** goes to the command that did.

## Its options

- **newest first**: the newest command at the top, and the view kept there; off, it reads as a terminal does, the newest last.`,
		props: ({ reversed }) => ({ reversed: reversed === true }),
		tour: [
			{ at: WHOLE, say: 'Every command run on this page and what datum answered, a frame each. The bar is its prompt.' },
			{ at: '.doc', say: "Tap an entry's answer to select it: an id pill on the bar, for the next command.", do: 'tap' },
			{ at: 'button.undo', say: '↶ takes back what that command wrote, and ↷ then puts it back.' },
			{ at: WHOLE, say: 'What you log on collect is run here too, its answer a frame.', with: 'widgetbar' }
		],
		legible: { width: 200, height: 71 }
	},
	// How far back nowview reaches is its setting, which its − / + step
	// (da-pcis); `at` centres it on an instant, which the time cursor
	// overrides while it is set (NowviewWidget.svelte). `orientation` is
	// the dayview's pivot (da-78evn). `showHistory` off is the snapshot,
	// which was the `0` at the foot of the ladder (da-hklaz) — a tree
	// saved with that `0` is read as it (`migrated`, shell.ts).
	nowview: {
		meta: meta('nowview', 'nowview', { history: NOWVIEW_HISTORY, showHistory: true, orientation: 'horizontal', labels: false }, [
			{
				key: 'history',
				label: 'history',
				type: 'choice',
				choices: NOWVIEW_STEPS.map((value) => ({ value, label: value }))
			},
			{ key: 'showHistory', label: 'show history', type: 'boolean' },
			ORIENTATION,
			// Each field's state named in a gutter past the band, the CLI's
			// `--labels` (da-us41f, vividn: «please off by default»).
			{ key: 'labels', label: 'labels', type: 'boolean' }
		]),
		component: NowviewWidget,
		guide: `What each field is doing now, and how it got there: each field's lanes over the last half hour, now at the edge, sliding along with the clock.

## A tap, a hold

What you tap becomes a pill on the bar, for the next command; nothing is logged here.

- Tap an entry to select it (an id pill), again to let it go; ctrl or ⌘ adds or takes one away. Double tap for a time and keep the selection.
- Tap the ground for that time: near now it is a time back from now (\`-q\` is five minutes ago), further back a clock time. Hold, or right-click, for the change or the moment nearest.
- Hold, then drag, for a stretch: a time and a length. Hold near now and drag left for a length ending now. Shift and a drag needs no hold.
- Press along a time axis to scrub a time: the clock's for a clock time, the one counting back from now for a time back. With a time chosen, the other axis adds a length.

## Moving around

Drag sideways to look back in time, and let go to coast; it stops at now. Pinch, or ctrl and the wheel, to see more or less of the past. **← now** comes back to now and takes the time pills away (**reset**, while on now with one); **← sel**, when what is selected is out of view, goes to it.

Each axis has a diamond: tap or hold it for the lengths it can show, and tap one; drag it along the axis to stretch the window, or across it to step; the wheel and ↑ ↓ step too.

## Its options

- **history**: how far back it reaches, from 30s to 12h.
- **show history**: off, only what each field is doing now, a block each.
- **orientation**: **horizontal**, or **vertical**, time down the page.
- **labels**: each field's state named beside its lane.`,
		// `'0'` is read here too, not only by the migration: a session or a
		// hand-written tree that never went through `inline` may still say it.
		props: ({ history, showHistory, at, orientation, labels }) => ({
			history: typeof history === 'string' && history !== '0' ? history : NOWVIEW_HISTORY,
			showHistory: showHistory !== false && history !== '0',
			at: typeof at === 'string' ? at : undefined,
			vertical: orientation === 'vertical',
			labels: labels === true
		}),
		onChange: (settings, history) => ({ ...settings, history }),
		// Turned on its side with the picture: the time axis wants the 160,
		// the lanes across it the 108.
		tour: [
			{ at: WHOLE, say: 'What each field is doing now, and over the last half hour how it got there, now at the edge.' },
			{ at: '.picture', say: 'Tap the ground for that time, or an entry to select it: a pill on the bar, for the next command. Nothing is logged here.', do: 'tap' },
			{ at: '.picture', say: 'Drag sideways to look back in time; let go and it coasts, stopping at now.', do: 'drag' },
			{ at: '.nowview-handles button.handle', say: "An axis's diamond: tap or hold it for the lengths it can show, or drag it along the axis to stretch the window." },
			{ at: WHOLE, say: 'A field you log on collect lands here, at now.', with: 'widgetbar' }
		],
		legible: ({ history, showHistory, orientation }) => {
			const along = showHistory === false || history === '0' ? SNAPSHOT_WIDTH : 160;
			return orientation === 'vertical' ? { width: 108, height: along } : { width: along, height: 108 };
		}
	},
	// Its type size is a declared option (da-hkjp): `small` is 0.85 of the
	// pane body's 0.85rem. The legible box follows it, as the strip's and
	// nowview's follow theirs — the padding is the same 16px either way and
	// the text inside it is what shrinks, so smaller type is legible in a
	// shorter, narrower box rather than merely fitting more into the old one.
	tail: {
		meta: meta('tail', 'tail', { type: 'normal' }, [
			{
				key: 'type',
				label: 'type',
				type: 'choice',
				choices: Object.keys(TAIL_TYPE).map((value) => ({ value, label: value }))
			}
		]),
		component: CliPane,
		guide: `The newest entries, a row each, as \`datum tail\` lists them: the newest at the bottom, under a header that stays put. A row's state is colored, with the state before it and after it as a block of color either side; hover one for its name.

## A tap, a hold

- Tap a row to select it (an id pill on the bar); shift adds a run of rows, ctrl or ⌘ one more. On a touch screen, hold a row to select it and lock the pill, and then each tap adds a row or takes one away.

## Scrolling

Scroll up for older entries, which load as you go, back to the start of the log; scroll down for newer. **now ↑** or **now ↓**, when the newest entries are out of view, goes back to them.

It follows what is chosen elsewhere: a field shows that field alone, an entry selected is scrolled to, a time is scrolled to with a dotted line at it, and a stretch of time shows only the rows inside it.

## Its options

- **type**: **normal**, or **small**, more rows in the same room.`,
		props: (settings) => ({ spec: TAIL, scale: tailScale(settings) }),
		tour: [
			{ at: WHOLE, say: 'The newest entries, a row each, the newest at the bottom.' },
			{ at: 'tbody tr', say: 'Tap a row to select it: an id pill on the bar. Shift adds a run of rows, ctrl or ⌘ one more.', do: 'tap' },
			{ at: 'tbody', say: 'Scroll up for older entries, which load as you go.', do: 'scroll' },
			{ at: WHOLE, say: 'Choose a field on collect, a field pill, and the tail shows that field alone.', with: 'widgetbar' },
			{ at: WHOLE, say: 'A time or a stretch chosen on the dayview is scrolled to here, with a dotted line at it.', with: 'daystrip' }
		],
		legible: (settings) => {
			const s = tailScale(settings);
			return { width: Math.round(200 * s), height: Math.round(16 + 86 * s) };
		}
	},
	// The custom widget (da-wb2n): one command line the reader writes. It
	// keeps that line in its own localStorage key for now; making it a
	// widget setting is its own change. Called `command` until da-zqop,
	// which freed that id for the commands widget (da-ltux).
	custom: {
		meta: meta('custom', 'custom'),
		component: CustomPane,
		guide: `Any datum command you choose, run again whenever something changes, its answer as the CLI printed it.

## Choosing the command

**✎** in its title opens a box: type the command (\`tail -n 5\`, \`stats --week\`, \`nowview\`) and press Enter; Escape keeps the one before. A command that only reads runs again each time your data changes. One that writes runs once, and then only when you press **▶**.`,
		tour: [
			{ at: WHOLE, say: 'Any datum command you choose, run again whenever your data changes, its answer as the CLI printed it.' },
			{ at: 'button.edit', say: 'Tap ✎ to type its command, tail -n 5 or stats --week, and press Enter.', do: 'tap' }
		],
		legible: { width: 200, height: 102 }
	},
	// The commands widget (da-ltux): every command as a round button, undo
	// first. It took the id `command` back from the widget above, which
	// da-zqop renamed to `custom` to free it. Legible at one row of circles
	// and the names under them (47px, da-fg4w4), or the ready command's
	// larger circle and its name (57px, da-h3jcu), plus the body's padding —
	// the scroll rule takes it from there, and one row reached sideways is
	// still every command.
	command: {
		meta: meta('command', 'command'),
		component: CommandPane,
		guide: `Every datum command, as a round button: tap one to run it, or to start writing it on the bar.

## The buttons

First **undo**, which takes back the last command; the small **↷** joined to it is **redo**, lit when there is something to redo. Then the groups, each opening its commands, with **back** to return:

- **write**: moment, on, off, change, record, remark, flag, update, retime, delete, revert
- **read**: tail, now, stats, get, history, grep, fields, count, info, check
- **fields**: setup, track, ignore, alias, rename
- **kit**, **db**: their subcommands. **db backup** and **db restore** open Backups.
- **sync**: opens Sync. When two devices changed one entry, it carries a red count and opens **conflicts** too.
- **settings**: config, and **help**, which explains the next thing you tap.

## A tap, and a hold

A tap on a logging command (moment, on, off, change, record…) writes it on the bar, and the collect widget then shows only the fields it fits: tap one to log. Any other command is written for you to finish, the pills you hold put in their places. A button with a double outline runs at once.

With pills on the bar, only the commands that can take them are shown. A command they make ready leads, larger: filled, a tap runs it (\`moment coffee\`, \`delete\` an entry selected); dashed, a tap writes it for you to finish.

Hold a command for its flags: back, help, then -t, -d, -y, -q and the rest, each added to the line by a tap. Escape steps back, out of the flags and then the group.

The ❯ on the bar opens these same buttons, one row, over the bar.`,
		tour: [
			{ at: WHOLE, say: 'Every datum command, a round button each: tap one to run it, or to start writing it on the bar.' },
			{ at: '.undo', say: 'undo takes back the last command; the small ↷ joined to it is redo.' },
			{ at: 'button[data-menu]', say: 'Tap a group to open its commands; back returns. Hold a command for its flags.', do: 'tap' },
			{ at: WHOLE, say: 'A logging command, moment or on, written on the bar narrows collect to the fields it fits: tap one to log it.', with: 'widgetbar' }
		],
		legible: { width: 160, height: 72 }
	},
	// The stats widget (da-n7r0): its settings ARE the widget's settings, and
	// its range chips move them with an `onchange` like any other widget's own
	// control — so since da-vhwq THIS SESSION keeps them, not the tree, and
	// «Save options with layout» is what hands a window over (Layout.svelte).
	// Which is why a reload starts the widget back at what the layout says.
	//
	// And the window is the whole of them since da-ptuwr: the field selector
	// is gone, so there is no selection left for a session or a layout to
	// keep. `range` is lifted out by NAME rather than spread, so a `field`
	// left in a layout saved before the change is not carried back in.
	stats: {
		meta: meta('stats', 'stats', { ...DEFAULT_STATS }),
		component: Stats,
		guide: `How much of each field there is over a window of time: how many entries, how long it ran, and its share of the time tracked, with a bar of the shares, a slice a field (hover one for its share).

## Choosing the window

The chips choose it: **all time**, **today**, **week**, **month**, or **custom**, which asks **since** and **until**. A chip is this tab's on this device, and **Save options with layout**, in its menu, keeps it with the layout.

With a field selected (a field pill on the bar, from the collect widget), it shows that field alone, its states one by one.

It runs \`stats\`, and its table is the CLI's own.`,
		props: ({ range }) => ({ settings: { range: (range ?? DEFAULT_STATS.range) as StatsRange } }),
		onChange: (_, settings) => settings as Record<string, unknown>,
		tour: [
			{ at: WHOLE, say: 'How much of each field there is over a window of time: its entries, how long it ran, its share of the time tracked.' },
			{ at: '.chips.range button', say: 'Choose the window: all time, today, week, month, or custom.', do: 'tap' },
			{ at: '.share', say: 'The bar of shares, a slice a field: hover one for its share.' },
			{ at: WHOLE, say: 'Choose a field on collect, a field pill, and stats shows that field alone, its states one by one.', with: 'widgetbar' }
		],
		legible: { width: 240, height: 140 }
	},
	// The chart widget (da-xv7ti): one key of some fields' entries over time,
	// a number key's points and means as a series a field, any other key's
	// values counted and stacked. The fields and the keys are OFFERED: the
	// widget has read them, the registry cannot list them. Legible at its
	// head, a legend line and a plot of about four ticks' height.
	chart: {
		meta: meta('chart', 'chart', { ...DEFAULT_CHART }, [
			{
				key: 'fields',
				label: 'fields',
				type: 'offered',
				many: true,
				choices: (_, offers) => offers[FIELDS_OFFER] ?? []
			},
			{
				key: 'key',
				label: 'key',
				type: 'offered',
				choices: (shown, offers) => offers[keysOffer(chartSettings(shown).fields)] ?? []
			},
			{ key: 'bucket', label: 'per', type: 'choice', choices: BUCKETS.map((value) => ({ value, label: value })) },
			{ key: 'range', label: 'window', type: 'choice', choices: RANGES.map((value) => ({ value, label: value })) },
			{
				key: 'show',
				label: 'show',
				type: 'choice',
				choices: SHOWS.map((value) => ({ value, label: { both: 'points and means', points: 'points', average: 'means' }[value] }))
			}
		]),
		component: ChartWidget,
		guide: `One key of the fields you choose, over time. A number is drawn as points, and a line of its mean per day, week or month, a color a field; anything else is counted per bucket and stacked. Hover a point, a mean or a bar for its number.

## Choosing what it draws

It starts empty: tap its name, and choose in its menu.

- **fields**: the fields drawn, as many as you like.
- **key**: the key drawn, the first number a field holds unless you choose one; the fields' state is a key too.
- **per**: a mean or a count per **day**, **week** (from Monday) or **month**.
- **window**: how far back, **7d**, **30d**, **90d**, **365d** or **all**.
- **show**: **points and means**, only the **points**, or only the **means**.

It reads each field's entries with \`tail F --json\`.`,
		props: (settings) => ({ settings: chartSettings(settings) }),
		tour: [
			{ at: WHOLE, say: 'One key of the fields you choose, over time: points and a line of their mean, or counts stacked.' },
			{ at: 'button.title', say: 'It starts empty: tap its name, and choose its fields and key in the menu.' }
		],
		legible: { width: 200, height: 120 }
	},
	// The collect widget (da-w6437), the fields widget until then (da-c7u8,
	// da-850j). The id `widgetbar` is the saved-layout key — every stored
	// layout and draft names it, so it stays. How a switch in
	// a state is labelled is one setting (`SwitchLabel` in fields.ts);
	// `showIgnored` (da-xhio) is the other, and it is the SESSION's — a way
	// to see the ignored fields the bar leaves out, alongside the setup
	// widget's form and not in place of it, which a reload keeps on this
	// device (da-ray91).
	widgetbar: {
		live: true,
		meta: meta('widgetbar', 'collect', { align: 'rows', switchLabel: 'field', showIgnored: false, clicks: 'one', side: 'left', fields: [] }, [
			// Where the buttons stand (da-omw4n): today's rows, or each at the
			// height its field is drawn in the pictures.
			{
				key: 'align',
				label: 'the buttons stand',
				type: 'choice',
				choices: [
					{ value: 'rows', label: 'in rows' },
					{ value: 'picture', label: 'aligned to the picture' }
				]
			},
			{
				key: 'switchLabel',
				label: 'a change field is labelled with its',
				type: 'choice',
				choices: [
					{ value: 'field', label: 'field' },
					{ value: 'state', label: 'state' }
				]
			},
			{ key: 'showIgnored', label: 'show ignored fields', type: 'boolean' },
			// vividn, 2026-11-03: «1-click vs 2-click mode which reverses long
			// press and short press operations» (da-u017i); 2026-11-04: «you can
			// still record with 1 tap unless there is some extra data or whatever
			// needed» (da-gra0z).
			{
				key: 'clicks',
				label: 'a tap',
				type: 'choice',
				choices: [
					{ value: 'one', label: 'logs what it can, 1-click' },
					{ value: 'two', label: 'opens and chooses, 2-click' }
				]
			},
			// vividn, 2026-11-04: «a "side: left | right" row in the widget
			// menu» (da-lw7ue): right is the mirror, the strip at the right edge.
			{
				key: 'side',
				label: 'side',
				type: 'choice',
				choices: [
					{ value: 'left', label: 'left' },
					{ value: 'right', label: 'right' }
				]
			},
			// vividn, 2026-11-05: «a widget option to filter a collect widget»
			// (da-gfy7w): set, only these fields; empty, every field. A name no
			// field has yet is offered and not drawn, so a layout may name a
			// kit's fields before the kit is installed. The page offers the
			// fields there are; each widget adds the names it holds itself, so
			// two collect panes never offer each other's (da-jbqf5).
			{
				key: 'fields',
				label: 'fields',
				type: 'offered',
				many: true,
				choices: (shown, offers) => {
					const there = offers[COLLECT_FIELDS_OFFER] ?? [];
					const own = (Array.isArray(shown.fields) ? shown.fields : []).filter(
						(f): f is string => typeof f === 'string' && !there.some((c) => c.value === f)
					);
					return [...there, ...[...new Set(own)].map((f) => ({ value: f, label: f }))];
				}
			}
		]),
		component: CollectPane,
		guide: `Every field you log, as a button: tap one to log it. A moment's button is an empty pill, an on/off field's fills while it is on, a change field's is filled with its state's color, and a record's name is underlined.

## At rest

The fields stand in rows. At the left edge is the last half second of today, a now strip, where what you tap is seen sweeping in at now; tap a field's band there to open that field. Then the fields that are on, inside a dotted line, then the rest, the last used first. Drag the rows sideways to see more. Pinch them, or ctrl and the wheel, to zoom: smaller fits more rows, larger fewer.

With a time chosen in another widget, the buttons show how each field stood then, and a moment logged at that instant is tinted: a tap on it deletes it.

## Time and duration

Between the now strip and the rows, at rest and beside an open field, stand **time −1m −5m** and **dur 1m 5m** (a column, or a row in a short pane). Each press adds up: −1m and then −5m log six minutes back, shown as a **−6m** pill, and 1m and then 5m a length of six minutes; the nowview and the day pictures show it as any chosen time. Drag a pill sideways to move it a minute at a time; its × takes it away.

A tap on **time** opens more ways to say it: a time typed as \`-t\` takes it (9:15am, yesterday 10am), **-y** goes a day back, **-d** to a date, and **-D** logs the whole day. A tap on **dur** opens a length typed (1h30m, 1.5h) and **-m**, a moment with no length. Drag them back toward the strip to close them.

## A tap, and a hold

**a tap**, in the menu, chooses what a tap does:

- **logs what it can, 1-click** (the default): a moment logs at once, an on/off field turns on or off, a record puts its command on the bar for you to finish, as does a field with data you must type. A change field opens.
- **opens and chooses, 2-click**: every field opens, and the larger button of the commands logs what you chose.

A hold, a right click or a shift click does what a tap does in the other mode. With a command waiting on the bar (from the commands, say), a tap logs that command for the field at once, and a hold opens it.

## An open field

An open field shows its name, then its states, its time and length being the section's beside it (a moment field has no length to give):

- A state logs the change at once (in 2-click mode, it is chosen, and the commands' larger button logs it).
- **key=** puts that key on the bar for you to type; **+ data** goes to the bar.
- **‹ back**, or Escape, closes it, and takes away the pills it made.

## Its options

- **the buttons stand**: **in rows**, or **aligned to the picture**, each at the height the day pictures draw its field. Aligned, a pinch zooms the heights and a drag up or down pans; the slim mark at the right zooms back out.
- **a change field is labelled with its**: **field**, or the **state** it is in.
- **show ignored fields**: the fields you stopped tracking, faint and dashed.
- **side**: the now strip at the **left** or the **right**.
- **fields**: only the fields chosen; none chosen, every field.

With an entry selected in a list (the CLI widget, the tail), it shows that entry instead: its field, state, time and data, **−A** for all of it, and **back**.

Each tap runs a command: \`moment F\`, \`on F\`, \`off F\`, \`change F S\`.`,		props: ({ align, switchLabel, showIgnored, clicks, side, zoom, fields }) => ({
			only: Array.isArray(fields) ? fields.filter((f): f is string => typeof f === 'string') : [],
			align: align === 'picture' ? 'picture' : 'rows',
			zoom: typeof zoom === 'number' ? zoom : 1,
			clicks: clicks === 'two' ? 'two' : 'one',
			side: side === 'right' ? 'right' : 'left',
			switchLabel: switchLabel === 'state' ? 'state' : 'field',
			showIgnored: showIgnored === true
		}),
		// A pinch on the rows (da-l1z6h) moves `zoom`, this session's as any
		// setting a widget moves itself, and so kept on this device across
		// reloads (da-ray91, da-gh1ep); not among the defaults, as the
		// strip's day height is not, so `Reset widget` is the way back to 1.
		onChange: (settings, zoom) => ({ ...settings, zoom }),
		tour: [
			{ at: WHOLE, say: 'Every field you log, a button each.' },
			{ at: '.room button.widget', say: 'A tap on a field logs it: a moment at once, an on/off field turned on or off; a change field opens its states.' },
			{ at: '.when button.step', say: 'Tap −1m: the next field you log is logged a minute back. Each press adds up.', do: 'tap' },
			{ at: '.room', say: 'Drag the rows sideways to see more; pinch, or ctrl and the wheel, to zoom.' },
			// A hold opens the field and logs nothing: a tour asks for no entry that did not happen.
			{ at: '.room button.widget', say: 'Hold a field to open it: its states, its time and its data, and nothing logged until you choose.', do: 'hold' },
			{ at: WHOLE, say: 'What you log lands in the nowview, at now.', with: 'nowview' },
			{ at: WHOLE, say: "And in today's band of the dayview.", with: 'daystrip' }
		],
		legible: FIELDS_LEGIBLE
	},
	// The aliases widget (da-x6yj): every alias, listed.
	aliases: {
		meta: meta('aliases', 'aliases'),
		component: AliasesWidget,
		guide: `Every alias, a word that stands for a command: your own first, as the word and what it runs, then the ones a field's setup gives it, by field, in its color.

## Adding one

It lists them and changes nothing. Type \`alias set <word> "<command>"\` in the bar to make one; a field's own come from its aliases tab in setup.`,
		tour: [
			{ at: WHOLE, say: "Every alias, a word that stands for a command: your own first, then the ones a field's setup gives it. Type alias set in the bar to make one." }
		],
		legible: { width: 160, height: 72 }
	},
	// The sync widget (da-x4t0a, da-yoef7): the page's database's devices,
	// a line each, then the other databases, then the acts. Legible at a
	// device's line wrapped once, two lines more and the acts' row, plus
	// padding. Its guide is the sync guide (da-oyvsz), which its own `?` opens
	// too, and folds `datum help sync` at its end.
	sync: {
		meta: meta('sync', 'sync'),
		component: SyncWidget,
		guide: `Sync keeps the same data on each of your devices: a phone, a laptop's browser, a computer with the datum command. Devices talk to each other directly. What they send is encrypted end to end, so a relay that helps them find each other passes on only what it cannot read.

## Where it is

The gear at the top of the page, then **Sync**; or type \`sync\` in the bar. [Open Sync](place:sync)

## Connect a second device

1. On the device that has your data, open Sync and press **Turn on sync**. Give this device a name you will recognize, and your username if you like, which every device you sync names you by, then press **Start sync**. Once sync is on, it is **Add device**.
2. A QR code appears. Scan it with the other device's camera, or press **Copy link** and open the link in the other device's browser.
3. The other device asks **Join a sync chain?** Give it a name you will recognize, and your username if you like, and press **Join**.
4. Each device now lists the other. Press **Done** on the first to put the QR away.

A link adds one device, once, and only until the time it says. For the next device, press Add device again. On a computer with the datum command, open **join from a terminal** under the QR and run what it shows there.

## What syncs

Everything in the database: entries, fields, aliases, and layouts kept with Save layout. A layout you only Accept stays on its device. Each database syncs on its own: Sync shows the one this page is on, and your other databases under it.

Joining merges: the joining device keeps what it had, and after the first sync both hold everything. Devices sync whenever two of them are online at once; a browser does while datum.day is open in it.

## When it says a device joined

Every other device shows a ⚠ in the tab bar: tap it, and it says «Another device joined», names it and when, and offers to expel it should it not be yours. Sync lists it too, with **OK**. The device that joined is told the chain's members once, plainly.

- One you added: press OK.
- One you don't know: in Sync press **More**, then **Expel** beside it, so it can no longer sync. **Unjoin** also takes back everything it changed.

Give each device a name you will know it by with **Rename this device**, which asks for your username too. More also holds **Pause sync**, and **Leave chain**, which stops this device syncing and keeps its data.

## Reading the widget

Every device syncing this database, each CLI install and browser, a line each, this one first, with when it was last seen; then your other databases, a tap each to open it. ✓ synced, ↻ syncing, ◔ pending, ! error (its reason under it), — not syncing. A device not reachable now is drawn gray, as it was when last seen. Its ? opens this guide.`,
		commands: 'sync',
		tour: [
			{ at: WHOLE, say: 'Sync keeps the same data on each of your devices, sent from one to another directly, encrypted end to end.' },
			{ at: '.actions button.command', say: 'Turn on sync, here, to connect a second device: it shows a QR code for the other to scan.' },
			{ at: 'button.guide', say: 'Its ? opens the guide to syncing, step by step.' }
		],
		legible: { width: 140, height: 100 }
	},
	// The Help place (da-z955f): the Help button's last answer (#lib/help.ts).
	// Legible at the title line and two lines of the explanation (18.4px
	// each) plus padding; the rest scrolls.
	help: {
		meta: meta('help', 'help'),
		component: HelpWidget,
		guide: `What Help was last asked about, explained: a widget's guide, a command's page, a pill or a control.

## Asking

Press **(?)** in the top bar (or **help** among the commands), then tap anything: it is explained here instead of used, and a widget starts its tour. Press it twice for how to use the page. **← front page** goes back to the welcome's words, the front page.

A command's page leads with what it does; its usage and options are folded under it, a tap to open each, and stay open on this device.`,
		tour: [
			{ at: WHOLE, say: 'What Help was last asked about, explained: press (?) in the top bar, then tap anything.' }
		],
		legible: { width: 200, height: 72 }
	},
	// The welcome widget (da-u7vz8): what datum is, the tutorial, and where
	// to read more — the first tab of a first visit (short since
	// da-uzyih.19). Legible at its title and the two sentences under it
	// (20px a line) plus padding; the rest scrolls.
	welcome: {
		meta: meta('welcome', 'welcome'),
		component: WelcomeWidget,
		guide: `What datum is, in a line, and where to start: the first tab of a first visit, and the Help place's front page.

## Its buttons

- **Tutorial** opens the tutorial, in a database of its own: fields, their kinds, the bar and tabs, taught by small games.
- **Read the docs** opens datum's docs here, in the page.
- The links point an AI agent at datum.day, and open datum's source.
- **Close welcome**, where the pane can close, closes it.`,
		tour: [
			{ at: WHOLE, say: 'What datum is, and where to start.' },
			{ at: 'button.command', say: 'Tutorial teaches fields, their kinds and the bar by small games, in a database of its own.' },
			{ at: '[data-open="docs"]', say: "Read the docs opens datum's docs, here in the page." }
		],
		legible: { width: 200, height: 100 }
	},
	// The docs widget (da-0d8l6), a place: the welcome's «Read the docs» opens
	// it. Legible at its back row and two lines of a page (about 20px each,
	// estimated) plus padding; the rest scrolls.
	docs: {
		meta: meta('docs', 'docs'),
		component: DocsWidget,
		guide: `datum's docs, read in the page: the contents first, a page for each part of datum, and under them how to use the page and each widget.

## Reading

A link to another doc opens it here; **← back** returns, and **contents** goes to the contents. A link out of the docs opens in a new tab, as does a link pressed with ctrl, ⌘ or shift.`,
		tour: [
			{ at: WHOLE, say: "datum's docs, read in the page." },
			{ at: 'article.doc a[data-doc]', say: 'Tap a link to another doc: it opens here.', do: 'tap' },
			{ at: 'button.back', say: '← back returns to the doc before.', do: 'tap' }
		],
		legible: { width: 200, height: 76 }
	},
	// The premade widget (da-36whg): fields ready to set up, by theme, each
	// opening setup filled in. Legible at its sentence and one row of cards
	// (about 40px each, estimated) plus padding; the rest scrolls.
	premade: {
		meta: meta('premade', 'premade'),
		component: PremadeWidget,
		guide: `Fields ready to set up, by theme: body, food & drink, mind, movement, work, home & chores, screens & media, habits, records.

## A tap

Tap one to open setup with that field filled in: its kind, color, symbol, states and data. Nothing is saved until you save it there. A field you have already says **set up**, and a tap opens setup on it. Weight asks first which you weigh in, **lbs** or **kg**.

Under home & chores, **Install the chores kit** adds chores on a schedule; it says what it installs, and **install** installs it.`,
		tour: [
			{ at: WHOLE, say: 'Fields ready to set up, by theme.' },
			{ at: 'button.card', say: 'Tap one to open setup with that field filled in; nothing is saved until you save it there.' },
			{ at: 'button.install', say: 'Install the chores kit adds chores on a schedule; it says what it installs first.' }
		],
		legible: { width: 200, height: 100 }
	},
	// «Backups» (da-9ow8m), a place (da-ea5q2): the gear's Backups and the
	// commands widget's `db backup` / `db restore` open it (`openPlace`). Legible
	// at its heading over the two file buttons, stacked (88px measured; the
	// wider is 193px), plus padding: side by side they want 388px.
	backups: {
		meta: meta('backups', 'backups'),
		component: BackupsWidget,
		guide: `Backups of your data: a file on this device, and plans that back up to a bucket of your own on a schedule.

## A file

**Save a backup file** saves everything in this database as a file; **Restore from a file** brings one back.

## Plans

A plan backs up while a datum.day tab is open: every 15 minutes, hourly, every 6 hours or daily, into a set of slots kept as a rotation, so older backups stay. Backups are not encrypted.

**New plan** asks a name, where to (a bucket and its keys, with **Test connection** and **Look for backups there**), which databases, how often and how many slots; **Add plan** adds it. Each plan says when each database last backed up, and lists its backups, newest first, each with **Restore**, which says what it would change and asks first. **Back up now** backs up at once, **Back up all now** every plan, and **Delete plan** deletes it.`,
		tour: [
			{ at: WHOLE, say: 'Backups of your data: a file on this device, and plans that back up to a bucket of your own.' },
			{ at: 'section.local', say: 'Save a backup file saves everything in this database as a file; Restore from a file brings one back.' },
			{ at: 'section.add input', say: 'A new plan: give it a name, then a bucket and its keys, the databases, how often and how many slots.', do: 'type' }
		],
		legible: { width: 206, height: 99 }
	},
	// «Settings» (da-99g5q), a place: the gear's «Settings» opens it
	// (`openPlace`). The page's settings, each with a line saying what it
	// does. Legible at the first setting and its line (about 70px,
	// estimated) plus padding; the rest scrolls.
	settings: {
		meta: meta('settings', 'settings'),
		component: SettingsWidget,
		guide: `The page's settings, each with a line saying what it does.

## The settings

- **Track timezone** logs this device's zone to the timezone field, so each day is drawn in the zone you lived it in; pressed again, as **Stop tracking timezone**, it stops.
- **Wrap tabs when swiping**, on this device: a swipe past the last tab goes to the first, and past the first to the last.`,
		tour: [
			{ at: WHOLE, say: "The page's settings, each with a line saying what it does." }
		],
		legible: { width: 200, height: 86 }
	},
	// The conflicts widget (da-6cutb), a place: the commands widget's
	// «conflicts» and a typed `conflicts` open it. Legible at one entry — its
	// heading, a key's sides, the strategies and the Resolve row (about 26px
	// each, estimated) — plus padding; the rest scrolls.
	conflicts: {
		meta: meta('conflicts', 'conflicts'),
		component: ConflictsWidget,
		guide: `Every entry two of your devices changed without seeing each other, a key at a time: each side lettered A, B…, with what it holds, its device and when.

## Settling one

Tap a side to keep it for that key, or type a side's letter or what the key should hold. For the keys you did not pick, choose **keep as shown**, or one side over the other, or a merge. **Resolve** settles the entry on every device; beside it is the command it runs, \`conflicts resolve\`.`,
		tour: [
			{ at: WHOLE, say: 'Every entry two of your devices changed without seeing each other. Tap a side to keep it; Resolve settles it on every device.' }
		],
		legible: { width: 220, height: 120 }
	},
	// The setup widget (da-fu29p, named `field` until da-rgyq8): where a field
	// is made and changed — the Track dialog and the spec widget (da-hbrfg,
	// kinds `spec` and `field`: `RETIRED_KINDS` in shell.ts) as one. Which field it is on is `editing`
	// (#lib/fieldWidget.ts), every copy's, so it has no setting. Legible at
	// its sentence, the form's name line and its buttons' row, with
	// their gaps and padding — about 180px, estimated from the 28px controls
	// rather than measured; a pane shorter scrolls the widget.
	setup: {
		meta: meta('setup', 'setup'),
		component: FieldWidget,
		guide: `A field is one thing you track: mood, coffee, sleep. This is where one is made, or changed.

## Choosing a field

With no field, it says **field:** and **track something**. Its sheet asks for a field name: Enter, or ⏎, opens that field, or a new one by that name; under it are the fields there are, narrowed as you type. A chosen field is a pill, whose **×** starts over, and **rename** renames it (asking first to merge it into another of that name).

Under the name it says whether the field is **NOT SET UP**, **TRACKED** or **IGNORED**, an on/off or change field its current state, and a button: **stop tracking** takes it off the collect widget (\`ignore F\`), and **start tracking** puts it back (\`track F\`), at once.

## Its kind

Four buttons choose what logging the field makes: **●** moment, a single point in time; **█** on/off, on for a while, then off; **▞** change, one state until the next; **¢** record, data with no time.

## basics, data, aliases

Three tabs, under the form; tap one or use ← and →.

- **basics**: a description, the field's color (or auto) and its symbol (a character or a picture). A change field lists its states: drag **⠿** (or ↑ ↓) to reorder one, give it a color and a name, **…** for what it means and its symbol, **✕** to remove it; **+ state** adds one, and **off**'s ✕ lets the field never be off. Then where the field is drawn in the day pictures: the eye hides it, the band places and sizes its lane by a drag or a pinch, and it can anchor to another field.
- **data**: what each entry carries besides its state, a line each: its name, how it is asked for, whether it is optional and its default, its type; **+ key** adds one.
- **aliases**: the words that log it. **made from the kind and states** makes them for you; off, you write your own, **+ alias** each.

## Saving

A new field has **start tracking**, which makes it and puts it on the collect widget (\`setup F --track\`, which one Undo takes back), **save without tracking**, which makes it ignored (\`--ignore\`), and **cancel**, which makes nothing.

A field already made has **Save** (one \`setup F\`; leaving it saves too), **Revert**, which drops what is not saved, or with nothing to drop steps the field back a version, through its whole history, and **Redo**, forward again.`,
		tour: [
			{ at: WHOLE, say: 'A field is one thing you track: mood, coffee, sleep. This is where one is made, or changed.' },
			{ at: 'button.track-something', say: 'Tap to name a field: a new one, or one you have. Then its kind, its states and data, and Save.' }
		],
		legible: { width: 240, height: 180 }
	},
	// The input widget (da-0dhc, back as da-3zon): the bottom bar's command
	// line drawn in a pane — the same buffer, the same pills, the same
	// Enter. Legible at its prompt row (36.4px measured) plus one line of
	// receipt above it (a document line at 1rem/1.35, 21.6px): the least
	// that is still a command line with an answer over it.
	// `reversed` (da-gvdc): the prompt row on top, the receipt under it.
	input: {
		meta: meta('input', 'input', { reversed: false }, [{ key: 'reversed', label: 'prompt on top', type: 'boolean' }]),
		component: InputWidget,
		guide: `The command line, in a pane: the same line, pills and Enter as the bar, with what the last command answered above it.

## Typing

Type a command and press Enter, or **➤**; ↑ and ↓ walk back through what you ran; **×** clears the line and its pills. The pills the other widgets make land here too.

## Its options

- **prompt on top**: the line above, and the answer under it.`,
		props: ({ reversed }) => ({ reversed: reversed === true }),
		tour: [
			{ at: WHOLE, say: 'The command line, in a pane: the same line, pills and Enter as the bar.' },
			{ at: 'form.line input', say: 'Type a command, tail say, and press Enter; ↑ and ↓ walk back through what you ran.', do: 'type' },
			{ at: '.receipt', say: 'What the last command answered.' }
		],
		legible: { width: 200, height: 58 }
	},
	// The debug widget (da-o2wm8), offered in the DEBUG database alone: the
	// bottom bar's TAU and DETENT as live knobs, so that the one person who
	// can judge how the bar FEELS can move them on his own phone. Legible
	// at its two knob rows and the report line under them (3 × 26px) plus
	// the body's padding — narrower or shorter and a slider is not a
	// slider. "we could even have different DEBUG widgets eventually": the
	// next one is another `debugKind` line here, not a rewrite of this one.
	debug: debugKind(
		'debug',
		{ width: 200, height: 94 },
		`How the bar feels when you drag it, as live knobs, so it can be tuned on a phone. Only in the DEBUG database.

## The knobs

**TAU**, how long a release keeps gliding, and **DETENT**, how near a resting place takes a glide, each a number and a slider. **copy** copies the two, and **reset to defaults** puts them back.`
	),
	// The second (da-6ecm5): a plausible week of sample data, seeded
	// into the database it runs in. Legible at its two buttons and a line
	// of report; the log under them scrolls.
	seed: debugKind(
		'seed',
		{ width: 200, height: 60 },
		`Seeds a plausible week of sample data into this database, to try things on. Only in the DEBUG database.

## Its buttons

**seed the last 7 days** logs a week of entries marked as the sample; **clear the sample** deletes them; **install the sample kit** and **uninstall the sample kit** add and remove its fields. Each command it ran is listed under them, with its answer.`
	),
	// A lesson of the tutorial (da-6cvrd): offered in its database alone,
	// where its tabs carry one each. Legible at a heading and two lines; its
	// fields part draws the collect widget, and is legible where that is.
	tutorial: {
		meta: meta('tutorial', 'tutorial', { lesson: 'intro' }),
		component: TutorialWidget,
		guide: `A lesson of the tutorial, which has a database of its own: its text, or, in a game's tab, its parts, its title and what its kind of data is over a nowview, the game and its collect, and under them what just happened.

## The lessons

The Introduction, then a game for a moment, for on/off, and for a change, then the bar and tabs. **Next** at the foot of each goes to the next. The Introduction's **where?** shows where to go back to your own database, datum; **Reset the tutorial** empties the tutorial's database and puts its tabs back, to start again.`,
		db: TUTORIAL_DB,
		tour: [
			{ at: WHOLE, say: 'A lesson of the tutorial, in a database of its own.' }
		],
		legible: ({ part }) => (part === 'fields' ? FIELDS_LEGIBLE : { width: 200, height: 100 })
	},
	// A lesson's game (da-2chdp, vividn, 2026-11-05: «game (should be called
	// game widget)»), in the tutorial's database alone, as its lesson is.
	game: {
		meta: meta('game', 'game widget', { lesson: 'moment' }),
		component: GameWidget,
		guide: `A lesson's game, in a box: **Start** begins a round, **End now** ends it early while it runs, filling as its time runs out.

## Playing

You log what happens in the collect widget beside it, as the game asks. When the round is over it reads back what you logged and says your score; **Reset** starts again. The change game starts with your first tap.`,
		db: TUTORIAL_DB,
		tour: [
			{ at: WHOLE, say: "A lesson's game: Start begins a round, End now ends it early." }
		],
		legible: { width: 200, height: 100 }
	},
	// A widget a KIT shipped (da-gh37k): `settings` name the kit and its
	// widget, whose command and row action are in the kit's own entry in
	// this database (store.ts), and carry the title it was chosen under.
	// The chooser offers each one the open database's kits ship, and no
	// other, so the kind is not listed itself. Legible, as the tail is, at
	// three rows.
	kit: {
		meta: meta('kit', 'kit widget'),
		component: KitWidget,
		hidden: true,
		guide: `A widget a kit installed in this database: the rows its command lists, each in the color the kit gives it. The chooser offers it only in the database the kit is in.

## A tap

Tap a row to do what the kit says a tap does: the chores kit's logs that chore done. A kit's widget only reads, and a tap only logs.`,
		title: ({ title, widget }) => [title, widget].find((t): t is string => typeof t === 'string'),
		tour: [
			{ at: WHOLE, say: 'A widget a kit installed: the rows its command lists. Tap a row to run what the kit gives it.' }
		],
		legible: { width: 160, height: 102 }
	},
	// What the shell shows in a tab whose layout cannot be inlined (da-0a5o).
	notice: {
		meta: meta('notice', 'notice'),
		component: Notice,
		hidden: true,
		guide: `A tab whose layout could not be drawn here, and why.

## What to do

It says what was wrong. Change the tab's widget, or close it, in layout mode; or **Reset default layouts** in the gear puts datum's own tabs back.`,
		tour: [
			{ at: WHOLE, say: 'A tab whose layout could not be drawn here, and why.' }
		],
		legible: { width: 120, height: 40 }
	}
};

/** The kinds the chooser offers in `db`, in the order it shows them. */
export const pickableIn = (db: string): string[] =>
	Object.keys(KINDS).filter((kind) => !KINDS[kind].hidden && (KINDS[kind].db ?? db) === db);

/**
 * The kinds EVERY database offers — the ungated ones.
 *
 * A kind gated to one database (`db` above) is deliberately not among
 * them, because this list carries a second rule: at the root a chosen
 * widget becomes a one-widget LAYOUT by name, so every kind here needs
 * its line in `BUILTIN` (`PICKABLE ⊆ BUILTIN`, tests/tree.test.mjs). The
 * built-ins are the same in every database and a gated kind's layout is
 * not — it is the one its OWN database carries, which is why `widgetInto`
 * (shell.ts) accepts a kit layout's name there as it accepts a built-in's.
 */
export const PICKABLE = Object.keys(KINDS).filter((kind) => !KINDS[kind].hidden && !KINDS[kind].db);

export const titleOf = (kind: string, settings: Record<string, unknown> = {}): string =>
	KINDS[kind]?.title?.(settings) ?? KINDS[kind]?.meta.title ?? kind;

/**
 * What a tab is CALLED, wherever it is shown: the root bar's tabs, a pane
 * frame's title and its tab strip all read this one rule (da-9puz).
 *
 * The layout a tab holds is its name — a tab carrying `layout` is one
 * thing with an identity, and wears it, as `layoutTitle` calls it
 * (`collect` reads «collection», defaults.ts). Failing that its own `title`, and
 * failing that what it shows: the widget kind's title, or for a split the
 * names of its panes, `tail | nowview` (`nameOf`, tree.ts, da-6ji5).
 */
export const labelOf = (tab: Tab): string => nameOf(tab, titleOf, layoutTitle);

/**
 * The settings a fresh widget of `kind` starts with, or undefined when the
 * kind declares none — which is what `Reset widget` asks the registry
 * (da-7ymx): a kind with nothing written down has nothing to put back, so
 * the menu leaves the item out. A COPY, so a reset cannot write through to
 * the registry's own object.
 */
export const defaultsOfKind = (kind: string): Record<string, unknown> | undefined => {
	const declared = KINDS[kind]?.meta.defaultSettings;
	return declared && Object.keys(declared).length > 0 ? { ...declared } : undefined;
};

/** The options a kind declares for its menu (da-gvdc, da-vhwq), or undefined when it declares none. */
export const optionsOfKind = (kind: string): WidgetOption[] | undefined => {
	const declared = KINDS[kind]?.meta.options;
	return declared && declared.length > 0 ? declared : undefined;
};

/** A kind's snaps, from its settings; a kind that declares none (or an unknown one) has none. */
export const snapsOfKind = (kind: string, settings: Record<string, unknown> = {}) => {
	const s = KINDS[kind]?.snap?.(settings);
	return { width: s?.width ?? [], height: s?.height ?? [] };
};

/** A kind's legible box, from its settings where it follows them; an unknown kind (a draft from a newer app) has none. */
export const legibleOfKind = (kind: string, settings: Record<string, unknown> = {}): Floor => {
	const l = KINDS[kind]?.legible;
	return typeof l === 'function' ? l(settings) : (l ?? { width: 0, height: 0 });
};
