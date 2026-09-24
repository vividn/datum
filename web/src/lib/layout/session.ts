// What THIS TAB IS SHOWING, this session (da-vhwq; first built, one option
// wide, as da-xhio).
//
// A widget setting used to BE the layout: every change went down
// `setWidget`, so moving a nowview's window dirtied the draft, lit the
// layout's dot and waited there to be saved. vividn, 2026-09-21, wanted the
// third state between «nothing» and «the layout»:
//
//   session   what this tab is showing right now, this session
//             — changed freely, from the widget's own menu, inline
//   layout    what the layout says it should show
//             — written ONLY by «Save options with layout»
//
// So this store is now the ONE place a widget setting changes: the menu's
// option rows write here, and so does a widget moving a setting of its own
// (nowview's − / +, the day strip's handle, the stats widget's range
// chips). The tree is written by `saveOptions` alone, and the lock hides
// that act — which is what makes «when locked, nothing modifies the
// layout» true by CONSTRUCTION rather than by a guard every writer has to
// remember.
//
// Nothing here knows what any option MEANS. What is held against a tab is
// only the DIFFERENCE from what the layout says (`overlay` below), so a
// setting moved and moved back leaves nothing behind, and «Save options
// with layout» is offered exactly when there is something to save.
//
// The lifetime, at the edges vividn did not have to say: a reload starts
// every tab back at what the layout says (this store is memory alone);
// leaving a root tab and coming back keeps what was set, since the page
// never unmounted; closing a tab forgets it (`pruneSession`); and
// restoring a saved layout keeps it, since the tab the setting is against
// comes back with the same id.

import { writable } from 'svelte/store';
import type { Layout, NodeId } from './types.js';
import { scopes } from './tree.js';

type Settings = Record<string, unknown>;

const store = writable<Record<string, Settings>>({});

/** What this session has set, key by key: read by the renderer and the menu. */
export const session = { subscribe: store.subscribe };

/**
 * The key a tab's session settings are held against: the LAYOUT it is in,
 * the TAB, and WHAT THE TAB HOLDS (`scopes`, tree.ts).
 *
 * Not the bare tab id. `defaults.ts` mints tab ids from a literal name, so
 * `main` and `mobile` both hold a `tail-tab` and a window set on one would
 * have shown on the other; and `Change Widget` leaves the tab where it is
 * and mints a new widget inside it, so a setting made on what was there
 * would have landed on what is there now. Either way the entry is orphaned
 * rather than inherited, and `pruneSession` drops it.
 */
export const sessionKey = (scope: string, tabId: NodeId, content: NodeId): string =>
	`${scope}\u0000${tabId}\u0000${content}`;

/** The same key, for a caller that holds the tree rather than the parts. */
export function keyOf(layout: Layout, tabId: NodeId): string {
	const at = scopes(layout).get(tabId);
	return sessionKey(at?.scope ?? '', tabId, at?.content ?? '');
}

/**
 * The keys of `next` whose value the layout does not already give: THE
 * OVERLAY. Named for what it returns and not `changed`, which read as a
 * predicate (the panel on #64).
 */
export const overlay = (base: Settings, next: Settings): Settings =>
	Object.fromEntries(Object.entries(next).filter(([key, value]) => value !== base[key]));

/**
 * What this tab shows, given the settings the layout holds for it.
 *
 * `base` is what the LAYOUT says for this widget (`widget.settings`);
 * `next` is the settings as they should now be SHOWN, which a caller
 * builds over the merged pair. What is held is the difference, REPLACED
 * rather than folded into, so a setting moved back to what the layout says
 * leaves nothing held — and «Save options with layout» is offered exactly
 * when there is something to save.
 */
export function setSession(key: string, base: Settings, next: Settings): void {
	const over = overlay(base, next);
	store.update(({ [key]: _was, ...rest }) =>
		Object.keys(over).length > 0 ? { ...rest, [key]: over } : rest
	);
}

/** Forget everything this session set against `key`: `Reset widget`'s half of the act. */
export const clearSession = (key: string): void => store.update(({ [key]: _dropped, ...rest }) => rest);

/**
 * A widget's settings with this session's over them — the one merge, so a
 * widget is handed its settings and never learns where each came from.
 */
export const withSession = (settings: Settings, over: Settings | undefined): Settings =>
	over ? { ...settings, ...over } : settings;

/**
 * Drop what is held against a tab that is no longer in the tree.
 *
 * Called on every change of the layout (`current.ts`), which is the only
 * thing that can close a tab or change what one holds. Restoring a saved
 * layout brings the same tabs and the same widgets back in the same
 * change, so it keeps what they hold; closing a tab or changing its widget
 * does not, so neither a later tab minted with the same literal id nor a
 * freshly chosen widget inherits it.
 */
export function pruneSession(layout: Layout): void {
	store.update((all) => {
		const keys = Object.keys(all);
		if (keys.length === 0) return all;
		const live = new Set(
			[...scopes(layout)].map(([tabId, { scope, content }]) => sessionKey(scope, tabId, content))
		);
		const kept = keys.filter((key) => live.has(key));
		return kept.length === keys.length ? all : Object.fromEntries(kept.map((key) => [key, all[key]]));
	});
}
