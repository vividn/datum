// The nowview as a crop of the dayview's data layer (da-0e06p).
//
// vividn, 2026-10-26: «the cached part could just be the data part, and then
// dayview and nowview draw their respective axes and ui elements overtop
// cheaply and quickly without needing any database calls». The CLI draws
// both halves and decides everything in them: `nowview --overlay` is the
// nowview's picture with no data and no database opened — its axis, ticks,
// gridlines, now-line, the words under its now strip and the strip's place
// (`.now-strip`, where it draws now) — and `dayview --data-layer` is a day's
// bands alone, a state still running drawn on to the day's end. This lays the
// second under the first: each day mapped onto each stretch of the overlay's
// axis by their instants (`data-from` … `data-hi`), clipped to the stretch, a
// running state cut at now, and the strip painted with the block each lane
// holds at now. So a stretch or a pan asks the CLI for its overlay alone.

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
 * the page — whether or not a now strip is drawn (da-dv45l).
 */
export function bandOf(root: Element, y: boolean): { start: number; extent: number } {
	const axis = shift(root.querySelector('.axis'));
	if (!y) return { start: 0, extent: axis[1] };
	const margin = shift(root.querySelector('.plot'))[0];
	return { start: axis[0], extent: Number(root.getAttribute('width')) - 2 * margin - axis[0] };
}

function parse(text: string): Document | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	return doc.documentElement.localName === 'svg' && !doc.querySelector('parsererror') ? doc : null;
}

/**
 * The overlay `overlay` with the data of `days` laid under it, as SVG text;
 * the overlay unchanged when it has no axis to crop for (the snapshot, an
 * error). `now` is the strip's instant: the strip is now's (da-dv45l).
 */
export function compose(overlay: string, days: string[], now: number): string {
	const doc = parse(overlay);
	const root = doc?.documentElement;
	const stretches = [...(root?.querySelectorAll('[data-lo]') ?? [])];
	if (!doc || !root || !stretches.length) return overlay;
	const slot = root.querySelector('.now-strip');
	const make = (name: string, attrs: Record<string, string | number>) => {
		const el = doc.createElementNS(NS, name);
		for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
		return el;
	};
	const y = stretches[0].getAttribute('data-axis') === 'y';
	const margin = shift(root.querySelector('.plot'))[y ? 1 : 0];
	const cross = bandOf(root, y).start;
	// The picture on now slides with the clock: its data moves in a `.flow`
	// group under the clip at the now-line, as the CLI's own did (da-hklaz).
	const flow = root.querySelector('clipPath[id^="flow-past"]')?.id;
	const defs = root.querySelector('defs') ?? root.insertBefore(make('defs', {}), root.firstChild);

	const pages = days.map(parse).filter((d): d is Document => !!d);
	const spans = pages.flatMap((p) => [...p.querySelectorAll('[data-lo]')]);
	const scale = new Map<string, number>();
	for (const page of pages) {
		for (const def of page.querySelectorAll('defs > *')) {
			if (def.id && !doc.getElementById(def.id)) defs.append(doc.importNode(def, true));
		}
	}

	for (const g of stretches) {
		const into = axisOf(g);
		const [lo, hi] = [into.lo - margin, into.hi - margin];
		const id = `crop-${y ? 'y' : 'x'}${Math.round(lo * 100)}-${Math.round(hi * 100)}`;
		if (!doc.getElementById(id)) {
			const clip = make('clipPath', { id });
			clip.append(make('rect', y ? { x: -1e5, y: lo, width: 2e5, height: hi - lo } : { x: lo, y: -1e5, width: hi - lo, height: 2e5 }));
			defs.append(clip);
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
			// Every stretch shares one rate: the strip is cut into the axis, not
			// drawn at a scale of its own, so a pattern's is any stretch's.
			for (const p of span.querySelectorAll('[fill^="url(#"]')) scale.set(p.getAttribute('fill')!.slice(5, -1), a);
			const day = make('g', {
				transform: y ? `matrix(1 0 0 ${a} ${cross} ${b})` : `matrix(${a} 0 0 1 ${b} ${cross})`,
				style: `--sx: ${a}`
			});
			for (const child of span.children) day.append(doc.importNode(child, true));
			// A block is cut to the stretch, as the CLI's own picture cut it,
			// and a state still running at now (drawn on to the day's end),
			// at now — on now the clip at the now-line does that as it moves.
			const until = flow ? into.to : Math.min(into.to, Math.max(into.from, Date.now()));
			const unit = (t: number) => from.lo + (t - from.from) * rate(from);
			trim(day, unit(Math.max(into.from, from.from)), unit(Math.min(into.to, from.to)), unit(until), from.to, y);
			at.append(day);
		}
	}

	// What a stretch draws a mark at, a pattern draws stripes at (da-hmtoq):
	// the widget scales them back by this as it does the marks.
	for (const [id, a] of scale) doc.getElementById(id)?.setAttribute('data-scale', String(a));

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
			[y ? 'x' : 'y']: num(block, y ? 'x' : 'y') + cross,
			[y ? 'width' : 'height']: num(block, y ? 'width' : 'height'),
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
	return new XMLSerializer().serializeToString(doc);

	/** Stripes in the strip are the overlay's size, not a stretch's: a copy of their pattern. */
	function stripFill(fill: string): string {
		const pattern = /^url\(#(.+)\)$/.exec(fill)?.[1];
		if (!pattern) return fill;
		if (!doc!.getElementById(`${pattern}-strip`)) {
			const copy = doc!.getElementById(pattern)?.cloneNode(true) as Element | undefined;
			copy?.setAttribute('id', `${pattern}-strip`);
			copy?.removeAttribute('data-scale');
			if (copy) defs.append(copy);
		}
		return `url(#${pattern}-strip)`;
	}
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
