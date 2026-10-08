// The widget kinds: what the tree can name, and what draws each one.
//
// Everything on the page is a widget (vividn: "They are widgets like
// anything else yes") — the day strip, the REPL and the fields widget
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
// body's 16px of padding — its snap. The REPL: three lines of scrollback
// (18.4px each) plus padding, 71px — the bottom bar is its prompt
// (da-rfs9), so it has no line of its own to keep. The tail,
// and a custom pane showing one: its header row and three rows (21.6px
// each) plus padding, 102px — and the tail's follows its `type` option
// (da-hkjp), the 16px of padding standing still while the text in it
// scales. The fields widget: one row, as tall as the «Track +» it began
// with (33px; the (+) it became is a field's height, da-y6jfb), plus
// padding. The aliases list: three of its lines
// (18.4px each) plus padding, 72px. The input widget: its prompt row
// (36.4px) and one document line of receipt (21.6px), 58px. nowview: datum draws it no shorter than a 50px band plus its
// axis, 92px of picture (da-wb2n), plus padding. Widths are where each
// stops making sense rather than where it stops rendering: an hour of the
// strip per 10px, a CLI line, the tail's four columns, four chips. The
// frame's own chrome is added by the renderer, not counted. One reader
// only: the root's minimum (Layout.svelte), so a screen too small for
// every pane scrolls the page rather than squeezing them — a drag never
// sees these numbers.

import type { ComponentType } from 'svelte';
import type { Tab, WidgetKindMeta, WidgetOption } from './types.js';
import { nameOf, type Floor } from './tree.js';
import CliPane from '$lib/CliPane.svelte';
import CommandPane from './CommandPane.svelte';
import CustomPane from '$lib/CustomPane.svelte';
import DebugPane from '$lib/DebugPane.svelte';
import Stats from '$lib/Stats.svelte';
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
} from '$lib/panes.js';
import { REACH_PX } from '$lib/lanes.js';
import { DEBUG_DB } from './defaults.js';
import { DEFAULT_STATS, type StatsRange } from '$lib/stats.js';
import AliasesWidget from './AliasesWidget.svelte';
import BackupsWidget from './BackupsWidget.svelte';
import DayviewWidget from './DayviewWidget.svelte';
import FieldsPane from './FieldsPane.svelte';
import HelpWidget from './HelpWidget.svelte';
import InputWidget from './InputWidget.svelte';
import Notice from './Notice.svelte';
import NowviewWidget from './NowviewWidget.svelte';
import ReplWidget from './ReplWidget.svelte';
import FieldWidget from './FieldWidget.svelte';
import SyncWidget from './SyncWidget.svelte';

