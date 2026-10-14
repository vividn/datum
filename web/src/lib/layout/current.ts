// The live tree: one store, one `apply`.
//
// Everything that changes the layout goes through `apply(op)`, where `op`
// is one of tree.ts's operations partially applied — so there is exactly
// one place a change happens and one store for everything else to read:
// the renderer (Layout.svelte), the root bar, and the shell (shell.ts,
// da-0a5o), which fills it on boot from this device's open layouts — each
// one's draft or its save — and keeps each draft in `localStorage` on
// every change. `loaded` is the shell's word that the store holds a tree
// to draw rather than the placeholder below: the tree this device last drew,
// or what it can build without the database, set before the CLI is asked
// anything (da-bhudz) — and replaced by the reader's own only where the two
// differ, so a widget mounts twice only when there is something to change.

import { writable } from 'svelte/store';
import type { Layout } from './types.js';
import { defaultLayoutFor } from './defaults.js';
import { pruneSession } from './session.js';

export const layout = writable<Layout>(defaultLayoutFor(false));

export const loaded = writable(false);

/**
 * The changes made to the tree drawn before the shell's boot was over
 * (da-bhudz) — a tab chosen, an option saved while an upgrade ran — which
 * the reader's own tree is given too when it arrives (`replay`).
 */
let early: ((layout: Layout) => Layout)[] | null = [];

export function apply(op: (layout: Layout) => Layout): void {
	early?.push(op);
	layout.update(op);
}

/** `tree` with every change made to the early one; from now on nothing is kept. */
export function replay(tree: Layout): Layout {
	const ops = early ?? [];
	early = null;
	return ops.reduce((t, op) => op(t), tree);
}

// A tab that has gone takes what this session set on it with it (da-vhwq):
// the tree is the only thing that can close a tab, so every change of it is
// where that is noticed. Set here rather than in `session.ts` so the store
// module stays a store and does not reach back for the tree.
layout.subscribe(pruneSession);

/**
 * LAYOUT MODE (da-vvcxc): the page as it was when the mode was entered, and
 * the root tab it was entered on — or null, which is normal mode, where
 * nothing the reader does moves the tree. Its acts are `mode.ts`'s; it is
 * here so the renderer and the shell read it without importing them.
 */
export const editing = writable<{ tab: string; before: Layout } | null>(null);
