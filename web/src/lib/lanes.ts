// What the picture drew where a press landed, and so which fields the
// fields widget puts FIRST (da-qu16): `nearest` reads the marks, `closer`
// is the one ordering over them, and `promoted` is what the bar does with
// it — the whole rule in one file, geometry and all.
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

import type { FieldButton } from './fields.js';

/** One mark the picture drew, as the rule below reads it. */
export interface Drawn {
	field: string;
	/** Where it is on screen. */
	box: { top: number; bottom: number; left: number; right: number };
	/** A dot is reached by nearness in HEIGHT; everything else fills a row. */
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
 * the rows a hairline away. The several it then finds are ordered by how
 * far off they fell (`closer`); equidistant ones still tie, and a tie
 * keeps its usual order rather than inventing a ranking.
 */
export const ROW_PX = 4;

/**
 * How far from a dot's HEIGHT a press may fall and still reach it, in px —
 * measured from the mark's own box, as `ROW_PX` is.
 *
 * A fingertip's half-width — `NOW_PX` in $lib/nowpoint.ts is the same 24px
 * for the same reason.
 *
 * It was once the radius of a circle round the dot's centre, so x counted
 * as much as y and a press at a dot's exact height but 25px away in TIME
 * reached it no more than one in another lane entirely. vividn, 2026-09-21
 * 13:37, on his phone: «it seems occur dots aren't well filtered when
 * clicking at a certain y in the now view. Let's filter.» A block is
 * reached by height alone because it spans every x; a dot now is too. How
 * far off in TIME it fell no longer decides WHETHER it is reached — only
 * where it comes in the order (`closer`).
 */
export const REACH_PX = 24;

/** The rank a DOT is given — and so the rank an inactive block falls back to. */
export const DOT = 2;

/** How near a press fell to one mark: the coarse rank, then the distances. */
export interface Near {
	/**
	 * Which KIND of reach found it — 0 best:
	 *
	 *   0  a block over the press's height whose span also holds its TIME
	 *   1  a block over its height (the now strip's chip is one: no time)
	 *   2  a dot within `REACH_PX` of its height
	 *
	 * A block only KEEPS a rank above the dots while its field is on; an
	 * inactive one falls back among them. That is `promoted` below, not
	 * here, because a mark carries no state — a BUTTON does.
	 */
	rank: number;
	/** How far outside the mark the press fell in HEIGHT, px — 0 within it. */
	dy: number;
	/** And in x, which only separates marks the height could not. */
	dx: number;
}

/** How far `v` fell outside `lo`–`hi`; 0 between them. */
const gap = (v: number, lo: number, hi: number) => Math.max(lo - v, v - hi, 0);

/**
 * Which of two marks a press is more ON: the kind of reach first, then
 * how far off in height, then how far off in time.
 *
 * The rank stays the primary key so a squarely-hit row is not diluted by
 * the neighbours it abuts — datum stacks the lanes by hashing each field's
 * name, so they abut and overlap. The distances only ORDER within a rank:
 * a dot under the finger comes before one 18px up, and a dot at the same
 * height an hour ago comes after one at the same height a minute ago
 * (vividn, 2026-09-21: «The fields with the closest y should appear first in
 * the fields list.»).
 *
 * The distance is to the mark's own BOX and not to its centre, so a press
 * INSIDE a mark is nought from it: «the closest y» to a lane a finger is
 * squarely in is zero, however tall the lane, and two marks the press is
 * in at once tie and keep the CLI's order rather than being split by a
 * hairline of centre arithmetic. (`|y − centre|` would rank a lane the
 * press is 8px inside below one 3px outside — which is the one thing «a
 * press inside a row is in that row» was written to prevent.)
 */
export const closer = (a: Near, b: Near) => a.rank - b.rank || a.dy - b.dy || a.dx - b.dx;

/**
 * The fields a press fell on or near, each with how near it fell.
 *
 * A field drawn twice keeps its best mark. `null` — nothing near, or a
 * selection with no height at all — is «change nothing».
 */
export function nearest(drawn: Drawn[], at: At): Map<string, Near> | null {
	if (at.y === null) return null;
	const rank = new Map<string, Near>();
	const put = (field: string, near: Near) => {
		const had = rank.get(field);
		if (had === undefined || closer(near, had) < 0) rank.set(field, near);
	};
	const y = at.y;
	const near = (d: Drawn, r: number): Near => ({
		rank: r,
		dy: gap(y, d.box.top, d.box.bottom),
		dx: gap(at.x, d.box.left, d.box.right)
	});
	const rows = drawn.filter((d) => !d.dot);
	const within = (slack: number) => rows.filter((d) => y >= d.box.top - slack && y <= d.box.bottom + slack);
	const hit = within(0);
	for (const d of hit.length ? hit : within(ROW_PX)) {
		const timely = d.from !== undefined && at.t !== null && at.t >= d.from && at.t <= (d.to ?? d.from);
		put(d.field, near(d, timely ? 0 : 1));
	}
	for (const d of drawn) {
		if (!d.dot) continue;
		const dot = near(d, DOT);
		if (dot.dy <= REACH_PX) put(d.field, dot);
	}
	return rank.size ? rank : null;
}

/**
 * `buttons` with the fields a press landed on or near brought to the
 * FRONT, nearest first (`nearest` in $lib/lanes.ts) and everything else in
 * its usual order.
 *
 * Nothing is removed — the press prioritises, it does not hide.
 *
 * Nearest is `closer`: the kind of reach, then how far off in height, then
 * in time. Which leaves one question the geometry above cannot answer, and
 * vividn answered it (2026-09-21 13:46): «a block can outrank an occur,
 * particularly if it is in an active state. If the block is false, then I
 * would rank by order to center.» So a block keeps its rank above the dots
 * only while its field is ON; an INACTIVE one falls back among them and is
 * ordered by distance like any other mark. It is answered HERE, against
 * the buttons, because a mark the picture drew carries no state and a
 * button does — `active` is the CLI's own answer to «is it on», carried
 * on it by `buttonsFor` in $lib/fields.ts. Nothing is fetched for this and
 * no renderer changed.
 *
 * A stable sort, so marks the rule cannot separate (several rows a few px
 * tall equally under one finger) keep the order they had, which is the
 * bar's usual one (`buttonsFor`): a tie is not a ranking and must not be
 * invented into one.
 */
export function promoted(buttons: FieldButton[], rank: Map<string, Near> | null): FieldButton[] {
	if (!rank?.size) return buttons;
	const near = (button: FieldButton): Near | undefined => {
		const at = rank.get(button.field);
		if (at === undefined || at.rank === DOT || button.active) return at;
		return { ...at, rank: DOT };
	};
	/** `closer`, with a field the press reached nothing of after every one it did. */
	const order = (a: Near | undefined, b: Near | undefined) =>
		a && b ? closer(a, b) : Number(!a) - Number(!b);
	return buttons
		.map((button, i) => ({ button, i, at: near(button) }))
		.sort((a, b) => order(a.at, b.at) || a.i - b.i)
		.map((e) => e.button);
}

/** The ranking a press published, or `null` while nothing is selected. */
export const lanes = writable<Map<string, Near> | null>(null);

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
