// A widget's demos (da-2fxiz). vividn, 2026-11-05: «Ideally there are even
// graphics and animations demonstrating various features. We should make sure
// that there is no drift in how these look compared to the app, so put some
// guards in place», and of a demo being the real widget, not a picture of it:
// «sounds good on your recommendation».
//
// So a demo keeps no image and no recording. It is the kind's own component,
// from `KINDS`, on a small sample database, played a short script of what a
// person does: a tap, a hold, a drag, a pinch, a scroll. A guide places one
// as a line of its own, `[what it shows](demo:<name>)` (#lib/guides.ts), which
// the docs and the Help place draw as a frame of the app's `/demo` page: a page
// of its own, so a tap there puts its pill on that page's bar and not on the
// reader's, and its database is held in memory (`initDb(true)`), so nothing it
// logs reaches a file. DemoPlayer.svelte plays it, and plays it again.
// `demos.test.mjs` plays every demo to its end.

import { HOLD_MS } from './hold.js';

export type Gesture = 'tap' | 'hold' | 'drag' | 'pinch' | 'scroll';

export interface DemoStep {
	/** What a person does, at the middle of `at`. */
	do: Gesture;
	/** Where: a selector, its first match drawn in the widget. */
	at: string;
	/** A drag's travel, a pinch's spreading (negative closes it), a scroll's, in px: [x, y]. */
	by?: [number, number];
}

export interface Demo {
	/** What it shows, in a few words: its frame's title. */
	title: string;
	/** The commands that make its sample database, run on an empty one before it plays; `-t` relative to now. */
	seed: string[];
	/** The widget's settings, where not its defaults. */
	settings?: Record<string, unknown>;
	/** Its frame, in CSS px. */
	width: number;
	height: number;
	script: DemoStep[];
}

/** Where `name` of `kind`'s demos plays: the `/demo` page, at `page`. */
export const demoSrc = (page: string, kind: string, name: string): string => `${page}?${new URLSearchParams({ db: 'demo', kind, demo: name })}`;

export const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** How long a step takes, by what it is: a tap is a press, a hold outlasts `HOLD_MS`. */
const LASTS: Record<Gesture, number> = { tap: 120, hold: HOLD_MS + 350, drag: 700, pinch: 700, scroll: 400 };
/** How many moves a moving finger makes on its way. */
const MOVES = 14;

/** How long a step waits for what it is done at to be drawn. */
const DRAWN_MS = 5000;

/** The first drawn match of `at` in `host`, once there is one, or an error naming it. */
async function find(host: Element, at: string): Promise<Element> {
	for (const until = performance.now() + DRAWN_MS; ; await sleep(50)) {
		const el = [...host.querySelectorAll(at)].find((e) => {
			const r = e.getBoundingClientRect();
			return r.width > 0 && r.height > 0;
		});
		if (el) return el;
		if (performance.now() > until) throw new Error(`a demo's step finds nothing drawn at ${at}`);
	}
}

type Point = { x: number; y: number };

/** Each finger's way, from and to, in client px. */
function ways(step: DemoStep, el: Element): [Point, Point][] {
	const r = el.getBoundingClientRect();
	const x = r.left + r.width / 2;
	const y = r.top + r.height / 2;
	const [dx, dy] = step.by ?? [0, 0];
	// A pinch's two fingers start 12px either side of the middle, close enough to land on the same element.
	if (step.do === 'pinch')
		return [
			[{ x: x - 12, y }, { x: x - 12 - dx / 2, y: y - dy / 2 }],
			[{ x: x + 12, y }, { x: x + 12 + dx / 2, y: y + dy / 2 }]
		];
	return [[{ x, y }, step.do === 'drag' ? { x: x + dx, y: y + dy } : { x, y }]];
}

/**
 * Do `step` in `host` as a person would: each finger down where it starts, along
 * its way, and up, the events at what is under it as it went down (a press keeps
 * its target, as a captured pointer does), a click after a still press; a scroll
 * a wheel there, and the scroller under it moved when nothing took the wheel: a
 * dispatched wheel has no default action, so nothing else scrolls it.
 * `fingers` is told where they are, and null once they lift.
 */
export async function perform(host: Element, step: DemoStep, fingers: (at: Point[] | null) => void): Promise<void> {
	const el = await find(host, step.at);
	const paths = ways(step, el);
	const target = (p: Point) => document.elementFromPoint(p.x, p.y) ?? el;
	if (step.do === 'scroll') {
		const [[at]] = paths;
		const [dx, dy] = step.by ?? [0, 120];
		fingers([at]);
		const on = target(at);
		const wheel = new WheelEvent('wheel', { bubbles: true, cancelable: true, composed: true, clientX: at.x, clientY: at.y, deltaX: dx, deltaY: dy });
		if (on.dispatchEvent(wheel)) scrollerOf(on)?.scrollBy({ left: dx, top: dy, behavior: 'smooth' });
		await sleep(LASTS.scroll);
		return fingers(null);
	}
	const touch = paths.length > 1;
	const targets = paths.map(([from]) => target(from));
	const fire = (type: string, i: number, p: Point) =>
		targets[i].dispatchEvent(
			new PointerEvent(type, {
				bubbles: true,
				cancelable: true,
				composed: true,
				clientX: p.x,
				clientY: p.y,
				// The mouse's own id, which a capture takes; a pinch's are a touch's two.
				pointerId: touch ? 2 + i : 1,
				pointerType: touch ? 'touch' : 'mouse',
				isPrimary: i === 0,
				button: type === 'pointermove' ? -1 : 0,
				buttons: type === 'pointerup' ? 0 : 1
			})
		);
	const along = (t: number) => paths.map(([a, b]) => ({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }));
	paths.forEach(([from], i) => fire('pointerdown', i, from));
	fingers(along(0));
	const still = step.do === 'tap' || step.do === 'hold';
	if (still) await sleep(LASTS[step.do]);
	else
		for (let n = 1; n <= MOVES; n++) {
			await sleep(LASTS[step.do] / MOVES);
			const at = along(n / MOVES);
			at.forEach((p, i) => fire('pointermove', i, p));
			fingers(at);
		}
	const end = along(1);
	end.forEach((p, i) => fire('pointerup', i, p));
	if (still && !touch) targets[0].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, composed: true, clientX: end[0].x, clientY: end[0].y, detail: 1 }));
	fingers(null);
}

/** The nearest element `el` is in that scrolls. */
function scrollerOf(el: Element): Element | null {
	for (let at: Element | null = el; at; at = at.parentElement) {
		const { overflowX, overflowY } = getComputedStyle(at);
		if ((/auto|scroll/.test(overflowY) && at.scrollHeight > at.clientHeight) || (/auto|scroll/.test(overflowX) && at.scrollWidth > at.clientWidth)) return at;
	}
	return null;
}
