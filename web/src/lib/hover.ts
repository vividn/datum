// What a drawn view says under the pointer (da-s5da).
//
// datum draws dayview and nowview as SVG and names every mark in it: a
// `<title>` that reads the row the way the tail does (`work: coding
// 14:00–14:30`), its state and instants as `data-` attributes, and on
// each day group the span of time the day's axis covers (`data-from` →
// `data-to` over `data-lo` → `data-hi` user units along `data-axis`). The
// page adds the one thing a picture cannot know, which is where the
// pointer is — `timeAt` — and shows a label with it. The `<title>` does
// not come along (da-8jb8): inline, it is the browser's own tooltip, drawn
// over the page's, so `inlineSvg` moves its text onto the mark.

import { clockLabel } from './clock.js';
import { pointAt, pointed, setCursor, type Cursor } from './cursor.js';

/** The outermost `<svg>` an element is drawn in. */
function pictureOf(el: Element): SVGSVGElement | null {
	let svg = el.closest('svg');
	while (svg?.ownerSVGElement) svg = svg.ownerSVGElement;
	return svg;
}

/** A day group of a picture, read: its axis in user units and in time, and where its slot begins across it. */
function readDay(svg: SVGSVGElement, el: Element) {
	const attr = (name: string) => el.getAttribute(name) ?? '';
	const y = attr('data-axis') === 'y';
	// The slot begins at its label group's origin (dayview_svg.rs), in
	// the root's user units as `data-lo` is.
	const label = svg.querySelector<SVGGraphicsElement>(`.day-label[data-day="${attr('data-day')}"]`);
	const [root, own] = [svg.getScreenCTM(), label?.getScreenCTM()];
	const origin = root && own ? root.inverse().multiply(own) : null;
	return {
		el,
		y,
		lo: Number(attr('data-lo')),
		hi: Number(attr('data-hi')),
		from: Date.parse(attr('data-from')),
		to: Date.parse(attr('data-to')),
		start: origin ? (y ? origin.e : origin.f) : -Infinity
	};
}

/** A point on screen in `svg`'s user units. */
function userPoint(svg: SVGSVGElement, x: number, y: number): DOMPoint | null {
	const ctm = svg.getScreenCTM();
	return ctm && new DOMPoint(x, y).matrixTransform(ctm.inverse());
}

/**
 * The instant under the pointer on the day `el` is drawn in — the day
 * group it sits inside, or, on the picture's ground, the day whose slot
 * the point is in (a gap belongs to the day above it). Clamped to the
 * day, so a click in the margin is the day's edge; `null` when the
 * picture has no time axis (nowview's snapshot). Read off the picture's
 * own geometry, so it holds at any scale the picture is shown at: the one
 * mapping the time cursor calls too; its line is placed by `placeOf`.
 */
export function timeAt(el: Element, clientX: number, clientY = 0): Date | null {
	const svg = pictureOf(el);
	const p = svg && userPoint(svg, clientX, clientY);
	if (!svg || !p) return null;
	const days = [...svg.querySelectorAll('[data-lo]')].map((d) => readDay(svg, d));
	const own = el.closest('[data-lo]');
	const day = days.find((d) => d.el === own) ?? days.findLast((d) => d.start <= (d.y ? p.x : p.y)) ?? days[0];
	if (!day || !(day.hi > day.lo)) return null;
	const f = Math.min(1, Math.max(0, ((day.y ? p.y : p.x) - day.lo) / (day.hi - day.lo)));
	return new Date(day.from + f * (day.to - day.from));
}

/**
 * Where on `svg` a selection is drawn: the day it belongs to, how far
 * along that day's axis it falls, and how much of the cross axis the day's
 * slot takes. `'snapshot'` for a picture with no time axis at all, `null`
 * when the selection is on none of the days drawn.
 *
 * A RELATIVE selection belongs to the day whose span ENDS at now — the
 * nowview's window — and is measured back from that end, so `-qq` lands
 * on the −10m gridline exactly and STAYS there: the window moves with the
 * minute and the line moves with it. Any other picture, the dayview's day
 * running to midnight, places it at the instant it currently is — and
 * «seven minutes ago» is a later instant every minute, so there the same
 * selection creeps FORWARD tick by tick.
 */
