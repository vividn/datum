// One data layer by instant, shared by the day strip and the nowview (da-m6i2a).
//
// vividn, 2026-10-27: «we could still do cache by day, just batch the data
// layer write when scrolling up, so generate 2 weeks at a time of data and cut
// it into days after generation». `dayview --data-layer` draws a day's bands
// alone, by instant, at a second a unit and `LAYER_HEIGHT` across, so either
// view scales the one picture to its own box: the strip a row (`bandOf`
// below), the nowview a crop under its overlay (#lib/nowcrop.ts). It is held
// here, outside both widgets, so a remount or a swipe keeps it, by database,
// orientation and date: the days asked in one microtask go in one run
// (`--pictures`), and a day held from before a write is checked by one
// `--digests` run and asked again only if its picture changed.

import { currentDb } from './database.js';
import { keepDay, keptDays, keptNow } from './heldDays.js';
import { LAYER_HEIGHT, layersArgv } from './panes.js';
import { run } from './run.js';
import { sanitizeSvg } from './svgSafe.js';

export { LAYER_HEIGHT };

/** The days the store holds of a database, the least recently asked dropped. */
const HOLDS = 120;

/** A day's data layer: its SVG, the run's stderr where it failed, and the SVG parsed and sanitized once. */
export type Layer = { text: string; error?: string; doc: () => Document | null };

/** A day held: its epoch, its answer, its text once answered, and the text it replaced, which a check compares with. */
type Entry = { epoch: number; run: Promise<Layer>; text?: string; base?: string };
/** A day claimed by an ask, and how its answer is given. */
type Claim = { date: string; entry: Entry; resolve: (layer: Layer) => void };

const stores = new Map<string, Map<string, Entry>>();
const claims = new Map<string, Claim[]>();

/** The local ISO date `back` days before today, as the CLI writes one. */
export function dateBack(back: number, from = new Date()): string {
	const at = new Date(from.getFullYear(), from.getMonth(), from.getDate() - back);
	return [at.getFullYear(), at.getMonth() + 1, at.getDate()].map((n) => String(n).padStart(2, '0')).join('-');
}

/**
 * The local days from the one instant `t` falls in back to `from`'s, as
 * `dateBack` counts them: a day a clock change is in is 23 or 25 hours, and
 * still one day (da-31hhd).
 */
export function daysBack(t: number, from = new Date()): number {
	return Math.round((new Date(from).setHours(0, 0, 0, 0) - new Date(t).setHours(0, 0, 0, 0)) / 86_400_000);
}

/**
 * The local dates a drawing from instant `from` to `to` reaches, today's
 * first, then each from `to`'s day back to `from`'s, counted in local days
 * (`daysBack`) and none ahead of today (da-vnfr3): a day the drawing starts
 * in even a millisecond before its midnight is one of them, a clock change
 * between or not.
 */
export function spanDates(from: number, to: number, now = new Date()): string[] {
	const back = (t: number) => Math.max(0, daysBack(t, now) || 0);
	const [near, far] = [Number.isFinite(to) ? back(to) : 0, back(from)];
	return [...new Set([0, ...Array.from({ length: far - near + 1 }, (_, i) => near + i)])].map((b) => dateBack(b, now));
}

/** Whole days from ISO date `from` back to ISO date `to`. */
const daysBetween = (from: string, to: string) => Math.round((Date.parse(from) - Date.parse(to)) / 86_400_000);

function layerOf(text: string, error?: string): Layer {
	let doc: Document | null | undefined;
	return {
		text,
		error,
		doc: () => {
			if (doc === undefined) {
				const parsed = text ? new DOMParser().parseFromString(text, 'image/svg+xml') : null;
				doc = parsed?.documentElement.localName === 'svg' && !parsed.querySelector('parsererror') ? parsed : null;
				if (doc) sanitizeSvg(doc.documentElement);
			}
			return doc;
		}
	};
}

const FAILED = 'the data of this day could not be drawn';

