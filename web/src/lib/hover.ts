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
import { cursor, setCursor } from './cursor.js';

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

/** Where on `svg` the instant `at` is drawn — the day it falls on, and how far along — or `null` if on none. */
function placeOf(svg: SVGSVGElement, at: Date) {
	const days = [...svg.querySelectorAll('[data-lo]')].map((d) => readDay(svg, d));
	const i = days.findIndex((d) => d.from <= +at && +at < d.to && d.hi > d.lo);
	if (i < 0) return null;
	const day = days[i];
	const along = day.lo + ((+at - day.from) / (day.to - day.from)) * (day.hi - day.lo);
	const view = svg.viewBox.baseVal;
	const end = days[i + 1]?.start ?? (day.y ? view.x + view.width : view.y + view.height);
	return { ...day, along, end };
}

/** The cursor's dotted line across the day of `svg` it falls on, replacing any before it. */
function drawCursor(svg: SVGSVGElement | null, at: Date | undefined): void {
	svg?.querySelector('line.cursor')?.remove();
	const place = svg && at && placeOf(svg, at);
	if (!place) return;
	const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
	const [a, b] = place.y ? ['y', 'x'] : ['x', 'y'];
	for (const [name, value] of Object.entries({
		class: 'cursor',
		[`${a}1`]: place.along,
		[`${a}2`]: place.along,
		[`${b}1`]: place.start,
		[`${b}2`]: place.end,
		// The time cursor's own blue (PromptBar.svelte's `.pill.time`), not a
		// token: this is an SVG attribute, where `var()` does not reach.
		stroke: '#7c9cff',
		'stroke-width': 1,
		'stroke-dasharray': '3 3',
		'vector-effect': 'non-scaling-stroke',
		'pointer-events': 'none'
	})) {
		line.setAttribute(name, String(value));
	}
	svg.append(line);
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
		drawCursor(svg, at);
	}

	let at: Date | undefined;
	let down = { x: NaN, y: NaN };
	const press = (e: PointerEvent) => (down = { x: e.clientX, y: e.clientY });
	const click = (e: MouseEvent) => {
		if (Math.hypot(e.clientX - down.x, e.clientY - down.y) > CLICK_SLOP) return;
		const when = e.target instanceof Element && timeAt(e.target, e.clientX, e.clientY);
		if (when) setCursor(when);
	};
	const enter = (e: KeyboardEvent) => {
		const mark = e.key === 'Enter' && markAt(e.target);
		if (mark) setCursor(markTime(mark));
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
	const unsubscribe = cursor.subscribe((c) => {
		at = c?.at;
		drawCursor(node.querySelector('svg'), at);
	});
	return {
		update: draw,
		destroy: () => {
			document.removeEventListener('pointerdown', away);
			unsubscribe();
		}
	};
}