function placeOf(svg: SVGSVGElement, c: Cursor) {
	const days = [...svg.querySelectorAll('[data-lo]')].map((d) => readDay(svg, d));
	if (!days.length) return null;
	// No day with a span at all is the SNAPSHOT — which is not «the instant
	// is off this picture», the window a selection has been panned out of.
	if (!days.some((d) => d.hi > d.lo)) return 'snapshot' as const;
	const view = svg.viewBox.baseVal;
	const at = (i: number) => {
		const day = days[i];
		const perMs = (day.hi - day.lo) / (day.to - day.from);
		// A picture with no date column has no label group to read a slot
		// origin from, and `readDay` says so with `-Infinity` (which is what
		// makes `timeAt` fall back to it). The slot is then the whole
		// picture across, which for nowview's one day it is.
		const near = day.y ? view.x : view.y;
		const far = day.y ? view.x + view.width : view.y + view.height;
		const start = Number.isFinite(day.start) ? day.start : near;
		const next = days[i + 1]?.start;
		return { ...day, perMs, start, end: next !== undefined && Number.isFinite(next) ? next : far };
	};
	const now = c.rel === undefined ? null : nowAlong(svg, days[0].y);
	const ending = now === null ? -1 : days.findIndex((d) => d.hi > d.lo && Math.abs(d.hi - now) <= 1);
	if (ending >= 0) {
		const day = at(ending);
		return { ...day, along: day.hi - (c.rel ?? 0) * 60_000 * day.perMs };
	}
	const i = days.findIndex((d) => d.from <= +c.at && +c.at < d.to && d.hi > d.lo);
	if (i < 0) return null;
	const day = at(i);
	return { ...day, along: day.lo + (+c.at - day.from) * day.perMs };
}

/**
 * The picture's time axis ON SCREEN: where its span begins and ends in px
 * along the axis it runs, and the two instants those are.
 *
 * What a gesture over the band needs and `timeAt` cannot give it — how
 * much TIME a pixel is worth, for the length of a drag and the distance a
 * pan moves the window (da-rskp). Read through `readDay`, the same reader
 * `timeAt` and `placeOf` go through, so the three cannot come to disagree
 * about where the picture put the hour. `null` for a picture with no time
 * axis at all: nowview's snapshot.
 */
export function timeAxis(svg: SVGSVGElement): { y: boolean; from: number; to: number; lo: number; hi: number } | null {
	const el = svg.querySelector('[data-lo]');
	const ctm = el && svg.getScreenCTM();
	if (!el || !ctm) return null;
	const day = readDay(svg, el);
	if (!(day.hi > day.lo) || !(day.to > day.from)) return null;
	const screen = (u: number) => {
		const p = new DOMPoint(day.y ? 0 : u, day.y ? u : 0).matrixTransform(ctm);
		return day.y ? p.y : p.x;
	};
	return { y: day.y, from: day.from, to: day.to, lo: screen(day.lo), hi: screen(day.hi) };
}

/** The instant `pos` px along `axis` is, unclamped: a drag may run past the picture. */
export function instantOn(axis: { from: number; to: number; lo: number; hi: number }, pos: number): number {
	return axis.from + ((pos - axis.lo) / (axis.hi - axis.lo)) * (axis.to - axis.from);
}

/** How many milliseconds one pixel along `axis` is worth. */
export const msPerPx = (axis: { from: number; to: number; lo: number; hi: number }): number =>
	(axis.to - axis.from) / (axis.hi - axis.lo);

/** The element the selection's marks are drawn as, ready to append. */
function mark(name: string, attrs: Record<string, string | number>): SVGElement {
	const el = document.createElementNS('http://www.w3.org/2000/svg', name);
	for (const [key, value] of Object.entries(attrs)) el.setAttribute(key, String(value));
	el.setAttribute('pointer-events', 'none');
	return el;
}

/**
 * The selection's line: bright white over a dark casing.
 *
 * Yellow is NOW's alone (da-ex56), so white is the selection's everywhere
 * — this one function draws it for every picture, the nowview's and the
 * dayview's alike, which is what makes the two colours mean one thing each
 * across the page. Written here, not as a token: these are SVG attributes,
 * where `var()` does not reach.
 */