/**
 * The data layer of each of `dates` (ISO, local) as of `epoch`, in their
 * order. A day held at `epoch` or later is answered from here. Any other is
 * claimed at once, so a widget asking for it meanwhile shares the answer, and
 * the days claimed in this microtask, by any widget, are answered together.
 */
export function layers(dates: string[], epoch: number, vertical = false, db = currentDb()): Promise<Layer>[] {
	const at = `${db}\n${vertical ? 'v' : 'h'}`;
	const held = stores.get(at) ?? new Map<string, Entry>();
	stores.set(at, held);
	return dates.map((date) => {
		const prior = held.get(date);
		held.delete(date);
		if (prior && prior.epoch >= epoch) return held.set(date, prior).get(date)!.run;
		let resolve: (layer: Layer) => void = () => {};
		// The text it replaces is known now, or is that entry's own: never
		// awaited, so a newer ask's run can drop an older one still waiting.
		const entry: Entry = { epoch, run: new Promise<Layer>((r) => (resolve = r)), base: prior?.text ?? prior?.base };
		held.set(date, entry);
		while (held.size > HOLDS) held.delete(held.keys().next().value!);
		if (!claims.has(at)) {
			claims.set(at, []);
			queueMicrotask(() => void flush(at, db, vertical));
		}
		claims.get(at)!.push({ date, entry, resolve });
		return entry.run;
	});
}

/**
 * The layer held of `date`, of whatever epoch, or kept past a reload, of
 * none (-1), without asking: the day strip draws a day off screen from it,
 * and checks it once the day is on screen, as it did its own pictures.
 */
export function heldLayer(date: string, vertical = false, db = currentDb()): { epoch: number; run: Promise<Layer> } | undefined {
	const held = stores.get(`${db}\n${vertical ? 'v' : 'h'}`) ?? new Map<string, Entry>();
	stores.set(`${db}\n${vertical ? 'v' : 'h'}`, held);
	const text = held.has(date) ? undefined : keptNow(db)?.get(vertical ? 'layer-v' : 'layer')?.get(date);
	if (text) held.set(date, { epoch: -1, run: Promise.resolve(layerOf(text)), text });
	return held.get(date);
}

/** Answer the days claimed of `at` since the last flush. */
async function flush(at: string, db: string, vertical: boolean): Promise<void> {
	const claimed = claims.get(at) ?? [];
	claims.delete(at);
	const held = stores.get(at)!;
	const today = dateBack(0);
	const kind = vertical ? 'layer-v' : 'layer';
	// A day claimed twice is answered once; one kept past a reload stands in
	// for no epoch, checked as one held from before is. The store is read only
	// for a past day with nothing to check, so today's run is queued at once,
	// beside the overlay it is drawn under.
	const byDate = new Map<string, Claim[]>();
	for (const claim of claimed) byDate.set(claim.date, [...(byDate.get(claim.date) ?? []), claim]);
	const unchecked = [...byDate].some(([date, mine]) => date < today && !mine.some((c) => c.entry.base));
	const kept = (keptNow(db) ?? (unchecked ? await keptDays(db) : undefined))?.get(kind);
	const priorOf = (date: string): string | undefined => byDate.get(date)!.find((c) => c.entry.base)?.entry.base || kept?.get(date);
	// A run a newer flush's same run dropped as it waited: each day is that
	// flush's, whose answer is this one's.
	const superseded = (result: { superseded?: boolean }) => {
		if (!result.superseded) return false;
		for (const [date, mine] of byDate) {
			for (const { entry } of mine) if (held.get(date) === entry) held.delete(date);
			void layers([date], mine[0].entry.epoch, vertical, db)[0].then((layer) => mine.forEach((claim) => claim.resolve(layer)));
		}
		return true;
	};
	const key = (argv: string[]) => `layers\n${at}\n${argv.join(' ')}`;
	const answer = (date: string, layer: Layer) => {
		for (const claim of byDate.get(date)!) {
			if (layer.error && held.get(date) === claim.entry) held.delete(date);
			else claim.entry.text = layer.text;
			claim.resolve(layer);
		}
	};

	// A day held from before is checked by its digest; today has the now in it and goes again.
	const priors = new Map<string, string>();
	for (const date of byDate.keys()) {
		const text = date < today ? priorOf(date) : undefined;
		if (text) priors.set(date, text);
	}
	const missing = [...byDate.keys()].filter((date) => !priors.has(date));
	if (priors.size) {
		const older = [...priors.keys()];
		const argv = layersArgv(older, vertical, true);
		const result = await run(argv, undefined, undefined, key(argv), undefined, undefined, true);
		if (superseded(result)) return;
		let digests: Record<string, string> = {};
		try {
			digests = JSON.parse(result.stdout).digests ?? {};
		} catch {
			// No answer is no digest: each day is asked again.
		}
		const newest = older.sort().at(-1)!;
		for (const date of older) {
			const digest = digests[daysBetween(newest, date)];
			if (digest && digest === (await sha256(priors.get(date)!))) answer(date, layerOf(priors.get(date)!));
			else missing.push(date);
		}
	}
	if (!missing.length) return;
	const argv = layersArgv(missing, vertical);
	const result = await run(argv, undefined, undefined, key(argv), undefined, undefined, true);
	if (superseded(result)) return;
	let pictures: Record<string, string> = {};
	try {
		pictures = JSON.parse(result.stdout).pictures ?? {};
	} catch {
		// No answer is no picture, said as the day's error.
	}
	const newest = [...missing].sort().at(-1)!;
	for (const date of missing) {
		const text = pictures[daysBetween(newest, date)];
		if (text && date < today) keepDay(db, kind, date, text);
		answer(date, text ? layerOf(text) : layerOf('', result.stderr.trim() || FAILED));
	}
}

