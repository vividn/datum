// The tutorial's «On and off» lesson (da-rru2t), as data: a book, a TV and
// a phone that go on and off on a script, overlapping (da-uzyih.4866), and
// what the learner's record of them is beside it.
//
// vividn, 2026-10-27: «You record when the tv is on/off, the phone is
// on/off and reading (whether or not the book is open.)» Each is an on/off
// field, `start` when it comes on and `end` when it goes off. The script is
// the truth; the record is read back from the database, so what the result
// says is what was stored, not what was tapped.

import type { TailRow } from './tail.js';

export interface Thing {
	field: string;
	name: string;
	/** When it is on (the book open): [on, off] in seconds from the round's start. */
	on: [number, number][];
}

export interface Round {
	/** Seconds the round lasts. */
	length: number;
	things: Thing[];
}

/** The round. A test shortens it; the fields are the kit's (`kits/tutorial.toml`). */
export const DEN: Round = {
	length: 60,
	things: [
		{ field: 'reading', name: 'book', on: [[3, 21], [38, 55]] },
		{ field: 'tv', name: 'TV', on: [[9, 30]] },
		{ field: 'phone', name: 'phone', on: [[15, 46]] }
	]
};

/** A span in milliseconds since the epoch; an open one has no `to`. */
export interface Block {
	from: number;
	to: number | null;
}

/** Whether `thing` is on `t` seconds into the round. */
export const isOn = (thing: Thing, t: number): boolean => thing.on.some(([a, b]) => t >= a && t < b);

/** Each start up to the next end of one field's rows, from `since` on: the learner's blocks. */
export function blocksOf(rows: TailRow[], since: number): Block[] {
	const blocks: Block[] = [];
	let open: number | null = null;
	const timed = rows.map((r) => ({ at: Date.parse(r.time ?? ''), on: r.following_state === true })).filter((r) => r.at >= since);
	for (const { at, on } of timed.sort((a, b) => a.at - b.at)) {
		if (on && open === null) open = at;
		else if (!on && open !== null) {
			blocks.push({ from: open, to: at });
			open = null;
		}
	}
	if (open !== null) blocks.push({ from: open, to: null });
	return blocks;
}

/**
 * The share of `from` to `end` in which the record and the thing agree, on
 * or off: the lesson's accuracy (da-uzyih.4864). An open block runs to `end`.
 */
export function agreement(truth: Block[], yours: Block[], from: number, end: number): number {
	const on = (blocks: Block[], t: number): boolean => blocks.some((b) => t >= b.from && t < (b.to ?? end));
	const edges = [...new Set([from, end, ...[...truth, ...yours].flatMap((b) => [b.from, b.to ?? end])])]
		.filter((t) => t >= from && t <= end)
		.sort((a, b) => a - b);
	let same = 0;
	for (let i = 0; i < edges.length - 1; i++) if (on(truth, edges[i]) === on(yours, edges[i])) same += edges[i + 1] - edges[i];
	return same / (end - from);
}