function dottedLine(a: string, b: string, along: number, from: number, to: number): SVGElement[] {
	return (
		[
			['cursor-casing', 'rgba(0, 0, 0, 0.6)', 3.5],
			['cursor', '#ffffff', 1.5]
		] as const
	).map(([cls, stroke, width]) =>
		mark('line', {
			class: `selection ${cls}`,
			[`${a}1`]: along,
			[`${a}2`]: along,
			[`${b}1`]: from,
			[`${b}2`]: to,
			stroke,
			'stroke-width': width,
			'stroke-dasharray': '3 3',
			'vector-effect': 'non-scaling-stroke'
		})
	);
}

/**
 * The arrow at the selection's head (da-rskp — vividn's «maybe … with a
 * little arrow pointing at it too??»): a small solid triangle sitting on
 * the day's leading edge and pointing down the line.
 *
 * Small on purpose: it says WHICH of two dotted lines is the selection
 * without becoming a third mark competing with them. In the picture's own
 * units, which a sized pane asks for at one unit per pixel.
 */
const HEAD = { half: 4, deep: 7 };

function arrowHead(y: boolean, along: number, start: number): SVGElement {
	const { half, deep } = HEAD;
	const points = y
		? [`${start},${along - half}`, `${start},${along + half}`, `${start + deep},${along}`]
		: [`${along - half},${start}`, `${along + half},${start}`, `${along},${start + deep}`];
	return mark('polygon', { class: 'selection cursor-head', points: points.join(' '), fill: '#ffffff' });
}

/**
 * Where the picture puts NOW along a day's axis, when it draws it at all.
 *
 * The CLI's own yellow line (`current-time-line`), read off the picture:
 * present only where now falls inside the window (da-5flt). It is what a
 * RELATIVE selection is measured back from — see `drawCursor`.
 */
function nowAlong(svg: SVGSVGElement, y: boolean): number | null {
	const line = svg.querySelector('.current-time-line');
	const box = line?.getBoundingClientRect();
	const p = box && userPoint(svg, box.left + box.width / 2, box.top + box.height / 2);
	return p ? (y ? p.y : p.x) : null;
}

/**
 * The selection on `svg`, replacing whatever was drawn before it: the
 * white dotted line where `placeOf` puts it, the arrow at its head, and —
 * when it has a length — the band of the range it covers, behind them both.
 *
 * A picture with no time axis at all is the nowview's SNAPSHOT: one
 * instant, no span to place anything along. There a selection is said by
 * drawing the line THROUGH the chips, top to bottom, so the snapshot reads
 * as «a selected time» rather than as now (da-rskp).
 */
function drawCursor(svg: SVGSVGElement | null, c: Cursor | null | undefined): void {
	svg?.querySelectorAll('.selection').forEach((el) => el.remove());
	if (!svg || !c) return;
	const place = placeOf(svg, c);
	if (!place) return;
	if (place === 'snapshot') return drawOnSnapshot(svg);
	const [a, b] = place.y ? ['y', 'x'] : ['x', 'y'];
	const along = place.along;
	// A window the selection has been panned out of draws none of it.
	if (along < place.lo - 0.5 || along > place.hi + 0.5) return;
	if (c.span) {
		// The range, behind the line that ends it: from `span` minutes
		// before the selection to the selection, clamped to the day drawn.
		const from = Math.max(place.lo, Math.min(place.hi, along - c.span * 60_000 * place.perMs));
		const [lo, hi] = [Math.min(from, along), Math.max(from, along)];
		svg.append(
			mark('rect', {
				class: 'selection cursor-span',
				[place.y ? 'y' : 'x']: lo,
				[place.y ? 'x' : 'y']: place.start,
				[place.y ? 'height' : 'width']: Math.max(0, hi - lo),
				[place.y ? 'width' : 'height']: Math.max(0, place.end - place.start),
				fill: 'rgba(255, 255, 255, 0.22)'
			}),
			...dottedLine(a, b, from, place.start, place.end)
		);
	}
	svg.append(...dottedLine(a, b, along, place.start, place.end), arrowHead(place.y, along, place.start));
}

/**
 * The snapshot's line: the same white dashes, drawn through the middle of
 * the chips rather than at an instant, because the snapshot HAS no axis to
 * put one on. A selection at now is not drawn at all (it is not a
 * selection, $lib/cursor.ts), so a line here always means «not now».
 *
 * The chips are the CLI's own `.now-chunk` rects, so their middle is read
 * off the picture rather than guessed from a canvas width.
 */
