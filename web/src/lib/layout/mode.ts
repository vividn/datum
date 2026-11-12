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
import { apply, editing, layout } from './current.js';
import { findPane, findTab, setWidget } from './tree.js';
import { isWidget, type NodeId } from './types.js';
import { dropSession, overlay, restoreSession, type Taken } from './session.js';

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
export function accept(): void {
	editing.set(null);
}

/** ↺: leave it with the page as it was when it was entered. */
export function discard(): void {
	const e = get(editing);
	if (!e) return;
	layout.set(e.before);
	for (const [key, settings] of held) restoreSession(key, settings);
	editing.set(null);
}

/**
 * A WIDGET'S OWN CONTROLS IN MODIFY MODE (da-xi4up): collect's now width,
 * flip, rows and pinch. Out of the mode a widget moving a setting of its own
 * writes this session (session.ts); in it the setting is the LAYOUT's, so ↺
 * takes it back and the tab's Save saves it. `fold` is the kind's
 * `onChange`, folded onto the tree's settings read fresh. A setting it
 * changes is dropped from the session too, or the overlay would go on
 * masking it (vremya, 2026-11-10), and ↺ holds it again.
 *
 * Out of the mode it is a widget writing one setting into the layout itself,
 * as «Save options with layout» would that one: the text widget's keep
 * (da-v42x5, vividn, 2026-11-12: «A keep writes the layout»). Nothing is
 * held then, there being no ↺ to give it back to.
 */
const held = new Map<string, Taken>();
/** Forgotten as the mode ends, whatever ends it. */
editing.subscribe((e) => e || held.clear());
export function modifyWidget(
	tabId: NodeId,
	key: string,
	fold: (settings: Record<string, unknown>, moved: unknown) => Record<string, unknown>,
	moved: unknown
): void {
	let changed: string[] = [];
	apply((l) => {
		const now = findTab(l, tabId);
		if (!now || !isWidget(now.content)) return l;
		// A setting moved to nothing is no key (da-1tmd1): the strip's day
		// height dragged back onto its box's follows the box again.
		const settings = Object.fromEntries(Object.entries(fold(now.content.settings, moved)).filter(([, v]) => v !== undefined));
		// What it moved, a setting unset among it, as well as what differs.
		changed = [...new Set([...Object.keys(overlay(now.content.settings, settings)), ...Object.keys(moved ?? {})])];
		return setWidget(l, tabId, { ...now.content, settings });
	});
	const took = dropSession(key, changed);
	if (!get(editing)) return;
	const was = held.get(key);
	if (Object.keys({ ...took.kept, ...took.passing }).length)
		held.set(key, { kept: { ...took.kept, ...was?.kept }, passing: { ...took.passing, ...was?.passing } });
}

/**
 * Escape resets and Enter applies, after the dispatch, as PromptBar.svelte
 * orders Escape: a key a menu, a dialog or a control has taken is theirs.
 * Enter is the page's only where no control has the focus — the ✓ has it
 * as the mode is entered — so Enter on a menu item or a Split arrow is
 * still that control's.
 */
export function onKey(event: KeyboardEvent): void {
	if (!get(editing) || document.querySelector('dialog:modal')) return;
	const free = event.target === document.body || event.target === document.documentElement;
	if (event.key !== 'Escape' && !(event.key === 'Enter' && free)) return;
	setTimeout(() => {
		if (event.defaultPrevented || !get(editing)) return;
		event.preventDefault();
		if (event.key === 'Escape') discard();
		else accept();
	});
}
