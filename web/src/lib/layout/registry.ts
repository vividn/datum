// The widget kinds: what the tree can name, and what draws each one.
//
// Everything on the page is a widget (vividn: "They are widgets like
// anything else yes") — the day strip, the REPL and the fields widget
// included. A kind is its `WidgetKindMeta` (the contract in types.ts:
// title and default settings) plus the Svelte component that draws
// it and, where the component wants something other than `settings`, the
// props to hand it. The chooser lists `PICKABLE`; the tree's empty tab
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
// each) plus padding, 102px. The fields widget: one row, as tall as its
// Track + (33px), plus padding. The aliases list: three of its lines
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
import type { Tab, WidgetKindMeta } from './types.js';
import { isSplit } from './types.js';
import type { Floor } from './tree.js';
import CliPane from '$lib/CliPane.svelte';
import CustomPane from '$lib/CustomPane.svelte';
import DayStrip from '$lib/DayStrip.svelte';
import Stats from '$lib/Stats.svelte';
import {
	DEFAULT_DAY_HEIGHT,
	DEFAULT_LABEL_WIDTH,
	MAX_DAY_HEIGHT,
	MIN_DAY_HEIGHT,
	NOWVIEW_HISTORY,
	TAIL,
	todayBox
} from '$lib/panes.js';
import { DEFAULT_STATS } from '$lib/stats.js';
import AliasesWidget from './AliasesWidget.svelte';
import FieldsPane from './FieldsPane.svelte';
import InputWidget from './InputWidget.svelte';
import Notice from './Notice.svelte';
import NowviewWidget from './NowviewWidget.svelte';
import ReplWidget from './ReplWidget.svelte';

export interface WidgetKind {
	meta: WidgetKindMeta;
	component: ComponentType;
	/** Props for the component, when it wants more than `settings`. */
	props?: (settings: Record<string, unknown>) => Record<string, unknown>;
	/**
	 * What the component's `change` event means for its settings: the
	 * detail folded into them. A kind without one has nothing to keep.
	 */
	onChange?: (settings: Record<string, unknown>, detail: unknown) => Record<string, unknown>;
	/** Content-box sizes a border drag snaps to, from the settings: see the note above. */
	snap?: (settings: Record<string, unknown>) => { width?: number[]; height?: number[] };
	/** Not offered by the chooser: a kind the app puts in a tab itself. */
	hidden?: boolean;
	/** The content box the kind is legible in, fixed or from the settings: see the note above. */
	legible: Floor | ((settings: Record<string, unknown>) => Floor);
}

/** The strip body's padding, top and bottom, as its snap counts it. */
const STRIP_PADDING = 16;

/**
 * One whole day at the height the strip's days are DRAWN — today's band
 * with its axis, the setting clamped as the strip clamps it — plus the
 * body's padding.
 */
const oneDay = ({ dayHeight }: Record<string, unknown>): number => {
	const asked = typeof dayHeight === 'number' ? dayHeight : DEFAULT_DAY_HEIGHT;
	return todayBox(Math.min(MAX_DAY_HEIGHT, Math.max(MIN_DAY_HEIGHT, asked))) + STRIP_PADDING;
};

const meta = (
	kind: string,
	title: string,
	defaultSettings: Record<string, unknown> = {}
): WidgetKindMeta => ({ kind, title, defaultSettings });

