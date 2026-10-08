// The unsaved draft, which layouts this device has open, and which it has
// locked (da-wpgy, da-0a5o).
//
// All per device, never synced — vividn: "Different devices will want
// different layouts", of the draft "I lean per device", and of the lock
// (2026-09-11 12:56) that it is the device's, never written to the saved
// entry. So they live in `localStorage`, one draft and one lock per layout
// name plus one list of open tabs, under versioned keys. Every access is
// guarded: a browser that refuses storage gets no draft and the saved
// layout, never an error.

import type { Layout, Pane } from './types.js';

/** What every key here begins with. */
const PREFIX = 'datum.layout.';
const DRAFT = `${PREFIX}draft.v1.`;
const LOCKED = `${PREFIX}locked.v1.`;
const ROOT = `${PREFIX}root.v1`;

function get(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

/**
 * The panes' copies from the last visit (`Stale.svelte`, da-bhudz): ONE key
 * for every database, so the cache is bounded once, and the first thing to
 * go when the reader's own state needs the room.
 */
export const STALE = 'datum.stale.v1';

function set(key: string, value: string | null): void {
	for (const retry of [false, true]) {
		try {
			if (value === null) localStorage.removeItem(key);
			else localStorage.setItem(key, value);
			return;
		} catch {
			// Full, perhaps of the cache: drop it and try once more. Refused
			// outright: the draft is simply not kept.
			if (retry) return;
			try {
				localStorage.removeItem(STALE);
			} catch {
				return;
			}
		}
	}
}

export function loadDraft(name: string): Layout | null {
	try {
		return JSON.parse(get(DRAFT + name) ?? 'null');
	} catch {
		return null;
	}
}

export const saveDraft = (layout: Layout) => set(DRAFT + layout.name, JSON.stringify(layout));
export const clearDraft = (name: string) => set(DRAFT + name, null);

/**
 * Whether this device has `name` locked — or null when it has never said,
 * which is what lets a layout that shipped open locked the first time
 * while an unlock, once made, is remembered (da-gshh).
 */
export function loadLocked(name: string): boolean | null {
	const stored = get(LOCKED + name);
	return stored === null ? null : stored === '1';
}
export const saveLocked = (name: string, locked: boolean) => set(LOCKED + name, locked ? '1' : '0');
/** Forget this device's say on `name`, so it opens as a first time would. */
export const clearLocked = (name: string) => set(LOCKED + name, null);

/**
 * How many of datum's new defaults (`newDefaults`, defaults.ts) this device
 * started on or has been offered (da-cr0w2) — none, where it has never said.
 */
const DEFAULTS = `${PREFIX}defaults.v1`;
export const loadDefaults = (): number => Number(get(DEFAULTS)) || 0;
export const saveDefaults = (seen: number) => set(DEFAULTS, String(seen));

/** The root bar as this device left it: the open layouts, and which is active. */
export interface RootState {
	names: string[];
	active: number;
}

export function loadRoot(): RootState | null {
	try {
		const state = JSON.parse(get(ROOT) ?? 'null');
		if (!Array.isArray(state?.names) || !state.names.every((n: unknown) => typeof n === 'string')) return null;
		if (state.names.length === 0) return null;
		const active = Number.isInteger(state.active) ? state.active : 0;
		return { names: state.names, active: Math.min(Math.max(0, active), state.names.length - 1) };
	} catch {
		return null;
	}
}

export const saveRoot = (state: RootState) => set(ROOT, JSON.stringify(state));

/**
 * The whole tree as this device last drew it on database `db` (da-bhudz):
 * what the page draws at once, before the CLI has answered anything — a
 * schema upgrade after a deploy can hold the first answer for a minute.
 * Per database, because a kit's layouts are the data db's.
 */
const SHELL = `${PREFIX}shell.v1.`;

export function loadShell(db: string): Layout | null {
	try {
		const tree = JSON.parse(get(SHELL + db) ?? 'null');
		return Array.isArray(tree?.root?.tabs) && tree.root.tabs.length ? tree : null;
	} catch {
		return null;
	}
}

export const saveShell = (db: string, layout: Layout) => set(SHELL + db, JSON.stringify(layout));

/** JSON with every object's keys sorted, so key order is never a change. */
function canonical(value: unknown): string {
	return JSON.stringify(value, (_, v) =>
		v && typeof v === 'object' && !Array.isArray(v)
			? Object.fromEntries(Object.keys(v).sort().map((k) => [k, v[k]]))
			: v
	);
}

/**
 * The pane with every tab's `locked` dropped, all the way down — by
 * walking the tree's own shape, so a widget's settings (an open record
 * that may hold any keys) are compared as they are.
 */
function unlocked(pane: Pane): Pane {
	return {
		...pane,
		tabs: pane.tabs.map(({ locked: _, ...tab }) =>
			'children' in tab.content
				? { ...tab, content: { ...tab.content, children: tab.content.children.map(unlocked) } }
				: tab
		)
	};
}

/**
 * Whether the draft differs from the save — ids included: a new id is a
 * change. A tab's lock is not: toggling it is not an edit of the tree
 * (da-2a5y), whether or not the lock ends up stored with the layout.
 */
export const isModified = (draft: Layout, saved: Layout) =>
	canonical({ ...draft, root: unlocked(draft.root) }) !== canonical({ ...saved, root: unlocked(saved.root) });
