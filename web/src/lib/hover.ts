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

import { get } from 'svelte/store';
import { fieldPill, pills, removePill, select, selected } from './bar.js';
import { clockLabel } from './clock.js';
import { pointAt, pointed, setCursor, type Cursor } from './cursor.js';
import {
	CLICK_SLOP,
	hidOf,
	inspectMods,
	markOf,
	marksIn,
	pressSpent,
	pressTraveled,
	spendPress,
	tipOf,
	type Tip
} from './inspect.js';
import { readLanes } from './lanes.js';
import { isSelected } from './selection.js';

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

type Day = ReturnType<typeof readDay>;

/**
 * Which stretch of `day`'s day a point is over (da-1v9gp).
 *
 * A day the clock jumped in is one group per UTC offset it was lived at,
 * each with a straight axis of its own. Clock times a jump skipped are
 * under none of them, and the point reads as the nearest end; ones a jump
 * repeated are under two, each in a lane its own `.lane` path bounds (drawn
 * with no stroke), and the point is in one of them.
 */
function stretchAt(days: Day[], day: Day, along: number, clientX: number, clientY: number): Day {
	const same = days.filter((d) => d.el.getAttribute('data-day') === day.el.getAttribute('data-day'));
	const under = same.filter((d) => d.lo <= along && along <= d.hi);
	const inLane = (d: Day) => {
		const lane = d.el.querySelector<SVGGeometryElement>(':scope > .lane');
		const ctm = lane?.getScreenCTM();
		return !!lane && !!ctm && lane.isPointInFill(new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse()));
	};
	const off = (d: Day) => Math.max(d.lo - along, along - d.hi);
	return under.find(inLane) ?? under.at(-1) ?? same.reduce((a, b) => (off(b) < off(a) ? b : a), day);
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
 *
 * A mark is asked about its own stretch of the day; anything else — the
 * ground, the day the strip holds for a gesture — about the stretch the
 * point is over (`stretchAt`).
 */
export function timeAt(el: Element, clientX: number, clientY = 0): Date | null {
	const svg = pictureOf(el);
	const p = svg && userPoint(svg, clientX, clientY);
	if (!svg || !p) return null;
	const days = [...svg.querySelectorAll('[data-lo]')].map((d) => readDay(svg, d));
	const own = el.closest('[data-lo]');
	const found = days.find((d) => d.el === own) ?? days.findLast((d) => d.start <= (d.y ? p.x : p.y)) ?? days[0];
	if (!found) return null;
	const along = found.y ? p.y : p.x;
	const day = own && own !== el ? found : stretchAt(days, found, along, clientX, clientY);
	if (!(day.hi > day.lo)) return null;
	const f = Math.min(1, Math.max(0, (along - day.lo) / (day.hi - day.lo)));
	return new Date(day.from + f * (day.to - day.from));
}

