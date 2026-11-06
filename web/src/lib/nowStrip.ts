// Collect's now strip (da-uuzu6): the last second of today's data layer.
//
// vividn, 2026-11-04: «I want this now section to show the last second
// essentially so immediately after you tap and state change, the new change
// sweeps in from the right for a second. Likewise occur dots start on the
// right hand side and sweep over over a second», and 2026-11-05: «maybe half
// a second instead of a full second», and «it should be aware of
// how nowview renders now. just updating the svg apporpriately, rather than
// rendering it from scratch», and of the strip as first drawn: «it should
// literally just draw the same now as nowview does». So the strip lays the
// layer the nowview and the day strip hold (#lib/dayLayers.ts) as the
// nowview's crop lays it (`laid`), and NowStrip.svelte slides it with the
// clock. The layer draws a running state
// on to the day's end, so it is drawn ahead already: datum is asked again on
// a write, never for the clock. A tap's mark is added at once (`pendingOf`),
// at the tap's instant, the one the CLI's clock is frozen at (da-szzgy).

import { writable } from 'svelte/store';
import { pendingCommand } from './commands.js';
import { layMarks } from './dayLayers.js';
import type { FieldButton } from './fields.js';
import { trim } from './nowcrop.js';
import { LAYER_HEIGHT, STANDARD_DOT } from './panes.js';

/** How much time the strip spans, now at its edge. */
export const STRIP_MS = 500;

/**
 * A tap's mark on the strip before the layer has it: a block from `from` to
 * `to`, or a dot at `from`, on its band (`y`, `h` in the layer's units,
 * `LAYER_HEIGHT` across; a dot's `h` its height, `aspect` its width over it).
 * `fill` null is an end: its field's lane is drawn only before `from`, so
 * what is beneath shows through.
 */
export type Mark = { field: string; from: number; to: number; y: number; h: number; fill: string | null; dot?: boolean; aspect?: number };

/** A tap's mark, and once its write landed, the epoch a layer must be asked at to hold it. */
export type Tap = { mark: Mark; after?: number };

/**
 * Every tap's mark the layers may not hold yet, for each strip that draws
 * now: collect's and the nowview's (da-d6vzq), so a tap shows in both in
 * the same frame. Each strip leaves out those its own layer holds (`unheld`).
 */
export const taps = writable<Tap[]>([]);

/**
 * Add `mark` to `taps`, and the call that settles it: the epoch its write
 * landed at, or null to take it back, refused. A tap still in flight stays
 * whatever its age; a settled one made over a minute before this tap
 * goes, each strip asking its layer again at every write, so a minute on
 * its own has long held it, and a strip mounted since asks at an epoch past it.
 */
export function tapped(mark: Mark): (after: number | null) => void {
	const held: Tap = { mark };
	taps.update((all) => [...all.filter((t) => t.after === undefined || t.mark.from > mark.from - 60_000), held]);
	return (after) => taps.update((all) => all.flatMap((t) => (t !== held ? [t] : after === null ? [] : [{ mark, after }])));
}

/** The marks of `all` a layer asked at epoch `shown` does not hold. */
export const unheld = (all: Tap[], shown: number): Mark[] => all.filter((t) => t.after === undefined || t.after > shown).map((t) => t.mark);

const NS = 'http://www.w3.org/2000/svg';
const num = (el: Element, name: string) => Number(el.getAttribute(name));
const time = (el: Element, name: string) => Date.parse(el.getAttribute(name) ?? '');

/**
 * Today's layer `doc` laid as the nowview lays it (`layMarks`, `trim` in
 * #lib/nowcrop.ts): each lane's own marks, cloned, at `k` px a ms from
 * `origin` and `across` px a layer unit, those reaching `span` ms either side
 * of `origin` alone, so the numbers stay small. Their `data-` and `aria-`
 * attributes and titles go, a lane's and a mark's `data-field` kept as
 * `data-mark`, so no selector of a button, a state or an entry finds them. With
 * the instants of the blocks' edges, for the slide to know when one is in
 * view, and each dot's instant and elements, for it to slide the dot wholly
 * past the left edge and then remove it. `y`: time runs down, as the layer of
 * a vertical nowview is drawn. `prefix` names the strip's copies of the
 * layer's stripes, a page resolving an id to its first element.
 */
