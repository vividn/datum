// The nowview as a crop of the dayview's data layer (da-0e06p).
//
// vividn, 2026-10-26: «the cached part could just be the data part, and then
// dayview and nowview draw their respective axes and ui elements overtop
// cheaply and quickly without needing any database calls». The CLI draws
// both halves and decides everything in them: `nowview --overlay` is the
// nowview's picture with no data and no database opened — its axis, ticks,
// gridlines, now-line, the words under its now strip and the strip's place
// (`.now-strip`) — and `dayview --data-layer` is a day's bands alone, a state
// still running drawn on to the day's end. This lays the second under the
// first: each day mapped onto each stretch of the overlay's axis by their
// instants (`data-from` … `data-hi`), clipped to the stretch, a running state
// cut at now, and the strip painted with the block each lane holds at the
// strip's instant. So a stretch or a pan asks the CLI for its overlay alone.

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

function parse(text: string): Document | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	return doc.documentElement.localName === 'svg' && !doc.querySelector('parsererror') ? doc : null;
}

/**
 * The overlay `overlay` with the data of `days` laid under it, as SVG text;
 * the overlay unchanged when it has no strip to crop for (the snapshot, an
 * error). `instant` is the strip's: the pill's, or now.
 */
export function compose(overlay: string, days: string[], instant: number): string {
	const doc = parse(overlay);
	const slot = doc?.querySelector('.now-strip');
	if (!doc || !slot) return overlay;
	const root = doc.documentElement;
	const make = (name: string, attrs: Record<string, string | number>) => {
		const el = doc.createElementNS(NS, name);
		for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
		return el;
	};
	const stretches = [...root.querySelectorAll('[data-lo]')];
	const y = stretches[0]?.getAttribute('data-axis') === 'y';
	const plot = /translate\(([-\d.e]+)[ ,]+([-\d.e]+)\)/.exec(root.querySelector('.plot')?.getAttribute('transform') ?? '');
	const margin = plot ? Number(plot[y ? 2 : 1]) : 0;
	const cross = num(slot, y ? 'x' : 'y');
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

	// The strip, as the CLI painted it (da-zccjs): each lane in the state its
	// block holds at the instant, read to the end of its second; a lane
	// changing state there, the state it left in the half facing the past
	// and the one it entered in the other; and a dot in that second moved
	// off the timeline into the strip's middle.
	const holding = (t: number) =>
		spans.find((s) => {
			const a = axisOf(s);
			return a.from <= t && t < a.to;
		});
	const blockAt = (field: Element | undefined, t: number) =>
		[...(field?.querySelectorAll('rect.block') ?? [])].find((b) => {
			const [from, to] = ['data-from', 'data-to'].map((n) => Date.parse(b.getAttribute(n) ?? ''));
			return from <= t && t < to;
		});
	const lanes = (t: number) => new Map([...(holding(t)?.querySelectorAll('g.field') ?? [])].map((f) => [f.getAttribute('data-field') ?? '', f]));
	const [after, before] = [lanes(instant + 999), lanes(instant - 1)];
	const [along, length] = y ? ['y', 'height'] : ['x', 'width'];
	const middle = num(slot, along) + num(slot, length) / 2;
	for (const [name, lane] of after) {
		const into = make('g', { class: 'field', 'data-field': name });
		const [then, now] = [blockAt(before.get(name), instant - 1), blockAt(lane, instant + 999)];
		const parts = now && then && then.getAttribute('data-id') !== now.getAttribute('data-id') ? [then, now] : now ? [now] : [];
		parts.forEach((block, i) => {
			const half = num(slot, length) / parts.length;
			const chunk = make('rect', {
				class: 'now-chunk',
				[y ? 'x' : 'y']: num(block, y ? 'x' : 'y') + cross,
				[y ? 'width' : 'height']: num(block, y ? 'width' : 'height'),
				[along]: num(slot, along) + i * half,
				[length]: half,
				fill: stripFill(block.getAttribute('fill') ?? 'none')
			});
			const hid = block.getAttribute('data-id');
			if (hid) chunk.setAttribute('data-id', hid);
			into.append(chunk);
		});
		for (const dot of lane.querySelectorAll('.point-bg[data-at]')) {
			const at = Date.parse(dot.getAttribute('data-at') ?? '');
			if (!(instant <= at && at < instant + 1000)) continue;
			const moved = make('g', {
				transform: y ? `translate(${cross} ${middle - num(dot, 'cy')})` : `translate(${middle - num(dot, 'cx')} ${cross})`
			});
			moved.append(doc.importNode(dot, true));
			const fg = dot.nextElementSibling;
			if (fg?.classList.contains('point-fg')) moved.append(doc.importNode(fg, true));
			into.append(moved);
			const hid = dot.getAttribute('data-id');
			for (const drawn of [...root.querySelectorAll('.crop .point-bg[data-id]')].filter((d) => hid && d.getAttribute('data-id') === hid)) {
				if (drawn.nextElementSibling?.classList.contains('point-fg')) drawn.nextElementSibling.remove();
				drawn.remove();
			}
		}
		if (into.children.length) slot.before(into);
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
