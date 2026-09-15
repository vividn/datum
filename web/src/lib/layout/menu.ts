// The ONE menu (da-wjob): what a pane's title opens and what the root
// bar's active tab opens are the same items, built here.
//
// vividn: the top bar "is actually just a specially formatted version of the
// same thing — make sure it is really completely aligned with the other
// widget picker, just formatted differently". So the difference between
// the two menus is not a second list written out somewhere else: it is
// WHICH PARTS a caller hands to `menuItems`, and everything a caller does
// hand in lands with the same label in the same order. What each menu is
// missing is a fact about the thing it belongs to:
//
// - the ROOT pane is the shell, so there is no strip for a tab to leave
//   and no `Break out`; its `Split` is `addEdge`, a new pane down one
//   whole side of the layout in front — the only thing a split beside the
//   shell can mean (da-a642).
// - the root draws no bar of its own (PaneFrame.svelte's `bare`), so `Bar`
//   means nothing there and is not offered.
// - the layout's own acts — save, restore, save as, lock — are on any tab
//   that HOLDS a layout (da-crp5), which the root's always does and a
//   pane's sometimes does; `Reset layout` joins them on a tab holding a
//   layout that SHIPPED with datum (da-luwe), which is a fact about the
//   layout and not about which of the two menus is open.
// - `Reset widget` (da-7ymx) is on a tab that HOLDS A WIDGET of its own —
//   not a layout, not a split — and whose kind declares defaults to put
//   back. The root's tab almost always holds a layout, so the root menu
//   almost never shows it; that is the same rule, not a second one.
// - `export backup…` is the root's alone: there is one database.
//
// Close is the LAST item of either menu, whatever else the caller hands
// in, and wears an `×` (da-ygc9): the one destructive act, at the far end
// of the list from `Choose…` and marked as itself.
//
// `layout-menu.test.mjs` asserts exactly that arithmetic, so any OTHER
// divergence between the two fails.

import type { Toward } from './tree.js';

/** A submenu's button: `icon` is the thick bar `[x, y, w, h]` drawn in a 16px square. */
export type Choice = { label: string; title: string; icon: number[]; checked: boolean; run: () => void };
/** A row's button: `text` is what it shows, `title` its name. */
export type Short = { text: string; title: string; run: () => void };
/**
 * One item of a menu. `marked` is the act to notice now — see `layoutActs`;
 * `glyph` is a character drawn before the label, which `Close` wears (da-ygc9).
 */
export type Item = {
	label: string;
	glyph?: string;
	run?: () => void;
	sub?: Choice[][];
	row?: Short[];
	disabled?: boolean;
	marked?: boolean;
};

/**
 * The four arrows, in the one order, and where each one puts a pane on the
 * screen — a rotated bar does not turn them, since the menu is drawn on
 * `document.body` and reads the screen's way (PaneMenu.svelte).
 */
export const ARROWS: { toward: Toward; text: string; where: string }[] = [
	{ toward: 'left', text: '←', where: 'to the left' },
	{ toward: 'right', text: '→', where: 'to the right' },
	{ toward: 'up', text: '↑', where: 'above' },
	{ toward: 'down', text: '↓', where: 'below' }
];

/** One menu item's row: the same operation aimed four ways, each named in full. */
export const aimed = (what: string, run: (toward: Toward) => void): Short[] =>
	ARROWS.map(({ toward, text, where }) => ({ text, title: `${what} ${where}`, run: () => run(toward) }));

/** An operation aimed four ways: what it does, named for the titles. */
export type Aimed = { what: string; run: (toward: Toward) => void };

/** The layout a tab holds, and the things that can be done to it. */
export type Acts = {
	/** Whether this copy's tree differs from its save: the orange dot's fact. */
	dirty: boolean;
	locked: boolean;
	save: () => void;
	restore: () => void;
	/** Back to the tree that SHIPPED under this name — only a layout that HAS one (da-luwe). */
	reset?: () => void;
	saveAs: () => void;
	toggleLock: () => void;
};

/**
 * The acts on the layout a tab holds (da-crp5).
 *
 * While the layout is MODIFIED, Save and Restore are marked (vividn: "Maybe
 * highlight Restore and Save so that it is obvious that that is what the
 * orange dot is referring to") — the dot's meaning, in the same place as
 * the acts that answer it.
 *
 * `Reset layout` sits between them and `Save layout as…`: the two ways
 * BACK together (da-luwe), Restore to the save and Reset to what shipped.
 * It is the only one that is not always there — a layout the reader made
 * themselves has no shipped tree to go back to, so its menu has four
 * items and a built-in's has five.
 */
export const layoutActs = (a: Acts): Item[] => [
	{ label: 'Save layout', marked: a.dirty, run: a.save },
	{ label: 'Restore layout', marked: a.dirty, run: a.restore },
	...(a.reset ? [{ label: 'Reset layout', run: a.reset }] : []),
	{ label: 'Save layout as…', run: a.saveAs },
	{ label: a.locked ? 'Unlock layout' : 'Lock layout', run: a.toggleLock }
];

/** The glyph Close wears (da-ygc9) — the multiplication sign, not the letter x. */
export const CLOSE = '\u00d7';

/**
 * The menu, in the one order — Close last (da-ygc9). Every optional part
 * is left out when the thing it belongs to has no such part — never
 * reordered, never renamed.
 */
export function menuItems(spec: {
	choose: () => void;
	/**
	 * `Reset widget` (da-7ymx): the widget in front, back to the defaults
	 * its kind declares. Undefined — and so left out — when the tab holds
	 * no widget of its own (a layout, a split) or its kind declares no
	 * defaults, which is `resetWidget` in `shell.ts` asking the registry.
	 * Beside `Choose…`, because like it it is about the widget in front.
	 */
	reset?: () => void;
	split?: Aimed;
	breakOut?: Aimed;
	close: { label: string; disabled?: boolean; run: () => void };
	bar?: Item;
	acts?: Acts;
	/** The root's own, appended: `export backup…`. */
	extra?: Item[];
}): Item[] {
	return [
		{ label: 'Choose…', run: spec.choose },
		...(spec.reset ? [{ label: 'Reset widget', run: spec.reset }] : []),
		...(spec.split ? [{ label: 'Split', row: aimed(spec.split.what, spec.split.run) }] : []),
		...(spec.breakOut ? [{ label: 'Break out', row: aimed(spec.breakOut.what, spec.breakOut.run) }] : []),
		...(spec.bar ? [spec.bar] : []),
		...(spec.acts ? layoutActs(spec.acts) : []),
		...(spec.extra ?? []),
		{ label: spec.close.label, glyph: CLOSE, disabled: spec.close.disabled, run: spec.close.run }
	];
}
