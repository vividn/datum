// The nowview as a crop of the dayview's data layer (da-0e06p).
//
// vividn, 2026-10-26: «the cached part could just be the data part, and then
// dayview and nowview draw their respective axes and ui elements overtop
// cheaply and quickly without needing any database calls». The CLI draws
// both halves and decides everything in them: `nowview --overlay` is the
// nowview's picture with no data and no database opened — its axis, ticks,
// gridlines, now-line, the words under its now strip and the strip's place
// (`.now-strip`, where it draws now) — and `dayview --data-layer` is a day's
// bands alone by instant, a state still running drawn on to the day's end,
// held by #lib/dayLayers.ts for the day strip too (da-m6i2a). This lays the
// second under the first: each day mapped onto each stretch of the overlay's
// axis by their instants (`data-from` … `data-hi`) and across onto its band,
// clipped to the stretch, a running state cut at now, and the strip painted
// with the block each lane holds at now. So a stretch or a pan asks the CLI
// for its overlay alone, and a clock change is one more instant on the way.

import { LAYER_HEIGHT, layMarks, type Layer } from './dayLayers.js';
import { sanitizeSvg } from './svgSafe.js';

const NS = 'http://www.w3.org/2000/svg';

/** A stretch of time along a picture's axis: `data-from` … `data-to` drawn from `data-lo` to `data-hi`. */
type Axis = { from: number; to: number; lo: number; hi: number };

const axisOf = (el: Element): Axis => ({
	from: Date.parse(el.getAttribute('data-from') ?? ''),
	to: Date.parse(el.getAttribute('data-to') ?? ''),
	lo: Number(el.getAttribute('data-lo')),
	hi: Number(el.getAttribute('data-hi'))
});

/** Units along the axis per millisecond. */
const rate = (a: Axis) => (a.hi - a.lo) / (a.to - a.from);

const num = (el: Element, name: string) => Number(el.getAttribute(name));

/** A transform's `translate(x y)`, as numbers. */
const shift = (el: Element | null | undefined) =>
	(/translate\(([-\d.e]+)[ ,]+([-\d.e]+)\)/.exec(el?.getAttribute('transform') ?? '')?.slice(1) ?? [0, 0]).map(Number);

/**
 * Where the overlay `root`'s band starts across time, in its plot's units, and
 * how far across it reaches: under the axis strip, read off where the axis's
 * baseline stands — below the band, or right of the strip when time runs down
 * the page — whether or not a now strip is drawn (da-dv45l); and short of the
 * clock's strip when time runs down, which stands right of the band (da-b9glf).
 */
export function bandOf(root: Element, y: boolean): { start: number; extent: number } {
	const axis = shift(root.querySelector('.axis'));
	if (!y) return { start: 0, extent: axis[1] };
	const margin = shift(root.querySelector('.plot'))[0];
	const clock = root.querySelector('.clock-axis');
	const end = clock ? shift(clock)[0] : Number(root.getAttribute('width')) - 2 * margin;
	return { start: axis[0], extent: end - axis[0] };
}

/**
 * The id the nowview gives a day's stripes (da-m6i2a): the day strip draws
 * from the same layer, and a page resolves `url(#…)` to its first element of
 * an id, so the nowview's blocks were painted with the strip's stripes.
 */
const own = (id: string) => `now-${id}`;

function parse(text: string): Document | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	return doc.documentElement.localName === 'svg' && !doc.querySelector('parsererror') ? doc : null;
}

/**
 * The overlay `overlay` with the data of `days` laid under it, as a node of
 * the page, sanitized as the layers are, for `CliOutput` to draw as it is:
 * as text, a dense day was serialized and parsed again at every draw, ~450 ms
 * of main thread (da-cqiil). The overlay's text unchanged when it has no axis
 * to crop for (the snapshot, an error). `now` is the strip's instant: the
 * strip is now's (da-dv45l). A day whose layer failed is said over the band,
 * not left out unsaid.
 */
