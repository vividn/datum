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
// - the layout's own acts — save, restore, save as — are on any tab
//   that HOLDS a layout (da-crp5), which the root's always does and a
//   pane's sometimes does; `Reset layout` joins them on a tab holding a
//   layout that SHIPPED — with datum (da-luwe) or in a kit (da-gg0v) —
//   which is a fact about the layout and not about which of the two menus
//   is open.
// - `Reset widget` (da-7ymx) and the widget's OWN OPTIONS, one item each
//   (da-vhwq, `options.ts`), are on a tab that SHOWS ONE WIDGET and whose
//   kind declares defaults, or options, to offer. That is a question of
//   the tab's shape, not of whether it names a layout: a saved layout of
//   exactly one widget is lifted into the tab that holds it (`inline`,
//   shell.ts), so the root tab of `datum.day/repl` or `/nowview` is that
//   widget and gets both (da-ze1x). A layout of several widgets arrives
//   under a split and gets neither — its panes have their own. Same rule
//   either menu, as ever.
// - `Keep tab` (da-xamd) is the root's alone: only a root tab can be a
//   PREVIEW, and only a preview has a keeping left to do. (`export
//   backup…` rode here once; it is the gear's «Backups» now, da-9ow8m.)
//
// Close is the LAST item of the list, whatever else the caller hands in,
// and wears an `×` (da-ygc9): the one destructive act, at the far end of
// the list from `Change Widget` and marked as itself.
//
// `layout-menu.test.mjs` asserts exactly that arithmetic, so any OTHER
// divergence between the two fails.
//
// THE LIST IS DRAWN TWO WAYS, by MODE (da-vvcxc). Everything above is
// about which items a caller HAS; how they are DRAWN is `named` and
// `arranged`, at the bottom of this file. In normal mode the name of the
// thing — a pane's title, a tab — opens what it IS (`named`): the layout it
// holds and what that is saved as, and the settings of the widget in front.
// In layout mode the same name opens what is DONE to the pane — which
// widget sits in it, its name, Split, Break out, Close — with what it IS
// one step in, under `Widget options ›` (`arranged`). There is no `▾`
// (vividn, 2026-10-14: «no more down arrow button at all»), and no `Bar`:
// the maximize badge is the bar's handle in layout mode (PaneFrame.svelte).

import type { ActKind } from '../act.js';
import type { Toward } from './tree.js';

/**
 * A submenu of WORDS (da-vhwq): one of a set of named values, the one in
 * force marked — a widget option of type `choice`, drawn where a modal's
 * `<select>` was. `title` names it in full where the label is a value on
 * its own («now», «45m»).
 */
export type Pick = {
	label: string;
	title?: string;
	checked: boolean;
	/** One of a SET (an `offered` option with `many`): a switch, which leaves the submenu open. */
	toggled?: boolean;
	run: () => void;
};
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
	/** Opens a submenu of words: `Pick` above. */
	list?: Pick[];
	row?: Short[];
	disabled?: boolean;
	marked?: boolean;
	/**
	 * What its tap does, for the outline (`$lib/act.ts`, rule 22): `run`
	 * where it WRITES at once, which draws the double (da-kk0vr); `open`
	 * where it opens a modal, a sheet or a confirm, which wears the `›`
	 * (da-vteda). Absent everywhere else.
	 */
	act?: ActKind;
	/**
	 * A WIDGET/PANE MANAGEMENT act, and so an item of the layout-mode menu
	 * rather than of the normal one (da-vvcxc). See `named` and `arranged`
	 * below: one list, one order, two ways to draw it.
	 */
	manage?: true;
	/**
	 * A menu one step in (da-vvcxc): choosing the item shows these in the
	 * menu's place — layout mode's `Widget options ›`, which holds what the
	 * name opens in normal mode.
	 */
	items?: Item[];
	/**
	 * One of a set the menu is CHOOSING BETWEEN, and whether this is the
	 * one — the logo's databases (da-y992). Present makes the item a
	 * `menuitemradio` carrying its own `aria-checked`, which is what the
	 * keyboard and a screen reader read the choice off; absent leaves an
	 * ordinary `menuitem`, which is every other item in the app.
	 */
	checked?: boolean;
	/**
	 * A SWITCH, and whether it is on (da-vhwq): a widget option of type
	 * `boolean`, changed by pressing the item itself. Present makes the
	 * item a `menuitemcheckbox` — a thing turned on and off, which is not
	 * what `checked` above means (one of a set).
	 */
	toggled?: boolean;
	/**
	 * A sentence and not an act: drawn as words that wrap, not a button,
	 * and walked past by the keyboard — the gear's word on the beta
	 * (da-eue7y), under the version it is about.
	 */
	note?: true;
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
	save: () => void;
	restore: () => void;
	/** Back to the tree that SHIPPED under this name — only a layout that HAS one (da-luwe). */
	reset?: () => void;
	saveAs: () => void;
};

