// What the picture drew where a press landed, and so which fields the
// fields widget puts FIRST (da-qu16).
//
// vividn, 2026-09-17 16:44: «I want to have a smart filter of fields
// depending on the y position of your click. So fields that have a span
// over that y position and occur dots that are close to that position
// should be prioritized in the fields menu.» And at 17:01: «clicking in
// the now region should also reorder fields based on y position.»
//
// Nothing is removed by this rule — it PRIORITISES. A person who clicked
// empty space still finds every field, in the order they always were.
//
// This store exists because the nowview's band takes the pointer itself
// (`data-points="own"`, $lib/hover.ts), so a press there reaches nothing
// else on the page; without it the widget that has to answer could not
// know a press had happened at all.
//
// No geometry is invented here and no field name is guessed: the CLI
// names every mark it draws — `.field[data-field]` groups holding blocks
// with `data-from`/`data-to`, dots with `data-at`, and the now strip's
// chip — and this reads THOSE. Where a field is drawn is the original
// datum's hash of its name plus a height
// (datum-core/src/field_spec/presentation.rs); the browser must never
// compute it.
//
// Cleared with the selection, because one press sets both: the clear
// itself lives in $lib/cursor.ts, which every door out of a selection
// already passes through.

import { writable } from 'svelte/store';

/** One mark the picture drew, as the rule below reads it. */
export interface Drawn {
	field: string;
	/** Where it is on screen. */
	box: { top: number; bottom: number; left: number; right: number };
	/** A dot is reached by distance; everything else fills a row. */
	dot?: boolean;
	/** A block's own span, in ms. The now strip's chip is a block with none. */
	from?: number;
	to?: number;
}

/** Where a press fell, and the instant it selected — `null` for one that selected none. */
export interface At {
	x: number;
	/** `null` for a selection made with no height at all: a typed `-t`, a tail row. */
	y: number | null;
	t: number | null;
}

/**
 * How far outside a row a press that landed in NO row may still reach
 * one, in px.
 *
 * The tiny nowview in `mobile` draws rows a few px tall, so «spans the
 * click's y» needs SOME tolerance or a thin lane cannot be aimed at. But
 * a tolerance that always applies pulls the neighbours in with every
 * squarely-hit row — datum stacks the lanes by hashing each field's name
 * and they abut and overlap (measured: 7px rows abutting in a desktop
 * nowview, chips overlapping by 6px in `mobile`'s snapshot) — and the
 * rule stops saying anything. So it is a FALLBACK: a press inside a row
 * is in that row, and only a press that fell in no row at all reaches for
 * the rows a hairline away. The several it then finds tie, and a tie
 * keeps its usual order rather than inventing a ranking.
 */
export const ROW_PX = 4;

/**
 * How near a dot's centre a press must fall to reach it, in px.
 *
 * A fingertip's half-width — `NOW_PX` in $lib/nowpoint.ts is the same
 * 24px for the same reason. A dot is a few px across and a finger is not,
 * so the distance that matters is the finger's, measured from the dot's
 * centre in both directions: a press beside a dot in TIME is no more on
 * it than one beside it in height.
 */
export const REACH_PX = 24;

/**
 * The fields a press fell on or near, each with its rank — 0 best.
 *
 *   0  a block over the press's height whose span also holds its TIME
 *   1  a block over its height (the now strip's chip is one: no time)
 *   2  a dot within `REACH_PX` of the press
 *
 * A field drawn twice keeps its best rank. `null` — nothing near, or a
 * selection with no height at all — is «change nothing».
 */
export function nearest(drawn: Drawn[], at: At): Map<string, number> | null {
	if (at.y === null) return null;
	const rank = new Map<string, number>();
	const put = (field: string, r: number) => {
		const had = rank.get(field);
		if (had === undefined || r < had) rank.set(field, r);
	};
	const y = at.y;
	const rows = drawn.filter((d) => !d.dot);
	const within = (slack: number) => rows.filter((d) => y >= d.box.top - slack && y <= d.box.bottom + slack);
	const hit = within(0);
	for (const d of hit.length ? hit : within(ROW_PX)) {
		const timely = d.from !== undefined && at.t !== null && at.t >= d.from && at.t <= (d.to ?? d.from);
		put(d.field, timely ? 0 : 1);
	}
	for (const d of drawn) {
		if (!d.dot) continue;
		const to = Math.hypot((d.box.left + d.box.right) / 2 - at.x, (d.box.top + d.box.bottom) / 2 - y);
		if (to <= REACH_PX) put(d.field, 2);
	}
	return rank.size ? rank : null;
}

/** The ranking a press published, or `null` while nothing is selected. */
export const lanes = writable<Map<string, number> | null>(null);

/** Read what `svg` drew around the press `at`, and publish the ranking. */
export function readLanes(svg: SVGSVGElement, at: At): void {
	const drawn: Drawn[] = [];
	for (const group of svg.querySelectorAll<SVGGraphicsElement>('.field[data-field]')) {
		const field = group.getAttribute('data-field') ?? '';
		// The marks the CLI named, and the now strip's chip — which is a
		// lane running on in the state it is in and carries no data of its
		// own (dayview_svg.rs), so it is read as a block with no span.
		for (const el of group.querySelectorAll<SVGGraphicsElement>('[data-at], [data-from], .now-chunk')) {
			const box = el.getBoundingClientRect();
			// A mark with nothing drawn in this window is not one a press
			// can land on, and its box is the origin — which would lie.
			if (!(box.height > 0)) continue;
			const [dot, from, to] = ['data-at', 'data-from', 'data-to'].map((a) => el.getAttribute(a));
			drawn.push({
				field,
				box,
				...(dot ? { dot: true } : {}),
				...(from && to ? { from: Date.parse(from), to: Date.parse(to) } : {})
			});
		}
	}
	lanes.set(nearest(drawn, at));
}