function drawOnSnapshot(svg: SVGSVGElement): void {
	const chunks = [...svg.querySelectorAll('.now-chunk')].map((r) => r.getBoundingClientRect());
	if (!chunks.length) return;
	const middle = (Math.min(...chunks.map((r) => r.left)) + Math.max(...chunks.map((r) => r.right))) / 2;
	const p = userPoint(svg, middle, 0);
	if (!p) return;
	const view = svg.viewBox.baseVal;
	svg.append(...dottedLine('x', 'y', p.x, view.y, view.y + view.height), arrowHead(false, p.x, view.y));
}

/** Where a block's drawing begins: its start, or its day's if it began before. */
function drawnStart(mark: Element): Date {
	const from = (el: Element | null) => Date.parse(el?.getAttribute('data-from') ?? '');
	return new Date(Math.max(from(mark), from(mark.closest('[data-lo]'))));
}

/** A mark's own time: a dot's instant, or where a block is drawn from. */
function markTime(mark: Element): Date {
	const at = mark.getAttribute('data-at');
	return at ? new Date(at) : drawnStart(mark);
}

/**
 * The label for a mark: a time, then the mark's title. A dot's time
 * is its own; a block's is the one under `clientX`, or where it is drawn
 * from when there is no pointer — as when it has the keyboard's focus.
 */
export function tipText(mark: Element, clientX?: number): string {
	const when = mark.hasAttribute('data-at') || clientX === undefined ? markTime(mark) : timeAt(mark, clientX);
	const title = mark.getAttribute('data-title') ?? '';
	return when ? `${clockLabel(when)} ${title}` : title;
}

/**
 * `text` as an `<svg>` for the page, or `null` when it is not one. The
 * markup is datum's own, but it carries whatever text was logged, so
 * nothing in it that could run is kept: no script, no foreign content, no
 * `on…` handler, no link that leaves the picture. No `<title>` either: its
 * text becomes its mark's `data-title` and, as the `<title>` was, its
 * accessible name.
 */
export function inlineSvg(text: string): SVGSVGElement | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	const svg = doc.documentElement;
	if (!(svg instanceof SVGSVGElement) || doc.querySelector('parsererror')) return null;
	for (const el of svg.querySelectorAll('script, foreignObject')) el.remove();
	for (const title of svg.querySelectorAll('title')) {
		const mark = title.closest('[data-state]');
		mark?.setAttribute('data-title', title.textContent ?? '');
		mark?.setAttribute('aria-label', title.textContent ?? '');
		title.remove();
	}
	for (const el of [svg, ...svg.querySelectorAll('*')]) {
		for (const { name, value } of [...el.attributes]) {
			if (/^on/i.test(name) || (/href$/i.test(name) && !value.startsWith('#'))) {
				el.removeAttribute(name);
			}
		}
	}
	return document.importNode(svg, true);
}

let tips = 0;

/** How far a press may travel and still be a click, in pixels: past it, it was a drag or a scroll. */
const CLICK_SLOP = 5;

/**
 * Whether something ABOVE this picture takes the pointer itself.
 *
 * The nowview's band does (da-rskp): a press there is one of four
 * gestures, and what a tap on it MEANS is a relative selection that does
 * not move the window — not the absolute instant this action sets. One
 * attribute and no import either way, the same idiom as `data-pans`
 * (PaneFrame.svelte). Asked at the press rather than at mount, because a
 * picture outlives the trees it is drawn in.
 */
const ownsPointer = (node: HTMLElement) => !!node.closest('[data-points="own"]');

/**
 * Svelte action: draw `text` into `node` and make its marks answer a
 * pointer, a tap and the keyboard with a label — the time and what the
 * mark is. Nothing is shown over empty space.
 *
 * And the time cursor (da-wnt2, $lib/cursor.ts): a click anywhere on a
 * day — a mark or its ground — points the page at the time under it,
 * Enter on a mark at the mark's own time, and wherever the cursor falls
 * on one of the picture's days a dotted line marks it, redrawn with the
 * picture.
 */