export interface WidgetKind {
	meta: WidgetKindMeta;
	component: ComponentType;
	/** Props for the component, when it wants more than `settings`. */
	props?: (settings: Record<string, unknown>) => Record<string, unknown>;
	/**
	 * What the component's `change` event means for its settings: the
	 * detail folded into them. A kind without one has nothing to keep.
	 *
	 * It is given the settings as SHOWN and what it returns lands in THIS
	 * SESSION (da-vhwq, Layout.svelte), not in the tree — so a widget
	 * moving one of its own settings cannot modify the layout, locked or
	 * not.
	 */
	onChange?: (settings: Record<string, unknown>, detail: unknown) => Record<string, unknown>;
	/**
	 * What the Help button says about this kind (da-vjwid): what it shows,
	 * what a tap does, what a hold does — a line each ($lib/help.ts).
	 * Required, so a kind cannot ship
	 * without one; `tree.test.mjs` asks the same of every kind.
	 */
	help: string;
	/** Content-box sizes a border drag snaps to, from the settings: see the note above. */
	snap?: (settings: Record<string, unknown>) => { width?: number[]; height?: number[] };
	/** Not offered by the chooser: a kind the app puts in a tab itself. */
	hidden?: boolean;
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

/**
 * How narrow nowview's snapshot (`history: '0'`) stays usable.
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
 * Aiming is the larger of the two, and `$lib/lanes.ts` already has its
 * number: `REACH_PX` is «a fingertip's half-width», and `NOW_PX` in
 * $lib/nowpoint.ts is the same 24 for the same reason. A block one whole
 * fingertip across is a target a finger can land on; narrower and the
 * press is aimed at something smaller than the thing pressing it. The
 * pane hands its content box to the CLI as `--width` (`wantsBox`,
 * $lib/panes.ts) and the CLI insets by `SVG_MARGIN_PX` each side, so the
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
const debugKind = (kind: 'debug' | 'seed', legible: Floor, help: string): WidgetKind => ({
	meta: meta(kind, kind),
	help,
	component: DebugPane,
	props: () => ({ kind }),
	db: DEBUG_DB,
	legible
});

export const KINDS: Record<string, WidgetKind> = {
	// The strip fills its box and shows as much of the past as fits above
	// today (da-zb0d). Its settings are what the reader has moved on it:
	// the day height and the date column (the `+` handle), whether today
	// is frozen at the bottom, and the hour range it is zoomed onto — so
	// a saved layout keeps a zoomed strip zoomed, and `Reset widget` is
	// the way back to the whole day (da-ukcf).
	daystrip: {
		// No `dayHeight` among the defaults: its default is DERIVED from the
		// pane's height (da-r0ydu), and a declared one would be what a
		// layout falls back to and what `Reset widget` restores — both of
		// which must be the follow.
		meta: meta('daystrip', 'dayview', {
			labelWidth: DEFAULT_LABEL_WIDTH,
			freezeToday: true,
			hourFrom: HOUR_FROM,
			hourTo: HOUR_TO,
			orientation: 'horizontal'
		}, [
			{ key: 'freezeToday', label: 'today frozen at the bottom', type: 'boolean' },
			// A vertical dayview is a week of columns.
			ORIENTATION
		]),
		component: DayviewWidget,
		help: [
			'Your days as bands of colour: today at the bottom, the days before it above.',
			'Tap a moment for a -t time pill; hold, then drag, for a span.',
			'Hold still, or double tap, to select the entry under your finger. Pinch zooms.'
		].join('\n'),
		props: ({ dayHeight, labelWidth, freezeToday, hourFrom, hourTo, orientation }) => ({
			orientation: orientation === 'vertical' ? 'vertical' : 'horizontal',
			// Undefined on purpose: the strip derives its day from its own
			// box while nothing is set (da-r0ydu), and a default here would
			// be a number nobody chose standing in the way of that.
			dayHeight: typeof dayHeight === 'number' ? dayHeight : undefined,
			labelWidth: typeof labelWidth === 'number' ? labelWidth : DEFAULT_LABEL_WIDTH,
			freezeToday: typeof freezeToday === 'boolean' ? freezeToday : true,
			hourFrom: typeof hourFrom === 'number' ? hourFrom : HOUR_FROM,
			hourTo: typeof hourTo === 'number' ? hourTo : HOUR_TO
		}),
		// The strip's `change` names the settings it moved — the `+` handle
		// moves two at once — so the fold is the same one for all of them.
		onChange: (settings, moved) => ({ ...settings, ...(moved as Record<string, unknown>) }),
		// A notch, not a wall: shorter, the strip clips today's band from
		// the top and keeps its axis. Only while the reader HOLDS a day
		// height — following its box (da-r0ydu) every height is one whole
		// day, so there is no one height to be drawn to.
		snap: (settings) =>
			typeof settings.dayHeight === 'number' ? { height: [oneDay(settings)] } : {},
		legible: (settings) => ({ width: 240, height: oneDay(settings) })
	},
	// `reversed` (da-gvdc): the newest entry first, and the view pinned to the top.
	repl: {
		meta: meta('repl', 'repl', { reversed: false }, [{ key: 'reversed', label: 'newest first', type: 'boolean' }]),
		component: ReplWidget,
		help: [
			'Every command run on this page and what datum answered, the newest last.',
			'Tap an entry to select it as an id pill; the bottom bar is its prompt.'
		].join('\n'),
		props: ({ reversed }) => ({ reversed: reversed === true }),
		legible: { width: 200, height: 71 }
	},
	// How far back nowview reaches is its setting, which its − / + step
	// (da-pcis); `at` centres it on an instant, which the time cursor
	// overrides while it is set (NowviewWidget.svelte). `orientation` is
	// the dayview's pivot (da-78evn).
	nowview: {
		meta: meta('nowview', 'nowview', { history: NOWVIEW_HISTORY, orientation: 'horizontal' }, [
			{
				key: 'history',
				label: 'history',
				type: 'choice',
				choices: NOWVIEW_STEPS.map((value) => ({ value, label: value === '0' ? 'now' : value }))
			},
			ORIENTATION
		]),
		component: NowviewWidget,
		help: [
			'What each field is doing now, and how it got there; − / + reach further back.',
			'Tap for a -t time pill; drag to pan; hold and drag, or drag left from now, for a span.',
			'Hold still, or double tap, to select the entry under your finger.'
		].join('\n'),
		props: ({ history, at, orientation }) => ({
			history: typeof history === 'string' ? history : NOWVIEW_HISTORY,
			at: typeof at === 'string' ? at : undefined,
			vertical: orientation === 'vertical'
		}),
		onChange: (settings, history) => ({ ...settings, history }),
		// Turned on its side with the picture: the time axis wants the 160,
		// the lanes across it the 108.
		legible: ({ history, orientation }) => {
			const along = history === '0' ? SNAPSHOT_WIDTH : 160;
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
		help: [
			'The newest entries, as `datum tail` lists them; scroll up for older ones.',
			'Tap a row to select it as an id pill; ctrl or shift adds more.',
			'A field pill narrows it to that field.'
		].join('\n'),
		props: (settings) => ({ spec: TAIL, scale: tailScale(settings) }),
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
		help: [
			'Any datum command you choose, run again whenever something changes.',
			'✎ in its title edits the command; a command that writes runs only on ▶.'
		].join('\n'),
		legible: { width: 200, height: 102 }
	},
	// The commands widget (da-ltux): every command as a button, Undo
	// first. It took the id `command` back from the widget above, which
	// da-zqop renamed to `custom` to free it. Legible at one button row
	// (33px) plus the body's padding — the scroll rule takes it from
	// there, and one row reached sideways is still every command.
	command: {
		meta: meta('command', 'command'),
		component: CommandPane,
		help: [
			'Every datum command as a button, in sections: Undo (↷ redoes) and Help, the verbs, setup, edits, analysis, db.',
			'A verb goes on the command line and filters the fields; a double outline runs at once.',
			'The pills you have loaded hide the commands that cannot take them.'
		].join('\n'),
		legible: { width: 160, height: 49 }
	},
	// The stats widget (da-n7r0): its settings ARE the widget's settings, and
	// its range chips move them with a `change` like any other widget's own
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
		help: [
			'How much of each field there is — entries, time, share — over the window chosen.',
			'The chips choose the window; a field pill narrows it to that field\'s states.'
		].join('\n'),
		props: ({ range }) => ({ settings: { range: (range ?? DEFAULT_STATS.range) as StatsRange } }),
		onChange: (_, settings) => settings as Record<string, unknown>,
		legible: { width: 240, height: 140 }
	},
	// The fields widget (da-c7u8), titled `fields`. The id `widgetbar` is the
	// saved-layout key — every stored layout and draft names it, so it stays;
	// the widget is called `fields` everywhere else (da-850j). How a switch in
	// a state is labelled is one setting (`SwitchLabel` in fields.ts);
	// `showIgnored` (da-xhio) is the other, and it is the SESSION's — a way
	// to see the ignored fields the bar leaves out, alongside the (+) and
	// not in place of it, that a reload starts back at off.
	widgetbar: {
		meta: meta('widgetbar', 'fields', { align: 'rows', switchLabel: 'field', showIgnored: false }, [
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
				label: 'a switch is labelled with its',
				type: 'choice',
				choices: [
					{ value: 'field', label: 'field' },
					{ value: 'state', label: 'state' }
				]
			},
			{ key: 'showIgnored', label: 'show ignored fields', type: 'boolean' }
		]),
		component: FieldsPane,
		help: [
			'Every field you log, as a button, lit when it is on.',
			'Tap to log it, with any pending verb and pills; hold for a field pill.',
			'+ declares a new field.'
		].join('\n'),
		props: ({ align, switchLabel, showIgnored }) => ({
			align: align === 'picture' ? 'picture' : 'rows',
			switchLabel: switchLabel === 'state' ? 'state' : 'field',
			showIgnored: showIgnored === true
		}),
		legible: { width: 160, height: 49 }
	},
	// The aliases widget (da-x6yj): every alias, listed.
	aliases: {
		meta: meta('aliases', 'aliases'),
		component: AliasesWidget,
		help: [
			'Every alias: your own, then the ones a spec gives its field.',
			'An alias is a word that expands into a command: `datum help alias`.'
		].join('\n'),
		legible: { width: 160, height: 72 }
	},
	// The sync widget (da-x4t0a, da-yoef7): the page's database's devices,
	// a line each, then the other databases, then the acts. Legible at a
	// device's line wrapped once, two lines more and the acts' row, plus
	// padding.
	sync: {
		meta: meta('sync', 'sync'),
		component: SyncWidget,
		help: [
			'Every device syncing this database — each CLI install and browser — a line each, this one first, with when it was last seen; then your other databases, a tap each to switch.',
			'✓ synced, ↻ syncing, ◔ pending, ! error (its reason under it), — not syncing.',
			'A device not reachable now is drawn grey, as it was when last seen.'
		].join('\n'),
		legible: { width: 140, height: 100 }
	},
	// The Help place (da-z955f): the Help button's last answer ($lib/help.ts).
	// Legible at the title line and two lines of the explanation (18.4px
	// each) plus padding; the rest scrolls.
	help: {
		meta: meta('help', 'help'),
		component: HelpWidget,
		help: [
			'What the Help button was last asked about, explained.',
			'Press Help, then tap a command, a widget or a pill; Help again for general help.'
		].join('\n'),
		legible: { width: 200, height: 72 }
	},
	// «Backups» (da-9ow8m), a place (da-ea5q2): the gear's Backups and the
	// commands widget's `db backup` / `db restore` open it (`openPlace`). Legible
	// at its heading over the two file buttons, stacked (88px measured; the
	// wider is 193px), plus padding: side by side they want 388px.
	backups: {
		meta: meta('backups', 'backups'),
		component: BackupsWidget,
		help: [
			'Backups: a file on this device, saved or restored, and plans that back up to a bucket while a tab is open.',
			'Each plan lists its backups by database, newest first, each with Restore.'
		].join('\n'),
		legible: { width: 206, height: 99 }
	},
	// The field widget (da-fu29p): where a field is made and changed — the
	// Track dialog and the spec widget (da-hbrfg, kind `spec`: `RETIRED_KINDS`
	// in shell.ts) as one. Which field it is on is `editing`
	// ($lib/fieldWidget.ts), every copy's, so it has no setting. Legible at
	// its sentence, a row of chips, the form's name and its Done row, with
	// their gaps and padding — about 180px, estimated from the 28px controls
	// rather than measured; a pane shorter scrolls the widget.
	field: {
		meta: meta('field', 'field'),
		component: FieldWidget,
		help: [
			'A field is one thing you track. Make one here, or pick one to change: its kind, states, keys, aliases and placement.',
			'Save makes a new field in one `track`; Done saves a change in one `spec set`; Cancel leaves it as it was.'
		].join('\n'),
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
		help: [
			'The command line, in a pane: the same line, pills and Enter as the bottom bar.',
			'Above it, what the last command answered.'
		].join('\n'),
		props: ({ reversed }) => ({ reversed: reversed === true }),
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
		'How the bottom bar feels, as live knobs: TAU and DETENT. Only in the DEBUG database.'
	),
	// The second (da-6ecm5): a plausible week of sample data, seeded
	// into the database it runs in. Legible at its two buttons and a line
	// of report; the log under them scrolls.
	seed: debugKind(
		'seed',
		{ width: 200, height: 60 },
		'Seeds a plausible week of sample data into this database. Only in the DEBUG database.'
	),
	// What the shell shows in a tab whose layout cannot be inlined (da-0a5o).
	notice: {
		meta: meta('notice', 'notice'),
		component: Notice,
		hidden: true,
		help: 'A tab whose layout could not be drawn here, and why.',
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

export const titleOf = (kind: string): string => KINDS[kind]?.meta.title ?? kind;

/**
 * What a tab is CALLED, wherever it is shown: the root bar's tabs, a pane
 * frame's title and its tab strip all read this one rule (da-9puz).
 *
 * The layout a tab holds is its name — a tab carrying `layout` is one
 * thing with an identity, and wears it. Failing that its own `title`, and
 * failing that what it shows: the widget kind's title, or for a split the
 * names of its panes, `tail | nowview` (`nameOf`, tree.ts, da-6ji5).
 */
export const labelOf = (tab: Tab): string => nameOf(tab, titleOf);

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
