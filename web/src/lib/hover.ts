// What a drawn view says under the pointer (da-s5da).
//
// datum draws dayview and nowview as SVG and names every mark in it: a
// `<title>` that reads the row the way the tail does (`work: coding
// 14:00–14:30`), its state and instants as `data-` attributes, and on
// each day group the span of time the day's axis covers (`data-from` →
// `data-to` over `data-lo` → `data-hi` user units along `data-axis`). The
// page adds the one thing a picture cannot know, which is where the
// pointer is — `timeAt` — and shows a label with it.

import { clockLabel } from './clock.js';

/** The outermost `<svg>` an element is drawn in. */
function pictureOf(el: Element): SVGSVGElement | null {
	let svg = el.closest('svg');
	while (svg?.ownerSVGElement) svg = svg.ownerSVGElement;
	return svg;
}

/**
 * The instant under the pointer on the day `el` is drawn in — the day
 * group it sits inside, or the picture's first day when `el` is the
 * picture itself. Clamped to the day; `null` when the picture has no time
 * axis (nowview's snapshot). Read off the picture's own geometry, so it
 * holds at any scale the picture is shown at: the one mapping the
 * click-to-time feature calls too.
 */
export function timeAt(el: Element, clientX: number, clientY = 0): Date | null {
	const svg = pictureOf(el);
	const day = el.closest('[data-lo]') ?? svg?.querySelector('[data-lo]');
	if (!svg || !day) return null;
	const num = (name: string) => Number(day.getAttribute(name));
	const [lo, hi] = [num('data-lo'), num('data-hi')];
	if (!(hi > lo)) return null;
	const box = svg.getBoundingClientRect();
	const view = svg.viewBox.baseVal;
	const along =
		day.getAttribute('data-axis') === 'y'
			? view.y + ((clientY - box.top) * view.height) / box.height
			: view.x + ((clientX - box.left) * view.width) / box.width;
	const from = Date.parse(day.getAttribute('data-from') ?? '');
	const to = Date.parse(day.getAttribute('data-to') ?? '');
	const f = Math.min(1, Math.max(0, (along - lo) / (hi - lo)));
	return new Date(from + f * (to - from));
}

/** Where a block's drawing begins: its start, or its day's if it began before. */
function drawnStart(mark: Element): Date {
	const from = (el: Element | null) => Date.parse(el?.getAttribute('data-from') ?? '');
	return new Date(Math.max(from(mark), from(mark.closest('[data-lo]'))));
}

/**
 * The label for a mark: a time, then the mark's `<title>`. A dot's time
 * is its own; a block's is the one under `clientX`, or where it is drawn
 * from when there is no pointer — as when it has the keyboard's focus.
 */
export function tipText(mark: Element, clientX?: number): string {
	const at = mark.getAttribute('data-at');
	const when = at ? new Date(at) : clientX === undefined ? drawnStart(mark) : timeAt(mark, clientX);
	const title = mark.querySelector('title')?.textContent ?? '';
	return when ? `${clockLabel(when)} ${title}` : title;
}

/**
 * `text` as an `<svg>` for the page, or `null` when it is not one. The
 * markup is datum's own, but it carries whatever text was logged, so
 * nothing in it that could run is kept: no script, no foreign content, no
 * `on…` handler, no link that leaves the picture.
 */
export function inlineSvg(text: string): SVGSVGElement | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	const svg = doc.documentElement;
	if (!(svg instanceof SVGSVGElement) || doc.querySelector('parsererror')) return null;
	for (const el of svg.querySelectorAll('script, foreignObject')) el.remove();
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

/**
 * Svelte action: draw `text` into `node` and make its marks answer a
 * pointer, a tap and the keyboard with a label — the time and what the
 * mark is. Nothing is shown over empty space.
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
	}

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
	return {
		update: draw,
		destroy: () => document.removeEventListener('pointerdown', away)
	};
}