export function laid(
	doc: Document | null | undefined,
	origin: number,
	k: number,
	across: number,
	span: number,
	y = false,
	prefix = 'collect-'
): { node: SVGGElement; edges: number[]; dots: { at: number; marks: Element[] }[] } {
	const node = document.createElementNS(NS, 'g');
	const edges: number[] = [];
	const dots: { at: number; marks: Element[] }[] = [];
	for (const day of doc?.querySelectorAll('.day-data[data-lo]') ?? []) {
		const [from, to, lo, hi] = [time(day, 'data-from'), time(day, 'data-to'), num(day, 'data-lo'), num(day, 'data-hi')];
		const rate = (hi - lo) / (to - from);
		if (!(rate > 0)) continue;
		const unit = (t: number) => lo + (t - from) * rate;
		const g = document.createElementNS(NS, 'g');
		for (const lane of day.querySelectorAll('g.field')) {
			const copy = document.importNode(lane, false) as Element;
			// A mark's symbol follows it, with no instant of its own.
			let keep = false;
			let dot: Element[] | null = null;
			for (const el of lane.children) {
				const point = el.hasAttribute('data-at');
				const [a, b] = point ? [time(el, 'data-at'), time(el, 'data-at')] : [time(el, 'data-from'), time(el, 'data-to')];
				if (Number.isFinite(a)) {
					keep = a <= origin + span && b >= origin - span;
					dot = keep && point ? [] : null;
					if (dot) dots.push({ at: a, marks: dot });
					else if (keep) edges.push(a, b);
				}
				if (!keep) continue;
				const mark = document.importNode(el, true);
				copy.append(mark);
				dot?.push(mark);
			}
			// Kept with nothing in view: a tap's mark is drawn in it, at the field's place in the stacking.
			g.append(copy);
		}
		trim(g, unit(origin - span), unit(origin + span), unit(origin + span), to, y);
		// Along time from `origin`, which `layMarks` then scales to px.
		const at = unit(origin);
		for (const el of g.querySelectorAll('*')) {
			for (const name of y ? ['y', 'cy', 'y1', 'y2'] : ['x', 'cx', 'x1', 'x2']) if (el.hasAttribute(name)) el.setAttribute(name, String(num(el, name) - at));
			// The marks are a picture: no selector of the page's controls or its entries finds them.
			const field = el.getAttribute('data-field');
			for (const { name } of [...el.attributes]) if (name.startsWith('data-') || name.startsWith('aria-')) el.removeAttribute(name);
			if (field !== null) el.setAttribute('data-mark', field);
			if (el.localName === 'title') el.remove();
			const fill = el.getAttribute('fill');
			if (fill?.startsWith('url(#')) el.setAttribute('fill', `url(#${prefix}${fill.slice(5)}`);
		}
		layMarks(g, k / rate, across, y);
		// Nor any selector of a picture's marks (`.point-fg`, `.block`): the nowview's strip stands in its picture (da-d6vzq).
		for (const el of g.querySelectorAll('[class]')) el.setAttribute('class', el.getAttribute('class')!.replace(/\S+/g, 'strip-$&'));
		node.append(g);
	}
	return { node, edges, dots };
}

/**
 * What a press of `argv` at `at` adds to the strip before the database
 * answers: a moment's dot, an `on`'s block in the field's colour, a change's
 * in its state's, an `off`'s end. An old word for each, `start` and
 * the rest, is read as the CLI reads it (`pendingCommand`). Null for anything else, a time or a length
 * with it included, which the strip's last second would not show where it lands.
 */
export function pendingOf(argv: string[], fields: FieldButton[], at: number): Mark | null {
	const [word, name, state, ...rest] = argv;
	const command = pendingCommand(word ?? '');
	const button = fields.find((b) => b.field === name);
	if (!button || rest.length || !Number.isFinite(button.y) || !Number.isFinite(button.height)) return null;
	const band = { field: name, from: at, to: Infinity, y: button.y * LAYER_HEIGHT, h: button.height * LAYER_HEIGHT };
	const bare = state === undefined;
	// As datum sizes it: the standard dot times the field's scale, on its band's middle.
	const dot = STANDARD_DOT * LAYER_HEIGHT * (button.dotScale ?? 1);
	if (command === 'moment' && bare) return { ...band, to: at, y: band.y + (band.h - dot) / 2, h: dot, fill: button.color, dot: true, aspect: button.dotAspect };
	if (command === 'on' && bare) return { ...band, fill: button.color };
	if (command === 'off' && bare) return { ...band, fill: null };
	if (command === 'change' && state) return { ...band, fill: button.stateColors[state] ?? button.color };
	return null;
}

/**
 * Each field's moment at the selected instant `at` in the layer `doc`, by
 * hid (da-0qdsp): a point's mark whose `data-at` is in the same second, the
 * resolution a selection is made to. The last such mark wins.
 */
export function momentsAt(doc: Document | null | undefined, at: number): Map<string, string> {
	const here = new Map<string, string>();
	for (const el of doc?.querySelectorAll('.point-bg[data-at][data-id][data-field]') ?? [])
		if (Math.floor(time(el, 'data-at') / 1000) === Math.floor(at / 1000)) here.set(el.getAttribute('data-field')!, el.getAttribute('data-id')!);
	return here;
}
