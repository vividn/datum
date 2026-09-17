// Which entries are selected, as the bar's ONE `hid` pill (da-r13j).
//
// vividn, 2026-09-17: «Clicking on a row in the tail, or on the generated
// document in the REPL adds a pill with the hid in the Input», and «control
// click allows selecting multiple different hids, shift click can select a
// range … you can lock the hid, which then enters a multi select mode».
//
// The selection is stored nowhere but the pill: its `text` is the hids,
// comma-joined, and its sections are one per hid, each ✕-able. Everything
// here is pure; `select` in $lib/bar.ts applies it to the store.

import type { Pill, PillSection } from './bar.js';
import { fits, type Grammar } from './commands.js';
import { shellSplit } from './shellSplit.js';

/** What a tap does to the selection. */
export type Pick = 'replace' | 'toggle' | 'range';

/**
 * Two spellings of one entry: a tail row carries the whole hid, a document
 * the short one the CLI prints — each is a prefix of the other.
 */
export const same = (a: string, b: string): boolean => a.startsWith(b) || b.startsWith(a);

export const isSelected = (selected: string[], hid: string): boolean => selected.some((h) => same(h, hid));

/** The pick a pointer event means: shift a range, ctrl/⌘ — or a locked pill — a toggle. */
export const pickOf = (e: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }, locked: boolean): Pick =>
	e.shiftKey ? 'range' : e.ctrlKey || e.metaKey || locked ? 'toggle' : 'replace';

/**
 * The selection after a tap on `hid`.
 *
 * `replace` selects it alone, and clears it when it was the one selected.
 * `range` is `anchor` (the last tap) to `hid` in `order`, the entries in the
 * order that surface draws them — what is not drawn is not in it; it
 * replaces the selection, or adds to it while the pill is `locked`. With no
 * anchor in `order` a range is a plain tap.
 */
export function picked(
	current: string[],
	hid: string,
	pick: Pick,
	order: string[],
	anchor: string | null,
	locked: boolean
): string[] {
	const from = anchor === null ? -1 : order.findIndex((h) => same(h, anchor));
	const to = order.findIndex((h) => same(h, hid));
	if (pick === 'range' && from >= 0 && to >= 0) {
		const span = order.slice(Math.min(from, to), Math.max(from, to) + 1);
		return [...(locked ? current.filter((h) => !isSelected(span, h)) : []), ...span];
	}
	if (pick === 'toggle' || (pick === 'range' && locked))
		return isSelected(current, hid) ? current.filter((h) => !same(h, hid)) : [...current, hid];
	return current.length === 1 && same(current[0], hid) ? [] : [hid];
}

/** How a hid is drawn on the pill: the CLI's short form. */
const short = (hid: string): string => hid.slice(0, 5);

/**
 * One section per hid, each dropping its own: what it leaves is the others
 * as TEXT, and `select`'s store redraws their sections ($lib/bar.ts) — a
 * `rest` carrying its own sections would nest every order of every subset.
 */
const drawn = (hids: string[]): PillSection[] =>
	hids.length > 1
		? hids.map((h) => ({ text: short(h), rest: { text: hids.filter((x) => x !== h).join(',') } }))
		: [{ text: short(hids[0]) }];

/** The `hid` pill holding `hids`, or null for none. */
export const hidPill = (hids: string[], locked = false): Pill | null =>
	hids.length ? { id: 'hid', kind: 'hid', text: hids.join(','), locked, sections: drawn(hids) } : null;

/**
 * The argv a press runs with the pills, and the pills it spent.
 *
 * A command gets only the pills its grammar takes — the rest stay loaded
 * for the next one. A first word the grammar does not know (a field-first
 * line, an alias, or no grammar yet) gets every pill but the hid (`fits`).
 * The hids go right after the command's name, where every id-taking usage
 * line has them (`retime <id> -5` parses, `retime -5 <id>` does not),
 * several as ONE comma-joined word (`update A B` takes B as a state); the
 * other pills after the words.
 */
export function withPills(
	grammar: Grammar | null,
	argv: string[],
	pills: Pill[]
): { argv: string[]; used: Pill[] } {
	const takes = argv.length ? grammar?.get(argv[0]) : undefined;
	const used = pills.filter((p) => fits(takes, p));
	const hid = used.find((p) => p.kind === 'hid');
	const ids = hid ? [hid.text] : [];
	const rest = used.filter((p) => p !== hid).flatMap((p) => shellSplit(p.text));
	return { argv: [...argv.slice(0, 1), ...ids, ...argv.slice(1), ...rest], used };
}
