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
//   layout that SHIPPED — with datum (da-luwe) or in a kit (da-gg0v) —
//   which is a fact about the layout and not about which of the two menus
//   is open.
// - `Reset widget` (da-7ymx) and `Options…` (da-gvdc) are on a tab that
//   SHOWS ONE WIDGET and whose kind declares defaults, or options, to
//   offer. That is a question of the tab's shape, not of whether it names
//   a layout: a saved layout of exactly one widget is lifted into the tab
//   that holds it (`inline`, shell.ts), so the root tab of `datum.day/repl`
//   or `/nowview` is that widget and gets both (da-ze1x). A layout of
//   several widgets arrives under a split and gets neither — its panes
//   have their own. Same rule either menu, as ever.
// - `export backup…` is the root's alone: there is one database. So is
//   `Keep tab` (da-xamd), which rides in beside it: only a root tab can be
//   a PREVIEW, and only a preview has a keeping left to do.
//
// Close is the LAST item of the list, whatever else the caller hands in,
// and wears an `×` (da-ygc9): the one destructive act, at the far end of
// the list from `Change Widget` and marked as itself.
//
// `layout-menu.test.mjs` asserts exactly that arithmetic, so any OTHER
// divergence between the two fails.
//
// THE LIST IS DRAWN IN TWO SLICES (da-bzhq). Everything above is about
// which items a caller HAS; where they are DRAWN is `named` and
// `managed`, at the bottom of this file. The name of the thing — a pane's
// title, a tab, the root tab — opens what it shows and what it is saved
// as; a `▾` beside the `+` opens what is done to the pane and its tabs.
// One builder, one order, two filters: an item moves between the two
// menus by gaining or losing `manage`, and can be in neither list twice.

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
	/**
	 * A WIDGET/PANE MANAGEMENT act, and so an item of the `▾` beside the
	 * `+` rather than of the menu the thing's NAME opens (da-bzhq). See
	 * `named` and `managed` below: one list, one order, two slices.
	 */
	manage?: true;
	/**
	 * One of a set the menu is CHOOSING BETWEEN, and whether this is the
	 * one — the logo's databases (da-y992). Present makes the item a
	 * `menuitemradio` carrying its own `aria-checked`, which is what the
	 * keyboard and a screen reader read the choice off; absent leaves an
	 * ordinary `menuitem`, which is every other item in the app.
	 */
	checked?: boolean;
};

/**
 * The four arrows, in the one order — h j k l, LEFT DOWN UP RIGHT (vividn,
 * 2026-09-21: "split and breakout arrows should be in vim key order left
 * down up right") — and where each one puts a pane on the screen. This
 * list is the DOM's order as well as the eye's, so the ArrowLeft /
 * ArrowRight walk along a row reads the same way round as the glyphs do.
 *
 * A rotated bar does not turn them: the menu is drawn on `document.body`
 * and reads the screen's way (PaneMenu.svelte).
 */
export const ARROWS: { toward: Toward; text: string; where: string }[] = [
	{ toward: 'left', text: '←', where: 'to the left' },
	{ toward: 'down', text: '↓', where: 'below' },
	{ toward: 'up', text: '↑', where: 'above' },
	{ toward: 'right', text: '→', where: 'to the right' }
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
	/**
	 * Undefined where the lock is drawn ELSEWHERE, which is the root bar:
	 * vividn, 2026-09-21, "move the lock button outside of the layout menu
	 * and put it by the gear icon to be more discoverable" (da-bzhq). A
	 * PANE's menu keeps it — there is no gear beside a pane, and a layout
	 * tab that is itself locked has to keep a way to unlock (da-crp5).
	 */
	toggleLock?: () => void;
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
	...(a.toggleLock ? [{ label: a.locked ? 'Unlock layout' : 'Lock layout', run: a.toggleLock }] : [])
];

/**
 * `Options…`, where there is a modal to open. A LOCKED menu keeps it
 * (da-gvdc): a lock stops the layout being split, tabbed or resized, and a
 * widget's own controls change its settings under one — nowview's `+` does
 * — so the modal, being the same setting, does too.
 */
export const optionsItem = (run?: () => void): Item[] => (run ? [{ label: 'Options…', run }] : []);