/** The SHA-256 of a picture as `--digests` takes it: the SVG as printed, less its newline. */
async function sha256(text: string): Promise<string | null> {
	if (!crypto.subtle) return null;
	const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text.replace(/\n$/, ''))));
	return Array.from(hash, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Has the day's layer one clock? A day the clock changed in names none (`--data-layer`): its row is the CLI's own picture. */
export const oneClock = (layer: Layer): boolean => !!layer.doc()?.querySelector('.day-data[data-offset]');

const NS = 'http://www.w3.org/2000/svg';

/**
 * The marks under `root`, a day's layer or a part of one, laid out in place
 * at `along` px a unit of time and `across` px a unit across: a block as long
 * as its time, every other mark the size `across` makes it, as `dayview`
 * draws one at that day height. In the attributes, so no mark carries a
 * transform of its own to keep its size: Chrome's per-frame compositing
 * update costs one per such mark, every frame anything on the page changes
 * (da-syaz8). `y`: time runs down (`--orientation vertical`).
 */
export function layMarks(root: Element, along: number, across: number, y = false): void {
	const [kx, ky] = y ? [across, along] : [along, across];
	const [at, length] = y ? ['y', 'height'] : ['x', 'width'];
	const num = (el: Element, name: string) => Number(el.getAttribute(name));
	const set = (el: Element, name: string, value: number) => el.setAttribute(name, String(value));
	const scale = (el: Element, name: string, by: number) => el.hasAttribute(name) && set(el, name, num(el, name) * by);
	const scaleAll = (el: Element, names: string[], by: number) => names.forEach((n) => scale(el, n, by));
	/** A mark centered at `at` along time, `length` long: its center moved, its length `by` times (one keeps it). */
	const center = (el: Element, by: number) => {
		const mid = (num(el, at) + num(el, length) / 2) * along;
		set(el, length, num(el, length) * by);
		set(el, at, mid - num(el, length) / 2);
	};
	/** A glyph's own stretch along x (`matrix(a 0 0 1 dx 0)`, dx its center's), about its new center. */
	const stretch = (el: Element, mid: number) => {
		const a = /^matrix\(([-\d.e]+) 0 0 1 /.exec(el.getAttribute('transform') ?? '')?.[1];
		if (a) el.setAttribute('transform', `matrix(${a} 0 0 1 ${mid * (1 - Number(a))} 0)`);
	};

	for (const el of root.querySelectorAll('rect, circle, text, use, line')) {
		if (el.matches('.bg') || el.closest('defs')) continue;
		if (el.localName === 'rect') {
			if (el.matches('.point-bg')) center(el, 1);
			else scaleAll(el, [at, length], along);
			scaleAll(el, y ? ['x', 'width'] : ['y', 'height'], across);
		} else if (el.localName === 'circle') {
			stretch(el, num(el, 'cx') * kx);
			scale(el, 'cx', kx);
			scale(el, 'cy', ky);
			scale(el, 'r', across);
		} else if (el.localName === 'use') {
			center(el, across);
			scaleAll(el, y ? ['x', 'width'] : ['y', 'height'], across);
			stretch(el, num(el, 'x') + num(el, 'width') / 2);
		} else if (el.localName === 'text' && el.matches('.error-label')) {
			// As datum sizes it: a twelfth of the day, between 7 and 11.
			const fs = Math.min(11, Math.max(7, LAYER_HEIGHT * across * 0.12));
			const line = el.parentElement!.querySelector('line')!;
			set(el, 'x', num(line, 'x1') * kx + 2);
			set(el, 'y', y ? num(line, 'y1') * ky - 2 : num(line, 'y1') * ky + fs);
			set(el, 'font-size', fs);
		} else if (el.localName === 'text') {
			stretch(el, num(el, 'x') * kx);
			scale(el, 'x', kx);
			scale(el, 'y', ky);
			scale(el, 'font-size', across);
		} else if (el.localName === 'line') {
			scaleAll(el, ['x1', 'x2'], kx);
			scaleAll(el, ['y1', 'y2'], ky);
		}
	}
}

/**
 * A day's layer as the day strip's row: `width` by `dayHeight`, as `dayview
 * --days 1` draws a band at that size — its blocks as long as their hours,
 * every other mark the size the day height gives it — and, with `now`, the
 * state still running cut there and the now-line drawn. A day of one clock
 * only: its 24 hours are the row's. A node of the page, sanitized as the
 * layer was, for `CliOutput` to draw as it is: serialized, a dense day was
 * parsed again at every draw, ~300 ms of main thread (da-mi2s6).
 */
export function bandOf(layer: Layer, width: number, dayHeight: number, now?: number): SVGSVGElement {
	const root = document.importNode(layer.doc()!.querySelector('svg')!, true);
	const span = root.querySelector('.day-data')!;
	const [from, to] = ['data-from', 'data-to'].map((name) => Date.parse(span.getAttribute(name) ?? ''));
	const num = (el: Element, name: string) => Number(el.getAttribute(name));
	const set = (el: Element, name: string, value: number) => el.setAttribute(name, String(value));
	layMarks(root, width / num(span, 'data-hi'), dayHeight / LAYER_HEIGHT);
	set(span, 'data-hi', width);
	for (const [n, v] of [['width', width], ['height', dayHeight]] as const) set(root, n, v);
	root.setAttribute('viewBox', `0 0 ${width} ${dayHeight}`);

	if (now !== undefined && now >= from && now < to) {
		const x = ((now - from) / (to - from)) * width;
		const at = new Date(now).toISOString();
		// What still runs is drawn on to the day's end in a layer, and stops at now in a row.
		for (const block of span.querySelectorAll('rect.block')) {
			if (Date.parse(block.getAttribute('data-to') ?? '') < to) continue;
			const w = Math.max(0, x - num(block, 'x'));
			if (w > 0) {
				set(block, 'width', w);
				block.setAttribute('data-to', at);
			} else block.remove();
		}
		const line = document.createElementNS(NS, 'line');
		for (const [n, v] of Object.entries({ class: 'current-time-line', x1: x, x2: x, y1: 0, y2: dayHeight, stroke: '#ffcc00', 'stroke-width': 2, 'stroke-dasharray': '4,2' }))
			line.setAttribute(n, String(v));
		root.querySelector('.plot')!.append(line);
	}
	return root;
}
