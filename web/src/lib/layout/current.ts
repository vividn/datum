// The live tree: one store, one `apply`.
//
// Everything that changes the layout goes through `apply(op)`, where `op`
// is one of tree.ts's operations partially applied — so there is exactly
// one place a change happens and one store for everything else to read:
// the renderer (Layout.svelte), the root bar, and the shell (shell.ts,
// da-0a5o), which fills it on boot from this device's open layouts — each
// one's draft or its save — and keeps each draft in `localStorage` on
// every change. `loaded` is the shell's word that the store holds the
// reader's tree rather than the placeholder below; the page draws nothing
// before it, so no widget mounts twice.

import { writable } from 'svelte/store';
import type { Layout } from './types.js';
import { defaultLayoutFor } from './defaults.js';

export const layout = writable<Layout>(defaultLayoutFor(false));

export const loaded = writable(false);

export function apply(op: (layout: Layout) => Layout): void {
	layout.update(op);
}
