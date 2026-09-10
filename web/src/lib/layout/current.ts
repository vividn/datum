// The live tree: one store, one `apply`.
//
// Everything that changes the layout goes through `apply(op)`, where `op`
// is one of tree.ts's operations partially applied — so there is exactly
// one place a change happens and one store for everything else to read:
// the renderer (Layout.svelte), and the save / restore / draft controls
// in the header (LayoutControls.svelte, da-wpgy), which fill it on mount
// from this device's draft or the saved layout and keep the draft in
// `localStorage` on every change. `loaded` is their word that the store
// holds the reader's tree rather than the placeholder below; the page
// draws nothing before it, so no widget mounts twice.

import { writable } from 'svelte/store';
import type { Layout } from './types.js';
import { PHONE_WIDTH, defaultLayoutFor } from './defaults.js';

export const layout = writable<Layout>(defaultLayoutFor(false));

export const loaded = writable(false);

export function apply(op: (layout: Layout) => Layout): void {
	layout.update(op);
}

/**
 * What a layout with no save and no draft starts as: today's page, for
 * this screen. The phone default is chosen by the screen's width at that
 * moment — the same breakpoint the old grid used — and only then; a draft
 * or a save is a decision the reader already made.
 */
export function fallbackLayout(name: string): Layout {
	const phone = typeof matchMedia === 'function' && matchMedia(`(max-width: ${PHONE_WIDTH}px)`).matches;
	return { ...defaultLayoutFor(phone), name };
}
