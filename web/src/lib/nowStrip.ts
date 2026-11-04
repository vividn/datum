// Collect's now strip (da-uuzu6): the last second of today's data layer.
//
// vividn, 2026-11-04: «I want this now section to show the last second
// essentially so immediately after you tap and state change, the new change
// sweeps in from the right for a second. Likewise occur dots start on the
// right hand side and sweep over over a second», and «it should be aware of
// how nowview renders now. just updating the svg apporpriately, rather than
// rendering it from scratch», and of the strip as first drawn: «it should
// literally just draw the same now as nowview does». So the strip lays the
// layer the nowview and the day strip hold (#lib/dayLayers.ts) as the
// nowview's crop lays it (`laid`), and NowStrip.svelte slides it with the
// clock. The layer draws a running state
// on to the day's end, so it is drawn ahead already: datum is asked again on
// a write, never for the clock. A tap's mark is added at once (`pendingOf`),
// at the tap's instant, the one the CLI's clock is frozen at (da-szzgy).

import { layMarks } from './dayLayers.js';
import type { FieldButton } from './fields.js';
import { trim } from './nowcrop.js';
import { LAYER_HEIGHT } from './panes.js';

/** How much time the strip spans, now at its edge. */
export const STRIP_MS = 1000;

/**
 * A tap's mark on the strip before the layer has it: a block from `from` to
 * `to`, or a dot at `from`, on its band (`y`, `h` in the layer's units,
 * `LAYER_HEIGHT` across). `fill` null is the strip's ground: an end.
 */
export type Mark = { field: string; from: number; to: number; y: number; h: number; fill: string | null; dot?: boolean };

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
 * the instants of their edges, for the slide to know when one is in view.
 */
export function laid(doc: Document | null | undefined, origin: number, k: number, across: number, span: number): { node: SVGGElement; edges: number[] } {
	const node = document.createElementNS(NS, 'g');
	const edges: number[] = [];
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
			for (const el of lane.children) {
				const [a, b] = el.hasAttribute('data-at') ? [time(el, 'data-at'), time(el, 'data-at')] : [time(el, 'data-from'), time(el, 'data-to')];
				if (Number.isFinite(a)) {
					keep = a <= origin + span && b >= origin - span;
					if (keep) edges.push(a, b);
				}
				if (keep) copy.append(document.importNode(el, true));
			}
			if (copy.firstChild) g.append(copy);
		}
		trim(g, unit(origin - span), unit(origin + span), unit(origin + span), to, false);
		// Along time from `origin`, which `layMarks` then scales to px.
		const at = unit(origin);
		for (const el of g.querySelectorAll('*')) {
			for (const name of ['x', 'cx', 'x1', 'x2']) if (el.hasAttribute(name)) el.setAttribute(name, String(num(el, name) - at));
			// The marks are a picture: no selector of the page's controls or its entries finds them.
			const field = el.getAttribute('data-field');
			for (const { name } of [...el.attributes]) if (name.startsWith('data-') || name.startsWith('aria-')) el.removeAttribute(name);
			if (field !== null) el.setAttribute('data-mark', field);
			if (el.localName === 'title') el.remove();
			const fill = el.getAttribute('fill');
			if (fill?.startsWith('url(#')) el.setAttribute('fill', `url(#collect-${fill.slice(5)}`);
		}
		layMarks(g, k / rate, across);
		node.append(g);
	}
	return { node, edges };
}

/**
 * What a press of `argv` at `at` adds to the strip before the database
 * answers: a moment's dot, a start's block in the field's colour, a switch's
 * in its state's, an end's ground. Null for anything else, a time or a length
 * with it included, which the strip's last second would not show where it lands.
 */
export function pendingOf(argv: string[], fields: FieldButton[], at: number): Mark | null {
	const [verb, name, state, ...rest] = argv;
	const button = fields.find((b) => b.field === name);
	if (!button || rest.length || !Number.isFinite(button.y) || !Number.isFinite(button.height)) return null;
	const band = { field: name, from: at, to: Infinity, y: button.y * LAYER_HEIGHT, h: button.height * LAYER_HEIGHT };
	const bare = state === undefined;
	if ((verb === 'moment' || verb === 'occur') && bare) return { ...band, to: at, y: band.y + band.h / 3, h: band.h / 3, fill: button.color, dot: true };
	if (verb === 'start' && bare) return { ...band, fill: button.color };
	if ((verb === 'end' || verb === 'stop') && bare) return { ...band, fill: null };
	if (verb === 'switch' && state) return { ...band, fill: button.stateColors[state] ?? button.color };
	return null;
}
