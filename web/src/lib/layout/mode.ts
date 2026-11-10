// MODIFY MODE (da-vvcxc, da-00c29). vividn, 2026-10-14: «long pressing … or
// right clicking on a primary tab name enters unlocked/layout mode. This
// should be very visually distinctive.» It replaces the per-device lock and
// every `▾`: outside it nothing the reader does moves the tree; inside it
// every pane is worked from its name, and the widgets do nothing else.
//
// It ends two ways (vividn, 2026-11-10: «show a check button on its right
// and a revert (circle arrow) button on it's left, which applies the changes
// or resets respectively», «1. only apply», «yes to esc and enter»). APPLY
// (✓, Enter) keeps the change on this device, unsaved: the draft `settle`
// (shell.ts) has been writing all along, so a reload keeps it, the tab wears
// the orange dot, and its menu's Save is what writes it into the database.
// RESET (↺, Escape) puts the page back as it was when the mode was entered.
// Both are on the modified tab and in the input bar's «modify mode» pill.
//
// ⚑ The draft is written as the edit is made, as it always was, so a reload
// in the middle of the mode keeps the edit, as Apply would.

//
// ONE MODE AT TWO SCOPES (da-k5hu5, vividn, 2026-11-10: «individual widgets
// should have a modify mode too, which you can access by tapping and holding
// on the widget title, or an option "Modify" in the widget menu», «2. yes
// exactly»): the tab's, entered on the root tab, and a widget's, entered on
// its pane's title, where that pane alone is live and wears ↺ and ✓. A
// widget scope whose pane goes (Close, Break out) widens to the tab's.

import { get } from 'svelte/store';
import { editing, layout } from './current.js';
import { findPane } from './tree.js';
import type { NodeId } from './types.js';

/** The two ways out, drawn the same on the tab and in the bar. */
export const APPLY = '✓';
export const RESET = '↺';

/** Enter modify mode on the root tab in front, or on one widget's pane of it. */
export function enter(pane?: NodeId): void {
	if (get(editing)) return;
	const now = get(layout);
	const tab = now.root.tabs[now.root.active];
	if (tab) editing.set({ tab: tab.id, before: now, ...(pane && { pane }) });
}

layout.subscribe((now) => {
	const e = get(editing);
	if (e?.pane && !findPane(now, e.pane)) editing.set({ tab: e.tab, before: e.before });
});

/** ✓: leave the mode, keeping the change on this device. */
export const accept = (): void => editing.set(null);

/** ↺: leave it with the page as it was when it was entered. */
export function discard(): void {
	const e = get(editing);
	if (!e) return;
	layout.set(e.before);
	editing.set(null);
}

/**
 * Escape resets and Enter applies, after the dispatch, as PromptBar.svelte
 * orders Escape: a key a menu, a dialog or a control has taken is theirs.
 * Enter is the page's only where no control has the focus — the ✓ has it
 * as the mode is entered — so Enter on a menu item or a Split arrow is
 * still that control's.
 */
export function onKey(event: KeyboardEvent): void {
	if (!get(editing) || document.querySelector('dialog[open]')) return;
	const free = event.target === document.body || event.target === document.documentElement;
	if (event.key !== 'Escape' && !(event.key === 'Enter' && free)) return;
	setTimeout(() => {
		if (event.defaultPrevented || !get(editing)) return;
		event.preventDefault();
		if (event.key === 'Escape') discard();
		else accept();
	});
}
