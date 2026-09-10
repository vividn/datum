// The unsaved draft, and which layout this device shows (da-wpgy).
//
// Both are per device, never synced — vividn: "Different devices will want
// different layouts", and of the draft, "I lean per device". So they live in
// `localStorage`, one draft per layout name, under versioned keys. Every
// access is guarded: a browser that refuses storage gets no draft and the
// saved layout, never an error.

import type { Layout } from './types.js';

const DRAFT = 'datum.layout.draft.v1.';
const CURRENT = 'datum.layout.current.v1';

function get(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

function set(key: string, value: string | null): void {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		// Refused: the draft is simply not kept.
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

/** The layout this device shows. */
export const currentName = () => get(CURRENT) ?? 'default';
export const setCurrentName = (name: string) => set(CURRENT, name);

/** JSON with every object's keys sorted, so key order is never a change. */
function canonical(value: unknown): string {
	return JSON.stringify(value, (_, v) =>
		v && typeof v === 'object' && !Array.isArray(v)
			? Object.fromEntries(Object.keys(v).sort().map((k) => [k, v[k]]))
			: v
	);
}

/** Whether the draft differs from the save — ids included: a new id is a change. */
export const isModified = (draft: Layout, saved: Layout) => canonical(draft) !== canonical(saved);