/** Day `i` of `days` drawn on `svg`, with how much its axis moves a millisecond and the cross axis its slot takes. */
function slotOf(svg: SVGSVGElement, days: Day[], i: number) {
	const view = svg.viewBox.baseVal;
	const day = days[i];
	const perMs = (day.hi - day.lo) / (day.to - day.from);
	// A picture with no date column has no label group to read a slot
	// origin from, and `readDay` says so with `-Infinity` (which is what
	// makes `timeAt` fall back to it). The slot is then the whole
	// picture across, which for nowview's one day it is.
	const near = day.y ? view.x : view.y;
	const far = day.y ? view.x + view.width : view.y + view.height;
	const start = Number.isFinite(day.start) ? day.start : near;
	// The next DAY's: a day the clock jumped in is several groups (da-1v9gp).
	const next = days.slice(i + 1).find((d) => d.start !== day.start)?.start;
	return { ...day, perMs, start, end: next !== undefined && Number.isFinite(next) ? next : far };
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
	const at = (i: number) => slotOf(svg, days, i);
	const now = c.rel === undefined ? null : nowAlong(svg, days[0].y);
	const ending = now === null ? -1 : days.findIndex((d) => d.hi > d.lo && Math.abs(d.hi - now) <= 1);
	if (ending >= 0) {
		const day = at(ending);
		return { ...day, along: day.hi - (c.rel ?? 0) * 60_000 * day.perMs };
	}
	// A picture drawn AHEAD of now (nowview's `--ahead`, da-hklaz) runs its
	// day on past the line, so the line is INSIDE the day and not at its
	// end; it stays put while the picture slides under it, and a relative
	// selection is measured back from it all the same.
	if (now !== null && svg.querySelector('.flow')) {
		const ahead = days.findIndex((d) => d.hi > d.lo && d.lo <= now && now < d.hi);
		if (ahead >= 0) {
			const day = at(ahead);
			return { ...day, along: now - (c.rel ?? 0) * 60_000 * day.perMs };
		}
	}
	// A nowview's strip cuts its axis at now (da-dv45l): a selection there is
	// the end of the stretch before it.
	const within = (d: Day) => d.hi > d.lo && d.from <= +c.at && +c.at < d.to;
	const i = Math.max(days.findIndex(within), days.findIndex((d) => d.hi > d.lo && d.to === +c.at));
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
 * axis at all: nowview's snapshot. Of the stretch holding `at` where there
 * are several, the first otherwise.
 */
export function timeAxis(
	svg: SVGSVGElement,
	at?: number
): { y: boolean; from: number; to: number; lo: number; hi: number } | null {
	const days = [...svg.querySelectorAll('[data-lo]')].map((el) => readDay(svg, el));
	const ctm = days.length && svg.getScreenCTM();
	if (!ctm) return null;
	// The stretch that holds `at`: a nowview cut by its now strip (da-9g3bf,
	// da-6csum) draws the time past the strip a strip further on, so a line
	// through the stretch before it misses the one after by that much.
	const day = (at === undefined ? undefined : days.find((d) => d.from <= at && at <= d.to)) ?? days[0];
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
 * RELATIVE selection is measured back from — see `drawCursor`. Beside a
 * now strip the line stands a gap past now, and says where now is in
 * `data-now` (da-9g3bf).
 */
function nowAlong(svg: SVGSVGElement, y: boolean): number | null {
	const line = svg.querySelector('.current-time-line');
	const at = line?.getAttribute('data-now');
	if (at) return Number(at);
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
 * The stretch from `since` to now dimmed on `svg` (da-g5jjp): what another
 * device may hold that this copy has not heard yet, until the session's
 * first sync lands. Over the marks, which are as uncertain as the ground,
 * and under the selection, which stays bright over it.
 */
function drawUnsynced(svg: SVGSVGElement | null, since: number | undefined): void {
	svg?.querySelectorAll('.unsynced').forEach((el) => el.remove());
	if (!svg || since === undefined) return;
	const days = [...svg.querySelectorAll('[data-lo]')].map((d) => readDay(svg, d));
	days.forEach((d, i) => {
		const [from, to] = [Math.max(since, d.from), Math.min(Date.now(), d.to)];
		if (!(to > from && d.hi > d.lo)) return;
		const s = slotOf(svg, days, i);
		const dim = mark('rect', {
			class: 'unsynced',
			[s.y ? 'y' : 'x']: s.lo + (from - s.from) * s.perMs,
			[s.y ? 'x' : 'y']: s.start,
			[s.y ? 'height' : 'width']: (to - from) * s.perMs,
			[s.y ? 'width' : 'height']: Math.max(0, s.end - s.start),
			fill: 'rgba(0, 0, 0, 0.5)'
		});
		svg.insertBefore(dim, svg.querySelector('.selection'));
	});
}

/**
 * The snapshot's line: the same white dashes, drawn through the middle of
 * the chips rather than at an instant, because the snapshot HAS no axis to
 * put one on. A selection at now is not drawn at all (it is not a
 * selection, #lib/cursor.ts), so a line here always means «not now».
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
 * It is told on the clock the mark is drawn on (da-3ctgr): its stretch's
 * `data-offset`, the one a day was lived by, and the reader's own where
 * the picture names none, as nowview's does not.
 */
export function tipText(mark: Element, clientX?: number): string {
	const when = mark.hasAttribute('data-at') || clientX === undefined ? markTime(mark) : timeAt(mark, clientX);
	const title = mark.getAttribute('data-title') ?? '';
	const offset = mark.closest('[data-lo]')?.getAttribute('data-offset');
	return when ? `${clockLabel(when, offset ? Number(offset) : undefined)} ${title}` : title;
}

/**
 * `text` as an `<svg>` for the page, or `null` when it is not one. The
 * markup is datum's own, but it carries whatever text was logged, so
 * nothing in it that could run is kept: no script, no foreign content, no
 * `on…` handler, no link that leaves the picture, no `<style>` and no
 * animation that writes a handler or a link — and, in a field's icon, nothing
 * that could fetch. No `<title>` either: its
 * text becomes its mark's `data-title` and, as the `<title>` was, its
 * accessible name.
 */
export function inlineSvg(text: string): SVGSVGElement | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	const svg = doc.documentElement;
	if (!(svg instanceof SVGSVGElement) || doc.querySelector('parsererror')) return null;
	// A field's icon (da-45wgz) is a person's own SVG, vetted by the CLI;
	// this is the second layer, with the CLI's rules: no `<style>` (it is
	// page-wide once inlined), and no `<set>`/`<animate>` that writes a
	// handler or a link later.
	for (const el of svg.querySelectorAll('script, foreignObject, style')) el.remove();
	for (const el of svg.querySelectorAll('set, animate, animateMotion, animateTransform')) {
		if (/^on|href$/i.test(el.getAttribute('attributeName') ?? '')) el.remove();
	}
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
	// A field's icon is the one part a person wrote: nothing in it that could
	// fetch, as the CLI's `check_icon` holds it (`fetches` there).
	for (const el of svg.querySelectorAll('defs > svg[id^="icon-"], defs > svg[id^="icon-"] *')) {
		for (const { name, value } of [...el.attributes]) if (fetches(value)) el.removeAttribute(name);
	}
	return document.importNode(svg, true);
}

/** The functions an icon draws with; `url()` is let through only to a `#fragment`. */
const DRAWS = new Set(['matrix', 'translate', 'scale', 'rotate', 'skewx', 'skewy', 'rgb', 'rgba', 'hsl', 'hsla']);

/**
 * Whether an attribute value could make the browser fetch (da-45wgz): any
 * CSS escape — `\75 rl(` IS `url(` — and any function but a transform's, a
 * colour's or a `url(#…)`, since `image-set('…')` fetches with no `url(`.
 */
function fetches(value: string): boolean {
	const v = value.toLowerCase();
	if (v.includes('\\')) return true;
	for (const m of v.matchAll(/([a-z0-9-]*)\s*\(\s*['"]?/g)) {
		if (m[1] === 'url' ? !v.startsWith('#', m.index + m[0].length) : !DRAWS.has(m[1])) return true;
	}
	return false;
}

/**
 * A FIELD PILL brought out in the picture (da-8810n).
 *
 * vividn, on what a view that cannot filter should do instead: "maybe a
 * highlight outline, so that all the occurences or data of that field is
 * brought out a little bit. nothing too intense, it must still be possible
 * to see all the other data well." So it is an outline around the marks
 * THAT field drew, and nothing at all is done to the others: no dimming,
 * no reordering, no second colour — a reader of a narrowed dayview still
 * reads the whole day.
 *
 * `data-pilled` on the CLI's own `.field[data-field]` groups, and the halo
 * is CSS (`CliOutput.svelte`). No geometry is invented and no field name is
 * guessed: the group already says which field it is, exactly as
 * #lib/lanes.ts reads it.
 */
function highlight(svg: SVGSVGElement | null, field: string | null): void {
	svg?.querySelectorAll('.field[data-pilled]').forEach((g) => g.removeAttribute('data-pilled'));
	if (!svg || field === null) return;
	for (const g of svg.querySelectorAll(`.field[data-field="${CSS.escape(field)}"]`))
		g.setAttribute('data-pilled', '');
}

/**
 * THE SELECTED ENTRIES, brought out in the picture (da-pzzoz).
 *
 * vividn: «when a piece of data is selected through other means (like
 * clicking in tail), then it should be highlighted in the dayview (and
 * nowview if that particular piece of data is represented there)». So this
 * is driven by the bar's `selected` store and by nothing a picture did:
 * every surface that selects an entry — a tail row, a document in the
 * CLI widget, a double tap on a mark — reaches every picture through it.
 *
 * «the field outline will highlight all data in that field, the hid will
 * highlight just one block, I suppose they should look the same» — so
 * `data-selected` wears the SAME halo `data-pilled` does, one rule for
 * both in CliOutput.svelte, and nothing is done to the marks around it.
 */
function bringOut(svg: SVGSVGElement | null, hids: string[]): void {
	svg?.querySelectorAll('[data-selected]').forEach((m) => m.removeAttribute('data-selected'));
	if (!svg || !hids.length) return;
	for (const mark of marksIn(svg)) {
		const hid = hidOf(mark);
		if (hid && isSelected(hids, hid)) mark.setAttribute('data-selected', '');
	}
}

/**
 * A mark's box as it is SEEN: one that flows (nowview's `--ahead`, da-hklaz)
 * is drawn on past the now-line, where its clip hides it, and its box would
 * take the tooltip there — so it stops at the line.
 */
function shownBox(mark: Element): DOMRect {
	const box = mark.getBoundingClientRect();
	const line = mark.closest('.flow') && mark.closest('svg')?.querySelector('.current-time-line')?.getBoundingClientRect();
	if (!line) return box;
	if (mark.closest('[data-axis]')?.getAttribute('data-axis') === 'y')
		return new DOMRect(box.x, box.y, box.width, Math.max(0, Math.min(box.bottom, line.top) - box.y));
	return new DOMRect(box.x, box.y, Math.max(0, Math.min(box.right, line.left) - box.x), box.height);
}

/**
 * The tip for the entries selected in `root`, and the box their marks
 * cover on screen; `null` when none of them is drawn here.
 *
 * What the nowview floats its tooltip over (da-pzzoz, `NowviewWidget`),
 * read here because this is the module that knows what a mark is.
 */
export function selectedTip(root: ParentNode, hids: string[]): { tip: Tip; box: DOMRect } | null {
	if (!hids.length) return null;
	const marks = marksIn(root).filter((m) => {
		const hid = hidOf(m);
		return hid && isSelected(hids, hid);
	});
	if (!marks.length) return null;
	const tip = tipOf(
		marks.map((m) => ({
			field: m.getAttribute('data-field') ?? m.closest('.field')?.getAttribute('data-field') ?? null,
			state: m.getAttribute('data-state'),
			from: m.getAttribute('data-from'),
			to: m.getAttribute('data-to'),
			at: m.getAttribute('data-at')
		}))
	);
	if (!tip) return null;
	const boxes = marks.map(shownBox);
	const [left, right] = [Math.min(...boxes.map((b) => b.left)), Math.max(...boxes.map((b) => b.right))];
	const [top, bottom] = [Math.min(...boxes.map((b) => b.top)), Math.max(...boxes.map((b) => b.bottom))];
	return { tip, box: new DOMRect(left, top, right - left, bottom - top) };
}

/**
 * Select the entry `mark` names, as a tap on it in a picture does: the
 * hid pill, and the time pill it replaces taken away.
 *
 * vividn: a tap «in dayview or nowview selects an existing piece of data
 * and creates an hid pill RATHER THAN a time pill» (da-pzzoz, where it was
 * a double tap; a single one since da-xg1vi). So the cursor is cleared
 * here, and the press is marked spent so the click and the lift that
 * share this finger do not make another.
 *
 * `false` over ground that names no entry, and on an entry ALREADY
 * selected: the caller then does whatever it would have done, which
 * everywhere is «select the time», and no gesture is ever dead. The second
 * is vividn's, 2026-10-22 (da-xg1vi): «first tap selects the piece of data,
 * second tap then selects time instead». Not with ctrl or ⌘, which toggle.
 */
export function inspectMark(
	mark: Element | null,
	e: { ctrlKey: boolean; metaKey: boolean },
	root: ParentNode | null = pictureOf(mark ?? document.body)
): boolean {
	const hid = hidOf(mark);
	if (!hid || !mark) return false;
	if (!e.ctrlKey && !e.metaKey && get(selected).includes(hid)) {
		// INSTEAD: the entry goes, unless a locked pill is holding it.
		if (!get(pills).find((p) => p.id === 'hid')?.locked) removePill('hid');
		return false;
	}
	const order = root ? marksIn(root).map(hidOf).filter((h): h is string => !!h) : [hid];
	pointAt(null);
	select(hid, inspectMods(e), order);
	spendPress();
	return true;
}

let tips = 0;

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
 * And the time cursor (da-wnt2, #lib/cursor.ts): a tap on a day's ground,
 * a right click, or a tap on the entry already selected points the page at
 * the time under it — a tap on a mark selects its entry (da-xg1vi) — Enter
 * on a mark at the mark's own time, and wherever the cursor falls
 * on one of the picture's days a dotted line marks it, redrawn with the
 * picture.
 */
export function picture(node: HTMLElement, { text, unsynced }: { text: string; unsynced?: number }) {
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
		const labeled = !!shown && shown === document.activeElement;
		show(null);
		const svg = inlineSvg(text);
		svg?.classList.add('svg');
		svg?.setAttribute('role', 'group');
		// A mark's own transform goes to a group around it, drawn the same:
		// the day strip scales a mark back about its own middle (da-dqbso),
		// which a transform on the mark itself would be composed inside.
		for (const el of svg?.querySelectorAll('.point-fg[transform]') ?? []) {
			const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
			g.setAttribute('transform', el.getAttribute('transform')!);
			el.removeAttribute('transform');
			el.replaceWith(g);
			g.append(el);
		}
		node.replaceChildren(...(svg ? [svg] : []), tip);
		const drawn = marks();
		for (const mark of drawn) mark.setAttribute('tabindex', '0');
		// A redraw replaces every mark; the keyboard keeps its place, and its
		// label only if it had one: a mark a press focused and a pan then
		// took the label off stays unlabeled (da-tke7i).
		(drawn[focused] as SVGElement | undefined)?.focus();
		if (!labeled) show(null);
		drawUnsynced(svg, unsynced);
		drawCursor(svg, sel);
		highlight(svg, pilled);
		bringOut(svg, chosen);
	}

	/**
	 * A redraw that lands while a press is down on the picture waits for
	 * the lift (da-hn2w6). It replaces the element the press went down on,
	 * and the browser then has nothing to dispatch the click at: the press
	 * set no cursor and said nothing. Drawn a task after the lift, which is
	 * after its click.
	 */
	let pressed = false;
	let late: string | null = null;
	let lifting: ReturnType<typeof setTimeout> | undefined;
	const hold = (e: PointerEvent) => {
		// Only the press a click comes of: a right press's lift may go to
		// the browser's menu and never arrive.
		if (e.button !== 0) return;
		clearTimeout(lifting);
		pressed = true;
	};
	const lift = () => {
		if (!pressed) return;
		clearTimeout(lifting);
		lifting = setTimeout(() => {
			pressed = false;
			if (late !== null) draw(late);
			late = null;
		});
	};
	const LIFTS = ['pointerup', 'pointercancel', 'contextmenu'];

	let sel: Cursor | null = null;
	/** The field the bar's pill names, as the last change left it. */
	let pilled: string | null = null;
	/** The entries the bar's hid pill names, as the last change left it. */
	let chosen: string[] = [];
	let down = { x: NaN, y: NaN };
	const press = (e: PointerEvent) => void (down = { x: e.clientX, y: e.clientY });

	/** The instant under a pointer event, on the day it landed in. */
	const timeOf = (e: MouseEvent) => (e.target instanceof Element ? timeAt(e.target, e.clientX, e.clientY) : null);

	/**
	 * The fields drawn at the height of `e` come first in the fields widget
	 * (da-qu16) — published after the selection, which is what clears them
	 * again.
	 */
	function lanesAt(e: MouseEvent, at: Date | null): void {
		const svg = node.querySelector<SVGSVGElement>('svg');
		if (svg) readLanes(svg, { x: e.clientX, y: e.clientY, t: at && +at });
	}

	/** Point the page at the time under `e`. */
	function pointTime(e: MouseEvent): boolean {
		const at = timeOf(e);
		if (!at) return false;
		setCursor(at);
		lanesAt(e, at);
		return true;
	}

	/**
	 * The right click is a TIME, wherever it lands (da-xg1vi, vividn: «long
	 * tap / right click → select a time»). On `contextmenu`, the one of a
	 * right press's events with a default to suppress. A band that takes the
	 * pointer itself (the nowview's) answers it there.
	 */
	const chord = (e: MouseEvent) => {
		if (pressSpent() || own()) return;
		if (Math.hypot(e.clientX - down.x, e.clientY - down.y) > CLICK_SLOP) return;
		if (!pointTime(e)) return;
		spendPress();
		e.preventDefault();
	};

	// A tap: the entry under it, or the time where there is none (da-xg1vi,
	// vividn: «single tap → select a piece of data; single tap where there
	// is no data … → select a time»). Not a press another listener already
	// answered, and not one that traveled: that was a pan or a scroll.
	const click = (e: MouseEvent) => {
		if (pressSpent() || own() || pressTraveled()) return;
		if (inspectMark(markOf(e.target), e, node)) return lanesAt(e, timeOf(e));
		pointTime(e);
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

	// A pan never labels what it passes over (da-xg1vi): a drag that
	// traveled hides the label, and a FINGER's label waits for its lift,
	// since its press may be the start of a scroll.
	const point = (e: PointerEvent) =>
		(e.buttons && pressTraveled()) || (e.type === 'pointerdown' && e.pointerType === 'touch')
			? show(null)
			: show(markAt(e.target), e.clientX, e.clientY, e.clientX);
	const tapped = (e: PointerEvent) =>
		e.pointerType === 'touch' && !pressTraveled() && show(markAt(e.target), e.clientX, e.clientY, e.clientX);
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
	node.addEventListener('pointerup', tapped);
	node.addEventListener('pointerleave', leave);
	// The wheel scrolls and pans: the label would stay where the mark was.
	node.addEventListener('wheel', blur, { passive: true });
	node.addEventListener('focusin', focus);
	node.addEventListener('focusout', blur);
	document.addEventListener('pointerdown', away);
	node.addEventListener('pointerdown', press);
	node.addEventListener('pointerdown', hold);
	// On the document, in capture: the lift may land anywhere on the page.
	for (const type of LIFTS) document.addEventListener(type, lift, true);
	// And the window losing focus: a button let go over another app never
	// lifts here. Not in capture, where every element's own blur would be one.
	window.addEventListener('blur', lift);
	node.addEventListener('contextmenu', chord);
	node.addEventListener('click', click);
	node.addEventListener('keydown', enter);
	const unsubscribe = pointed.subscribe((c) => {
		sel = c ?? null;
		drawCursor(node.querySelector('svg'), sel);
	});
	// Marked on the picture already drawn, not by redrawing it: a pill is
	// not a new command, and asking datum for the day again to outline one
	// of its fields would be a process per gesture.
	const unpilled = fieldPill.subscribe((field) => {
		pilled = field;
		highlight(node.querySelector('svg'), pilled);
	});
	// The selection, the same way and for the same reason — and from the
	// STORE, so an entry selected in the tail is brought out here without
	// the two surfaces knowing about each other.
	const unchosen = selected.subscribe((hids) => {
		chosen = hids;
		bringOut(node.querySelector('svg'), chosen);
	});
	return {
		update: (next: { text: string; unsynced?: number }) => {
			if (next.unsynced !== unsynced) drawUnsynced(node.querySelector('svg'), (unsynced = next.unsynced));
			if (next.text === text) return;
			text = next.text;
			if (pressed) late = text;
			else draw(text);
		},
		destroy: () => {
			document.removeEventListener('pointerdown', away);
			for (const type of LIFTS) document.removeEventListener(type, lift, true);
			window.removeEventListener('blur', lift);
			clearTimeout(lifting);
			unsubscribe();
			unpilled();
			unchosen();
		}
	};
}
