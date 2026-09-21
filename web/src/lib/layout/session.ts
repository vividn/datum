// Session-level widget options (da-xhio): a setting that belongs to THIS
// tab in THIS session, and never to the layout.
//
// Every option today IS the layout: `Options.svelte` writes one with
// `setWidget`, so a change dirties the draft and `draft.ts` keeps it in
// `localStorage` across a reload. An option declared `session` is the
// other thing — held here, in memory, keyed by the tab it belongs to, and
// gone when the page is. vividn, 2026-09-21, of the ignored-fields toggle:
// «I guess it should just live per session?»
//
// This is the first instance of the pattern da-vhwq describes (widget
// options as session state, written to the layout only by an explicit
// «save options with layout»), and is meant to be subsumed by it rather
// than grown: nothing here knows what any option MEANS.

import { writable } from 'svelte/store';

type Settings = Record<string, unknown>;

const store = writable<Record<string, Settings>>({});

/** What this session has set, tab by tab: read by the renderer and the modal. */
export const session = { subscribe: store.subscribe };

/** Set one session option on one tab. */
export function setSession(tabId: string, key: string, value: unknown): void {
	store.update((all) => ({ ...all, [tabId]: { ...all[tabId], [key]: value } }));
}

/** Forget everything this session set on `tabId`: `Reset widget`'s half of the act. */
export const clearSession = (tabId: string): void =>
	store.update(({ [tabId]: _dropped, ...rest }) => rest);

/**
 * A widget's settings with this session's over them — the one merge, so
 * the widget is handed its settings and never learns where each came from.
 */
export const withSession = (settings: Settings, over: Settings | undefined): Settings =>
	over ? { ...settings, ...over } : settings;