export function compose(overlay: string, days: Layer[], now: number): SVGSVGElement | string {
	const parsed = parse(overlay)?.documentElement;
	if (!parsed?.querySelector('[data-lo]')) return overlay;
	sanitizeSvg(parsed);
	const root = document.importNode(parsed, true) as unknown as SVGSVGElement;
	const stretches = [...root.querySelectorAll('[data-lo]')];
	// The ids drawn so far: a node not in a document has no `getElementById`,
	// whose first of an id this keeps, as the map is built from the last.
	const ids = new Map([...root.querySelectorAll('[id]')].reverse().map((el) => [el.id, el]));
	const slot = root.querySelector('.now-strip');
	const make = (name: string, attrs: Record<string, string | number>) => {
		const el = document.createElementNS(NS, name);
		for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
		return el;
	};
	const y = stretches[0].getAttribute('data-axis') === 'y';
	const margin = shift(root.querySelector('.plot'))[y ? 1 : 0];
	const band = bandOf(root, y);
	const cross = band.start;
	// The layer's day is `LAYER_HEIGHT` across, the band its own (da-m6i2a).
	const c = band.extent / LAYER_HEIGHT;
	// The picture on now slides with the clock: its data moves in a `.flow`
	// group under the clip at the now-line, as the CLI's own did (da-hklaz).
	const flow = root.querySelector('clipPath[id^="flow-past"]')?.id;
	const defs = root.querySelector('defs') ?? root.insertBefore(make('defs', {}), root.firstChild);
	const define = (el: Element) => {
		ids.set(el.id, el);
		defs.append(el);
	};

	const pages = days.map((day) => day.doc()).filter((d): d is Document => !!d);
	const failed = days.find((day) => day.error)?.error;
	if (failed) {
		const note = make('text', { class: 'layer-error', x: (y ? cross : margin) + 4, y: (y ? margin : cross) + 12, 'font-size': 11, fill: '#ff0000' });
		note.textContent = failed;
		root.append(note);
	}
	const spans = pages.flatMap((p) => [...p.querySelectorAll('[data-lo]')]);
	for (const page of pages) {
		for (const def of page.querySelectorAll('defs > *')) {
			const id = def.localName === 'pattern' ? own(def.id) : def.id;
			if (!def.id || ids.has(id)) continue;
			const copy = document.importNode(def, true);
			copy.setAttribute('id', id);
			define(copy);
		}
	}

	for (const g of stretches) {
		const into = axisOf(g);
		const [lo, hi] = [into.lo - margin, into.hi - margin];
		const id = `crop-${y ? 'y' : 'x'}${Math.round(lo * 100)}-${Math.round(hi * 100)}`;
		if (!ids.has(id)) {
			const clip = make('clipPath', { id });
			clip.append(make('rect', y ? { x: -1e5, y: lo, width: 2e5, height: hi - lo } : { x: lo, y: -1e5, width: hi - lo, height: 2e5 }));
			define(clip);
		}
		let at: Element = make('g', { class: 'crop', 'clip-path': `url(#${id})` });
		g.append(at);
		if (flow) {
			const [clipped, moving] = [make('g', { 'clip-path': `url(#${flow})` }), make('g', { class: 'flow' })];
			at.append(clipped);
			clipped.append(moving);
			at = moving;
		}
		for (const span of spans) {
			const from = axisOf(span);
			if (!(from.to > into.from && from.from < into.to)) continue;
			// Units of the day's picture to the overlay's: its instants where the overlay draws them.
			const a = rate(into) / rate(from);
			const b = lo + (from.from - into.from) * rate(into) - a * from.lo;
			const unit = (t: number) => from.lo + (t - from.from) * rate(from);
			// Laid out at the overlay's px (`layMarks`), a mark as big as the
			// band makes it: under a stretch each would carry a transform of
			// its own to keep its size (da-syaz8).
			const day = make('g', { transform: y ? `translate(${cross} ${b})` : `translate(${b} ${cross})` });
			for (const child of span.children) day.append(document.importNode(child, true));
			// A block's stripes start where it starts (da-rdiph): a copy of its
			// pattern at its instant, which the widget scales back about it, so
			// a stretch moves them with the block and never across it.
			for (const el of day.querySelectorAll('[fill^="url(#"]')) {
				const id = anchored(own(el.getAttribute('fill')!.slice(5, -1)), el.hasAttribute('data-from') ? unit(Date.parse(el.getAttribute('data-from')!)) * a : null);
				el.setAttribute('fill', `url(#${id})`);
			}
			// A block is cut to the stretch, as the CLI's own picture cut it,
			// and a state still running at now (drawn on to the day's end),
			// at now — on now the clip at the now-line does that as it moves.
			const until = flow ? into.to : Math.min(into.to, Math.max(into.from, Date.now()));
			trim(day, unit(Math.max(into.from, from.from)), unit(Math.min(into.to, from.to)), unit(until), from.to, y);
			layMarks(day, a, c, y);
			at.append(day);
		}
	}

	// The strip, as the CLI paints it: each lane in the state its block holds
	// at now, where the overlay draws now.
	const holding = spans.find((s) => {
		const a = axisOf(s);
		return a.from <= now && now < a.to;
	});
	const [along, length] = y ? ['y', 'height'] : ['x', 'width'];
	for (const lane of slot ? (holding?.querySelectorAll('g.field') ?? []) : []) {
		const block = [...lane.querySelectorAll('rect.block')].find((b) => {
			const [from, to] = ['data-from', 'data-to'].map((n) => Date.parse(b.getAttribute(n) ?? ''));
			return from <= now && now < to;
		});
		if (!block) continue;
		const into = make('g', { class: 'field', 'data-field': lane.getAttribute('data-field') ?? '' });
		const chunk = make('rect', {
			class: 'now-chunk',
			[y ? 'x' : 'y']: num(block, y ? 'x' : 'y') * c + cross,
			[y ? 'width' : 'height']: num(block, y ? 'width' : 'height') * c,
			[along]: num(slot!, along),
			[length]: num(slot!, length),
			fill: stripFill(block.getAttribute('fill') ?? 'none')
		});
		const hid = block.getAttribute('data-id');
		if (hid) chunk.setAttribute('data-id', hid);
		into.append(chunk);
		slot!.before(into);
	}
	// What stands at now — its line, drawn a gap past it, the strip and the
	// word under it — is one group, which the widget scales back about now
	// under a stretch, as it does a mark: a stretch does not widen the strip
	// that datum's picture of the window then draws as wide as ever (da-ygpto).
	const line = root.querySelector('.current-time-line[data-now]');
	const atNow = [...(line?.parentElement?.children ?? [])].filter((el) =>
		el.matches('.current-time-line, .current-time-line-far, .current-time-line-end, .now-label, .now-strip, g.field')
	);
	if (line && atNow.length) {
		const group = make('g', { class: 'at-now', 'data-anchor': num(line, 'data-now') - margin });
		atNow[0].before(group);
		group.append(...atNow);
	}
	return root;

	/** The pattern `id` laid from `origin` along time, in the day's units: a copy, `data-origin` saying where. */
	function anchored(id: string, origin: number | null): string {
		if (origin === null || !Number.isFinite(origin)) return id;
		const at = `${id}-at${origin}`;
		if (!ids.has(at)) {
			const copy = ids.get(id)?.cloneNode(true) as Element | undefined;
			if (!copy) return id;
			copy.setAttribute('id', at);
			copy.setAttribute('data-origin', String(origin));
			define(copy);
		}
		return at;
	}

	/** Stripes in the strip are the overlay's size, not a stretch's: a copy of their pattern. */
	function stripFill(fill: string): string {
		const drawn = /^url\(#(.+)\)$/.exec(fill)?.[1];
		if (!drawn) return fill;
		const pattern = own(drawn);
		if (!ids.has(`${pattern}-strip`)) {
			const copy = ids.get(pattern)?.cloneNode(true) as Element | undefined;
			copy?.setAttribute('id', `${pattern}-strip`);
			if (copy) define(copy);
		}
		return `url(#${pattern}-strip)`;
	}
}

/** What of a picture is its time axis's rule: its gridlines, ticks and their labels, in order. */
const RULE = ['gridline', 'tick', 'tick-label'].map((c) => `.plot > .${c}`).join(', ');

/**
 * The time axis of `overlay`, datum's picture of another window, laid into
 * the picture `into` in place of its own (da-gy4xz): each gridline, tick and
 * label where `into` draws its instant, `by` ms on, so a gesture's preview
 * is ruled and labelled as datum rules the window it is at, and its release
 * moves no label. `null` takes it out again, and `into`'s own come back.
 */
export function graftAxis(into: SVGSVGElement, overlay: string | null, by = 0): void {
	into.querySelector('.live-axis')?.remove();
	into.classList.remove('axis-live');
	const doc = overlay ? parse(overlay) : null;
	const [from, to] = [doc?.documentElement, into].map((root) => [...(root?.querySelectorAll('[data-lo]') ?? [])].map(axisOf));
	const plot = into.querySelector('.plot');
	if (!doc || !from.length || !to.length || !plot) return;
	const y = into.querySelector('[data-lo]')!.getAttribute('data-axis') === 'y';
	const [m1, m0] = [doc.querySelector('.plot'), plot].map((p) => shift(p)[y ? 1 : 0]);
	// A place before, after or on a strip is read off the stretch it is nearest.
	const near = (all: Axis[], off: (a: Axis) => number) => all.reduce((a, b) => (off(b) < off(a) ? b : a));
	const move = (u: number) => {
		const a = near(from, (s) => Math.max(s.lo - u, u - s.hi));
		const t = a.from + (u - a.lo) / rate(a) + by;
		const b = near(to, (s) => Math.max(s.from - t, t - s.to));
		return b.lo + (t - b.from) * rate(b);
	};
	const g = into.ownerDocument.createElementNS(NS, 'g');
	g.setAttribute('class', 'live-axis');
	for (const el of doc.querySelectorAll(RULE)) {
		const copy = into.ownerDocument.importNode(el, true);
		for (const name of y ? ['y', 'y1', 'y2'] : ['x', 'x1', 'x2'])
			if (copy.hasAttribute(name)) copy.setAttribute(name, String(move(num(copy, name) + m1) - m0));
		g.append(copy);
	}
	plot.insertBefore(g, plot.querySelector(RULE));
	into.classList.add('axis-live');
}

/**
 * Each block of `day` cut to `lo` … `hi` along its axis, in the day's own
 * units, and dropped where nothing of it is left; one still running at the
 * day's `end` (drawn on to it) cut at `running` too.
 */
function trim(day: Element, lo: number, hi: number, running: number, end: number, y: boolean): void {
	const [start, length] = y ? ['y', 'height'] : ['x', 'width'];
	for (const block of day.querySelectorAll('rect.block')) {
		const to = Date.parse(block.getAttribute('data-to') ?? '') >= end ? Math.min(hi, running) : hi;
		const [a, b] = [Math.max(lo, num(block, start)), Math.min(to, num(block, start) + num(block, length))];
		if (b <= a) block.remove();
		else {
			block.setAttribute(start, String(a));
			block.setAttribute(length, String(b - a));
		}
	}
}