/** The glyph Close wears (da-ygc9) — the multiplication sign, not the letter x. */
export const CLOSE = '\u00d7';

/**
 * What the item that opens the one chooser is CALLED (da-bzhq). vividn,
 * 2026-09-21: «Let's change Choose... to "Change Widget"». The same words
 * on the chooser's own heading and on an empty pane's button, so the
 * press and what it opens agree (Chooser.svelte, Layout.svelte).
 *
 * ⚑ The dialog also offers LAYOUTS (its `Layouts` group), so this name is
 * narrower than what it opens. Shipped as asked and flagged rather than
 * renamed to a third thing.
 */
export const CHANGE = 'Change Widget';

/**
 * The menu, in the one order — Close last (da-ygc9). Every optional part
 * is left out when the thing it belongs to has no such part — never
 * reordered, never renamed.
 */
export function menuItems(spec: {
	choose: () => void;
	/**
	 * `Reset widget` (da-7ymx): the widget in front, back to the defaults
	 * its kind declares. Undefined — and so left out — when the tab shows
	 * no one widget (a split, and so a layout of more than one) or its kind
	 * declares no defaults, which is `resetWidget` in `shell.ts` asking the
	 * registry.
	 * Beside `Change Widget`, because like it it is about the widget in front.
	 */
	reset?: () => void;
	/**
	 * `Options…` (da-gvdc): the widget in front's options modal, after
	 * `Reset widget`. Undefined when its kind declares no options
	 * (`widgetOptions` in `options.ts`).
	 */
	options?: () => void;
	/**
	 * `Rename…` (da-6ji5, da-s7sdm): the name the thing wears, typed. Only
	 * where the tab holds no layout — a layout tab wears its layout's name.
	 *
	 * The caller names it, the way it names `close`, because the same act
	 * is called after the same thing: `Rename tab…` in a strip, `Rename
	 * pane…` where the pane's one tab IS the pane and its name is the one
	 * on the frame.
	 */
	rename?: { label: string; run: () => void };
	split?: Aimed;
	breakOut?: Aimed;
	close: { label: string; disabled?: boolean; run: () => void };
	bar?: Item;
	acts?: Acts;
	/** `Save as layout…` (da-6ji5): a tab holding a split that is nobody's layout yet. */
	saveAs?: () => void;
	/** The root's own, appended: `export backup…`. */
	extra?: Item[];
}): Item[] {
	return [
		{ label: CHANGE, run: spec.choose },
		...(spec.reset ? [{ label: 'Reset widget', run: spec.reset }] : []),
		...optionsItem(spec.options),
		...(spec.rename ? [{ label: spec.rename.label, run: spec.rename.run, manage: true as const }] : []),
		...(spec.split ? [{ label: 'Split', row: aimed(spec.split.what, spec.split.run), manage: true as const }] : []),
		...(spec.breakOut
			? [{ label: 'Break out', row: aimed(spec.breakOut.what, spec.breakOut.run), manage: true as const }]
			: []),
		...(spec.bar ? [{ ...spec.bar, manage: true as const }] : []),
		...(spec.acts ? layoutActs(spec.acts) : []),
		...(spec.saveAs ? [{ label: 'Save as layout…', run: spec.saveAs }] : []),
		...(spec.extra ?? []),
		{ label: spec.close.label, glyph: CLOSE, disabled: spec.close.disabled, run: spec.close.run, manage: true as const }
	];
}

/**
 * The slice the thing's NAME opens: what it SHOWS and what it is SAVED as
 * — Change Widget, Reset widget, Options…, the layout's own acts, the
 * root's `export backup…`.
 */
export const named = (items: Item[]): Item[] => items.filter((item) => !item.manage);

/**
 * The slice the `▾` beside the `+` opens (da-bzhq): what is done to the
 * PANE and its TABS — Rename…, Split, Break out, Bar, Close.
 *
 * vividn, 2026-09-21: "next to the + tab button, lets put a little down
 * arrow button, and put all the widget/pane management options there
 * (Split, Breakout, etc)." The `+` adds a tab; the `▾` beside it is the
 * rest of what can be done to the strip and the pane, which is why the
 * two hide together under a lock.
 */
export const managed = (items: Item[]): Item[] => items.filter((item) => item.manage);
