// LAYOUT MODE (da-vvcxc). vividn, 2026-10-14: «long pressing … or right
// clicking on a primary tab name enters unlocked/layout mode. This should be
// very visually distinctive.» It replaces the per-device lock and every `▾`:
// outside it nothing the reader does moves the tree; inside it every pane is
// worked from its name, and the open root tab's name ends it.
//
// It ends three ways, and only these. ACCEPT keeps the change on this
// device, where it keeps its open tabs — the draft `settle` (shell.ts) has
// been writing all along, so a reload keeps it (vividn, 18:44: «yes accept
// should keep it on reload») and nothing reaches the database. SAVE writes
// the layout in front into the database, so it reaches every synced device.
// DISCARD puts the page back as it was when the mode was entered. Escape
// asks which (`leave`): «Esc Esc reverts, Esc Enter accepts», and moving to
// another tab with a change in hand asks the same.
//
// ⚑ The draft is written as the edit is made, as it always was, so a reload
// in the middle of the mode keeps the edit, as Accept would.

import { get } from 'svelte/store';
import { choose } from '$lib/confirm.js';
import { editing, layout } from './current.js';
import { save as saveLayout } from './shell.js';

/** Enter layout mode on the root tab in front. */
export function enter(): void {
	if (get(editing)) return;
	const now = get(layout);
	const tab = now.root.tabs[now.root.active];
	if (tab) editing.set({ tab: tab.id, before: now });
}

/** Whether the page differs from what it was when the mode was entered. */
const changed = (): boolean => {
	const e = get(editing);
	return !!e && JSON.stringify(get(layout)) !== JSON.stringify(e.before);
};

/** Leave the mode, keeping the change on this device. */
export const accept = (): void => editing.set(null);

/** Leave it with the page as it was when it was entered. */
export function discard(): void {
	const e = get(editing);
	if (!e) return;
	layout.set(e.before);
	editing.set(null);
}

/** Write the layout in front into the database, and leave the mode. */
export async function save(): Promise<void> {
	const e = get(editing);
	if (!e) return;
	await saveLayout(e.tab);
	editing.set(null);
}

let asking = false;

/**
 * Leave the mode, asking how when there is a change to ask about: Accept,
 * Save layout (a tab holding a layout has one to write), or — Escape, or
 * the dialog's last button, the red one — Discard changes. Accept is the
 * first button, so it has the focus and Enter presses it.
 */
export async function leave(): Promise<void> {
	if (!get(editing) || asking) return;
	if (!changed()) return accept();
	asking = true;
	try {
		// Save is offered only where the tab holds a layout to write, and named only where it is offered.
		const named = get(layout).root.tabs.find((t) => t.id === get(editing)?.tab)?.layout !== undefined;
		const answer = await choose(
			`This layout has changed. Accept keeps the change on this device; ${named ? 'Save layout writes it into the database, which every synced device shares; ' : ''}Discard changes puts it back.`,
			named ? ['Accept', 'Save layout'] : ['Accept'],
			'Discard changes',
			'run',
			// Discard throws the change away: the dialog's one red button (da-ad2lt).
			true
		);
		if (answer === 'Accept') accept();
		else if (answer === 'Save layout') await save();
		else discard();
	} finally {
		asking = false;
	}
}

/** Whether the mode is asking how to end: Escape is the dialog's, not the page's. */
export const isAsking = (): boolean => asking;
