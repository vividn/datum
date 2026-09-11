// The widget kinds: what the tree can name, and what draws each one.
//
// Everything on the page is a widget (vividn: "They are widgets like
// anything else yes") — the day strip, the REPL and the quick-log bar
// included. A kind is its `WidgetKindMeta` (the contract in types.ts:
// title, floors, default settings) plus the Svelte component that draws
// it and, where the component wants something other than `settings`, the
// props to hand it. The picker lists `PICKABLE`; the tree's empty tab
// (`EMPTY_KIND`, tree.ts) is not a kind here — Layout.svelte draws the
// picker for it.
//
// Floors are content-box pixels, measured on the built app (2026-09-10,
// Chromium, 0.85rem monospace): the pane's body at the size below which
// the kind stops being legible. The strip: today's band at the default
// day height plus datum's margins and its hour axis (`todayBox(85)`,
// 104px) plus the body's 16px of padding — a taller day is more strip,
// and the strip scrolls in a box shorter than its one band. The REPL: its prompt line (35px) and three lines of
// scrollback (18.4px each) plus padding, 106px. The tail, and a command
// pane showing one: its header row and three rows (21.6px each) plus
// padding, 102px. The
// bar: one row of chips (29px) plus padding. nowview: datum draws it no
// shorter than a 50px band plus its axis, 92px of picture (da-wb2n), plus
// padding. Widths are where each stops
// making sense rather than where it stops rendering: an hour of the strip
// per 10px, a CLI line, the tail's four columns, four chips. The frame's
// own chrome (title bar, border) is added by the renderer, not counted.
//
// `fit`: the pane takes the height its content asks for rather than a
// share of the split — the bar, whose height is however many rows its
// chips wrap to (da-vyo2).

import type { ComponentType } from 'svelte';
import type { WidgetKindMeta } from './types.js';
import CliPane from '$lib/CliPane.svelte';
import CommandPane from '$lib/CommandPane.svelte';
import DayStrip from '$lib/DayStrip.svelte';
import Stats from '$lib/Stats.svelte';
import { DEFAULT_DAY_HEIGHT, NOWVIEW_HISTORY, TAIL } from '$lib/panes.js';
import { DEFAULT_STATS } from '$lib/stats.js';
import BarWidget from './BarWidget.svelte';
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
	/** Content-sized along a column: see the note above. */
	fit?: boolean;
}

const meta = (
	kind: string,
	title: string,
	minWidth: number,
	minHeight: number,
	defaultSettings: Record<string, unknown> = {}
): WidgetKindMeta => ({ kind, title, minWidth, minHeight, defaultSettings });

export const KINDS: Record<string, WidgetKind> = {
	// The strip fills its box and shows as much of the past as fits above
	// today (da-zb0d); how tall a day is drawn is its one setting, which
	// its notch changes.
	daystrip: {
		meta: meta('daystrip', 'dayview', 240, 120, { dayHeight: DEFAULT_DAY_HEIGHT }),
		component: DayStrip,
		props: ({ dayHeight }) => ({
			dayHeight: typeof dayHeight === 'number' ? dayHeight : DEFAULT_DAY_HEIGHT
		}),
		onChange: (settings, dayHeight) => ({ ...settings, dayHeight })
	},
	repl: { meta: meta('repl', 'repl', 200, 106), component: ReplWidget },
	// How far back nowview reaches is its setting, which its − / + step
	// (da-pcis); `at` centres it on an instant, which nothing sets yet.
	nowview: {
		meta: meta('nowview', 'nowview', 160, 108, { history: NOWVIEW_HISTORY }),
		component: NowviewWidget,
		props: ({ history, at }) => ({
			history: typeof history === 'string' ? history : NOWVIEW_HISTORY,
			at: typeof at === 'string' ? at : undefined
		}),
		onChange: (settings, history) => ({ ...settings, history })
	},
	tail: { meta: meta('tail', 'tail', 200, 102), component: CliPane, props: () => ({ spec: TAIL }) },
	// The command pane (da-wb2n) keeps its line in its own localStorage
	// key for now; making it a widget setting is its own change.
	command: { meta: meta('command', 'command', 200, 102), component: CommandPane },
	// The stats widget (da-n7r0): its settings ARE the widget's settings —
	// the tree keeps them, and a `change` from the widget writes them back
	// (Layout.svelte).
	stats: {
		meta: meta('stats', 'stats', 240, 140, { ...DEFAULT_STATS }),
		component: Stats,
		props: (settings) => ({ settings: { ...DEFAULT_STATS, ...settings } }),
		onChange: (_, settings) => settings as Record<string, unknown>
	},
	widgetbar: { meta: meta('widgetbar', 'widgets', 160, 45), component: BarWidget, fit: true }
};

/** The kinds the picker offers, in the order it shows them. */
export const PICKABLE = Object.keys(KINDS);

export const titleOf = (kind: string): string => KINDS[kind]?.meta.title ?? kind;

/** A kind's floor; an unknown kind (a draft from a newer app) has none. */
export const floorOfKind = (kind: string) => {
	const m = KINDS[kind]?.meta;
	return { width: m?.minWidth ?? 0, height: m?.minHeight ?? 0 };
};
