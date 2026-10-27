// Maximize: one pane alone, filling the root's content (da-00am).
//
// vividn, on the deployed build: «let's add a little maximize icon, that then
// maximizes the pane, leaving the layout and coming back should unmaximize
// the pane, the maximize shouldn't be too sticky in general.»
//
// So it is VIEW STATE, not layout state: a store of its own rather than a
// field on the tree. Nothing about the layout changes when a pane is
// maximized — no draft is written, no copy wears the modified dot, and a
// reload comes back to the whole tree by construction, because this store
// is not persisted anywhere. It is also what makes «restore» exact: the
// tree was never touched, nor taken down — it stays mounted and hidden
// under the big pane (da-x7xjg, Layout.svelte) — so putting it back is
// showing it again, ratios, tabs, pictures and all.
//
// What is remembered is the ROOT TAB it belongs to and the PANE, because a
// maximize belongs to the layout it was made in: the root render honours it
// only while that tab is the one in front and the pane is still somewhere in
// its tree. That one check is also the healing — a closed pane, a break-out,
// any edit that takes the pane away, and the state simply stops applying.
// The rest of the not-stickiness is explicit: `unmaximize` from the shell's
// `activate`, `open`, `restore` and `resetLayout`.

import { derived, get, writable } from 'svelte/store';
import type { NodeId, Pane, Split, Tab, Widget } from './types.js';
import { isSplit } from './types.js';
import { layout } from './current.js';

/** The root tab the maximize belongs to, and the pane filling it. */
export interface Maximized {
	tab: NodeId;
	pane: NodeId;
}

export const maximized = writable<Maximized | null>(null);

export const unmaximize = (): void => maximized.set(null);

/** The root tab in front: what a maximize made now belongs to. */
const frontTab = (): NodeId | undefined => {
	const root = get(layout).root;
	return root.tabs[root.active]?.id;
};

/** The badge's press: maximize this pane, or restore it if it is the one. */
export function toggleMaximize(paneId: NodeId): void {
	const tab = frontTab();
	if (tab === undefined) return;
	maximized.update((m) => (m && m.tab === tab && m.pane === paneId ? null : { tab, pane: paneId }));
}

/**
 * The maximize IN FORCE: the state, while the root tab it belongs to is the
 * one in front. What a frame's badge reads to know which way round it is —
 * the store alone would light the badge of a pane inside a tab nobody is
 * looking at.
 */
export const inForce = derived([layout, maximized], ([$layout, $maximized]) =>
	$maximized && $maximized.tab === $layout.root.tabs[$layout.root.active]?.id ? $maximized : null
);

/**
 * The maximized pane inside root tab `tab` — null when the state does not
 * apply here, which is the whole of the healing.
 */
export function maximizedIn(m: Maximized | null, tab: Tab): Pane | null {
	if (!m || m.tab !== tab.id) return null;
	const walk = (content: Widget | Split): Pane | null => {
		if (!isSplit(content)) return null;
		for (const child of content.children) {
			if (child.id === m.pane) return child;
			for (const t of child.tabs) {
				const found = walk(t.content);
				if (found) return found;
			}
		}
		return null;
	};
	return walk(tab.content);
}