export function picture(node: HTMLElement, text: string) {
	const tip = document.createElement('div');
	tip.className = 'tip';
	tip.id = `picture-tip-${++tips}`;
	tip.setAttribute('role', 'tooltip');
	tip.hidden = true;

	const own = () => ownsPointer(node);

	let shown: Element | null = null;
	const marks = () => [...node.querySelectorAll('[data-state]')];
	const markAt = (target: EventTarget | null) =>
		target instanceof Element ? target.closest('[data-state]') : null;

	function show(mark: Element | null, x = 0, y = 0, clientX?: number) {
		shown?.removeAttribute('aria-describedby');
		shown = mark;
		tip.hidden = !mark;
		if (!mark) return;
		mark.setAttribute('aria-describedby', tip.id);
		tip.textContent = tipText(mark, clientX);
		// Beside the pointer, and turned back inside the window at its
		// edges — a phone's included.
		// Measured from a known origin. The tip is `position: fixed` with no
		// offsets until the two lines below, so until then it sits at its
		// STATIC position — and a shrink-to-fit box there is only as wide as
		// the room to the right of it. In the day strip, whose bands start
		// after the date column (da-iu9i), that read ~70px narrow and the
		// clamp let the label off the right of a 390px screen.
		tip.style.left = '0px';
		tip.style.top = '0px';
		const { width, height } = tip.getBoundingClientRect();
		tip.style.left = `${Math.max(4, Math.min(x + 12, innerWidth - width - 4))}px`;
		tip.style.top = `${y + 16 + height <= innerHeight ? y + 16 : Math.max(4, y - height - 8)}px`;
	}

	function draw(text: string) {
		const focused = marks().indexOf(document.activeElement as Element);
		show(null);
		const svg = inlineSvg(text);
		svg?.classList.add('svg');
		svg?.setAttribute('role', 'group');
		node.replaceChildren(...(svg ? [svg] : []), tip);
		const drawn = marks();
		for (const mark of drawn) mark.setAttribute('tabindex', '0');
		// A redraw replaces every mark; the keyboard keeps its place.
		(drawn[focused] as SVGElement | undefined)?.focus();
		drawCursor(svg, sel);
	}

	let sel: Cursor | null = null;
	let down = { x: NaN, y: NaN };
	const press = (e: PointerEvent) => (down = { x: e.clientX, y: e.clientY });
	const click = (e: MouseEvent) => {
		if (own()) return;
		if (Math.hypot(e.clientX - down.x, e.clientY - down.y) > CLICK_SLOP) return;
		const when = e.target instanceof Element && timeAt(e.target, e.clientX, e.clientY);
		if (when) setCursor(when);
	};
	// Enter on a focused mark selects the mark's own time — and it does so
	// even where the POINTER is somebody else's. The nowview's band takes
	// clicks (`data-points="own"`), and returning early here took the
	// keyboard's way to select with them: on main Enter in the band made
	// `-t 14:30`, and on the first head of da-rskp it made no pill at all
	// (vremya on PR #48). There the selection is marked as the band's, so
	// it leaves the window where it is — the same rule a tap there follows.
	const enter = (e: KeyboardEvent) => {
		const mark = e.key === 'Enter' && markAt(e.target);
		if (!mark) return;
		if (own()) pointAt({ at: markTime(mark), from: 'nowview' });
		else setCursor(markTime(mark));
	};

	const point = (e: PointerEvent) => show(markAt(e.target), e.clientX, e.clientY, e.clientX);
	// A finger lifting leaves the element too; its label stays until the
	// next tap lands somewhere else.
	const leave = (e: PointerEvent) => e.pointerType !== 'touch' && show(null);
	const focus = (e: FocusEvent) => {
		const mark = markAt(e.target);
		const box = mark?.getBoundingClientRect();
		show(mark, box?.left, box?.bottom);
	};
	const blur = () => show(null);
	const away = (e: PointerEvent) => !node.contains(e.target as Node) && show(null);

	draw(text);
	node.addEventListener('pointermove', point);
	node.addEventListener('pointerdown', point);
	node.addEventListener('pointerleave', leave);
	node.addEventListener('focusin', focus);
	node.addEventListener('focusout', blur);
	document.addEventListener('pointerdown', away);
	node.addEventListener('pointerdown', press);
	node.addEventListener('click', click);
	node.addEventListener('keydown', enter);
	const unsubscribe = pointed.subscribe((c) => {
		sel = c ?? null;
		drawCursor(node.querySelector('svg'), sel);
	});
	return {
		update: draw,
		destroy: () => {
			document.removeEventListener('pointerdown', away);
			unsubscribe();
		}
	};
}
