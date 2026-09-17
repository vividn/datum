// Which field a press in the nowview landed ON, and the fields it could
// have landed on, in the order the picture drew them (da-rskp).
//
// This store exists for da-qu16, which is the fields widget's half of the
// same sentence: vividn, 2026-09-17 17:01 — "clicking in the now region
// should also reorder fields based on y position". The nowview's band
// takes the pointer itself (`data-points="own"`, $lib/hover.ts), so a
// press there reaches nothing else on the page; without this, the widget
// that has to answer it could not know a press had happened at all.
//
// What is published is what the PICTURE says, read off its own marks: the
// CLI names every lane `.field[data-field]` and draws it at a height of
// its own, so `under` is the lane the press fell in and `order` is every
// lane top to bottom. No geometry is invented here and no field name is
// guessed — a reader of this store is reading datum's own drawing.
//
// Cleared with the selection, because a press is what sets both.

import { writable } from 'svelte/store';

export interface Lanes {
	/** The field the press landed on, or `null` between the lanes. */
	under: string | null;
	/** Every field drawn, in the order the picture stacked them. */
	order: string[];
}

export const lanes = writable<Lanes | null>(null);

/**
 * Read the lanes of `svg` at `clientY` on screen and publish them.
 *
 * Each lane's box is the union of what the CLI drew in it, which is what
 * `getBoundingClientRect` on the group gives.
 */
export function readLanes(svg: SVGSVGElement, clientY: number): void {
	const drawn = [...svg.querySelectorAll<SVGGraphicsElement>('.field[data-field]')]
		.map((el) => ({ field: el.getAttribute('data-field') ?? '', box: el.getBoundingClientRect() }))
		// A field with nothing in this window is an empty group, and an
		// empty group's box is the origin — which would sort first and lie
		// about the order. A lane nothing was drawn in is not a lane a
		// press can land in either.
		.filter((l) => l.box.height > 0)
		.sort((a, b) => a.box.top - b.box.top);
	if (!drawn.length) return void lanes.set(null);
	// A lane runs the way time does, so only the cross coordinate decides
	// which one a press is in; along the band every lane is the same lane.
	const hit = drawn.find(({ box }) => clientY >= box.top && clientY <= box.bottom);
	lanes.set({ under: hit?.field ?? null, order: drawn.map((l) => l.field) });
}