export const KINDS: Record<string, WidgetKind> = {
	// The strip fills its box and shows as much of the past as fits above
	// today (da-zb0d); how tall a day is drawn is its one setting, which
	// its notch changes.
	daystrip: {
		meta: meta('daystrip', 'dayview', {
			dayHeight: DEFAULT_DAY_HEIGHT,
			labelWidth: DEFAULT_LABEL_WIDTH,
			freezeToday: true
		}),
		component: DayStrip,
		props: ({ dayHeight, labelWidth, freezeToday }) => ({
			dayHeight: typeof dayHeight === 'number' ? dayHeight : DEFAULT_DAY_HEIGHT,
			labelWidth: typeof labelWidth === 'number' ? labelWidth : DEFAULT_LABEL_WIDTH,
			freezeToday: typeof freezeToday === 'boolean' ? freezeToday : true
		}),
		// The strip's `change` names the settings it moved — the `+` handle
		// moves two at once — so the fold is the same one for all of them.
		onChange: (settings, moved) => ({ ...settings, ...(moved as Record<string, unknown>) }),
		// A notch, not a wall: shorter, the strip clips today's band from
		// the top and keeps its axis.
		snap: (settings) => ({ height: [oneDay(settings)] }),
		legible: (settings) => ({ width: 240, height: oneDay(settings) })
	},
	repl: { meta: meta('repl', 'repl'), component: ReplWidget, legible: { width: 200, height: 71 } },
	// How far back nowview reaches is its setting, which its − / + step
	// (da-pcis); `at` centres it on an instant, which the time cursor
	// overrides while it is set (NowviewWidget.svelte).
	nowview: {
		meta: meta('nowview', 'nowview', { history: NOWVIEW_HISTORY }),
		component: NowviewWidget,
		props: ({ history, at }) => ({
			history: typeof history === 'string' ? history : NOWVIEW_HISTORY,
			at: typeof at === 'string' ? at : undefined
		}),
		onChange: (settings, history) => ({ ...settings, history }),
		legible: { width: 160, height: 108 }
	},
	tail: {
		meta: meta('tail', 'tail'),
		component: CliPane,
		props: () => ({ spec: TAIL }),
		legible: { width: 200, height: 102 }
	},
	// The custom widget (da-wb2n): one command line the reader writes. It
	// keeps that line in its own localStorage key for now; making it a
	// widget setting is its own change. Called `command` until da-zqop,
	// which freed that id for the commands widget (da-ltux).
	custom: { meta: meta('custom', 'custom'), component: CustomPane, legible: { width: 200, height: 102 } },
	// The stats widget (da-n7r0): its settings ARE the widget's settings —
	// the tree keeps them, and a `change` from the widget writes them back
	// (Layout.svelte).
	stats: {
		meta: meta('stats', 'stats', { ...DEFAULT_STATS }),
		component: Stats,
		props: (settings) => ({ settings: { ...DEFAULT_STATS, ...settings } }),
		onChange: (_, settings) => settings as Record<string, unknown>,
		legible: { width: 240, height: 140 }
	},
	// The fields widget (da-c7u8), titled `fields`. The id `widgetbar` is the
	// saved-layout key — every stored layout and draft names it, so it stays;
	// the widget is called `fields` everywhere else (da-850j). How a switch in
	// a state is labelled is its one setting (`SwitchLabel` in fields.ts).
	widgetbar: {
		meta: meta('widgetbar', 'fields'),
		component: FieldsPane,
		props: ({ switchLabel }) => ({ switchLabel: switchLabel === 'state' ? 'state' : 'field' }),
		legible: { width: 160, height: 49 }
	},
	// The aliases widget (da-x6yj): every alias, listed.
	aliases: { meta: meta('aliases', 'aliases'), component: AliasesWidget, legible: { width: 160, height: 72 } },
	// The input widget (da-0dhc, back as da-3zon): the bottom bar's command
	// line drawn in a pane — the same buffer, the same pills, the same
	// Enter. Legible at its prompt row (36.4px measured) plus one line of
	// receipt above it (a document line at 1rem/1.35, 21.6px): the least
	// that is still a command line with an answer over it.
	input: { meta: meta('input', 'input'), component: InputWidget, legible: { width: 200, height: 58 } },
	// What the shell shows in a tab whose layout cannot be inlined (da-0a5o).
	notice: { meta: meta('notice', 'notice'), component: Notice, hidden: true, legible: { width: 120, height: 40 } }
};

/** The kinds the chooser offers, in the order it shows them. */
export const PICKABLE = Object.keys(KINDS).filter((kind) => !KINDS[kind].hidden);

export const titleOf = (kind: string): string => KINDS[kind]?.meta.title ?? kind;

/**
 * What a tab is CALLED, wherever it is shown: the root bar's tabs, a pane
 * frame's title and its tab strip all read this one rule (da-9puz).
 *
 * The layout a tab holds is its name — a tab carrying `layout` is one
 * thing with an identity, and wears it. Failing that its own `title`, and
 * failing that what it shows: the widget kind's title, or «split» for a
 * split of panes that is nobody's layout — scaffolding, with no name of
 * its own to wear.
 */
export const labelOf = (tab: Tab): string =>
	tab.layout ?? tab.title ?? (isSplit(tab.content) ? 'split' : titleOf(tab.content.kind));

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