/**
 * The acts on the layout a tab holds (da-crp5).
 *
 * While the layout is MODIFIED, Save and Restore are marked (vividn: "Maybe
 * highlight Restore and Save so that it is obvious that that is what the
 * orange dot is referring to") — the dot's meaning, in the same place as
 * the acts that answer it.
 *
 * `Reset layout` sits between them and `Save layout as`: the two ways
 * BACK together (da-luwe), Restore to the save and Reset to what shipped.
 * It is the only one that is not always there — a layout the reader made
 * themselves has no shipped tree to go back to, so its menu has four
 * items and a built-in's has five.
 */
export const layoutActs = (a: Acts): Item[] => [
	{ label: 'Save layout', marked: a.dirty, act: 'run' as const, run: a.save },
	{ label: 'Restore layout', marked: a.dirty, run: a.restore },
	...(a.reset ? [{ label: 'Reset layout', run: a.reset }] : []),
	{ label: 'Save layout as', act: 'open' as const, run: a.saveAs }
];

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
 *
 * It is a `manage` item, so it is in layout mode's menu (da-z8klv,
 * da-vvcxc): WHICH widget sits in a pane is a thing done to the pane,
 * beside Split and Close, and not part of what the pane is called.
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
	 * The first of the NAME's slice: `Change Widget` swaps the widget out
	 * and is layout mode's (da-z8klv), this one is about the widget in front.
	 */
	reset?: () => void;
	/**
	 * The widget in front's own options, one item each, after `Reset
	 * widget` (da-vhwq): `widgetOptions` in `options.ts` builds them from
	 * the registry's declarations and this session's settings, and ends
	 * them with «Save options with layout» where there is something to
	 * save and the layout is unlocked. Empty where the tab shows no one
	 * widget, or its kind declares nothing and nothing is held.
	 */
	options?: Item[];
	/**
	 * `Rename…` (da-6ji5, da-s7sdm): the name the thing wears, typed. Only
	 * where the tab holds no layout — a layout tab wears its layout's name.
	 *
	 * The caller names it, the way it names `close`, because the same act
	 * is called after the same thing: `Rename tab` in a strip, `Rename
	 * pane` where the pane's one tab IS the pane and its name is the one
	 * on the frame.
	 */
	rename?: { label: string; run: () => void };
	split?: Aimed;
	breakOut?: Aimed;
	close: { label: string; disabled?: boolean; run: () => void };
	acts?: Acts;
	/** `Save as layout` (da-6ji5): a tab holding a split that is nobody's layout yet. */
	saveAs?: () => void;
	/** The root's own, appended: `Keep tab`. */
	extra?: Item[];
}): Item[] {
	return [
		{ label: CHANGE, act: 'open' as const, run: spec.choose, manage: true as const },
		...(spec.reset ? [{ label: 'Reset widget', run: spec.reset }] : []),
		...(spec.options ?? []),
		...(spec.rename ? [{ label: spec.rename.label, act: 'open' as const, run: spec.rename.run, manage: true as const }] : []),
		...(spec.split ? [{ label: 'Split', row: aimed(spec.split.what, spec.split.run), manage: true as const }] : []),
		...(spec.breakOut
			? [{ label: 'Break out', row: aimed(spec.breakOut.what, spec.breakOut.run), manage: true as const }]
			: []),
		...(spec.acts ? layoutActs(spec.acts) : []),
		...(spec.saveAs ? [{ label: 'Save as layout', act: 'open' as const, run: spec.saveAs }] : []),
		...(spec.extra ?? []),
		{ label: spec.close.label, glyph: CLOSE, disabled: spec.close.disabled, run: spec.close.run, manage: true as const }
	];
}

/**
 * What the thing's NAME opens in normal mode: what it IS and what it is
 * SAVED as — `Reset widget`, the widget's own options, the layout's own
 * acts, the root's `Keep tab`. A pane whose widget declares neither
 * defaults nor options, holds nothing in this session and holds no layout,
 * has none of them and wears no title menu at all (da-z8klv).
 */
export const named = (items: Item[]): Item[] => items.filter((item) => !item.manage);

/** The item layout mode puts what the name opens in normal mode under. */
export const OPTIONS = 'Widget options';

/**
 * What a pane's NAME opens in layout mode (da-vvcxc). vividn, 2026-10-14:
 * «change widget / rename pane / … / Widget Options > (rather than being
 * the primary menu, this goes in a secondary menu when in layout mode /
 * Close Pane», and of Split and Break out «still include the split and
 * break out in the menu as before, but of course only in layout mode». So:
 * the management acts in their one order, then `Widget options ›` holding
 * `named` where it has anything, then Close, last as ever.
 */
export function arranged(items: Item[]): Item[] {
	const acts = items.filter((item) => item.manage);
	const own = named(items);
	return [
		...acts.slice(0, -1),
		...(own.length ? [{ label: OPTIONS, items: own }] : []),
		...acts.slice(-1)
	];
}
