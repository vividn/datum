// The first field's tour (da-ii8lo). vividn, 2026-10-15, asked where someone
// who has just saved their first field logs it: «after saving their first
// field, let's automatically change tabs to the collect tab and pulse the
// field button that was created. Then pulse the nowview and say how their
// recent history appears here … Finally pulse the dayview and with a
// tooltip to say this is where longer history is stored».
//
// So three STOPS, each an element on the page and a sentence: the new
// field's button, the nowview, the dayview. A stop whose element is not on
// the page when the tour begins is left out. It runs once per browser —
// `KEY` is written as the first stop is shown, so a reload mid-tour does
// not start it again — and only for a first field: a field saved with others already
// tracked (another device's, say) is not anyone's first.
//
// `Tour.svelte` draws `tour`'s first stop and moves on; this decides what
// the stops are and brings their page to the front.
//
// A WIDGET'S TOUR (da-7dflk). vividn, 2026-11-05: «every widget should have a
// tour, where stuff is highlighted and invites you to actually interact with
// it … if there are other widgets open on the page, it can go into how they
// interact. at the end of the tour it can offer a read more button». So each
// kind's registry entry carries its `tour`, steps on parts of the widget; a
// step that INVITES (`do`) lets that part be used through the dimmed page, and
// goes on once it has been; a step `with` another kind is shown only where
// that widget is drawn beside it. It ends on the widget whole, with «read
// more», its guide in the docs. Its menu's «? help», and Help then a tap on
// it, start it.

import { get, writable } from 'svelte/store';
import type { FieldButton } from './fields.js';
import { HOLD_MS } from './hold.js';
import { fields } from './layout/engine.js';
import { KINDS, WHOLE, titleOf, type Invite } from './layout/registry.js';
import { leavePlace, open } from './layout/shell.js';

export interface Stop {
	/** The element the stop points at: the first match that is drawn. */
	at: string;
	/** Where `at` is looked for, a widget's host: the page where absent. */
	in?: Element;
	say: string;
	/** What it invites the person to do there: the tour goes on once they have. */
	do?: Invite;
	/** The guide «read more» opens: the last stop of a widget's tour. */
	more?: string;
}

/** The stops still to show, the one on screen first; empty when no tour is running. */
export const tour = writable<Stop[]>([]);

const KEY = 'datum.tour';

/**
 * What a tap on the button does, for each kind of field: said, not asked
 * for, since a tap in the tour only moves it on (da-q8koz).
 */
const TAP: Record<FieldButton['shape']['kind'], (f: string) => string> = {
	moment: (f) => `A tap on ${f} opens it: set its time and data, then log it.`,
	'on/off': (f) => `A tap on ${f} opens it: set its time and data, then turn it on or off.`,
	change: (f) => `A tap on ${f} opens it: choose what it is now, its time and data.`,
	record: (f) => `A tap on ${f} starts a ${f} entry in the bar below.`,
	allday: (f) => `A tap on ${f} opens it: set its day and data, then log it.`
};

// The section and not its host: the host is `display: contents`, never drawn.
export const FIELDS = '[data-widget="widgetbar"] > section';
export const NOWVIEW = '[data-widget="nowview"] > section';
export const DAYVIEW = '[data-widget="daystrip"] > section';
export const buttonOf = (field: string) => `[data-widget="widgetbar"] button[data-field="${CSS.escape(field)}"]`;

/** The first element `at` names in `within` that is drawn, if any: a tab behind the front one is mounted, hidden (TabBody.svelte). */
export function shown(at: string, within: ParentNode = document): Element | null {
	for (const el of within.querySelectorAll(at)) {
		if (el.closest('.tab-body.behind')) continue;
		const r = el.getBoundingClientRect();
		if (r.width > 0 && r.height > 0) return el;
	}
	return null;
}

/** Wait (a few seconds at most) for the page to draw what `at` names. */
async function drawn(at: string): Promise<Element | null> {
	for (const end = performance.now() + 5000; performance.now() < end; ) {
		const el = shown(at);
		if (el) return el;
		await new Promise(requestAnimationFrame);
	}
	return null;
}

/** Whether this browser has had the tour; one that keeps nothing has not. */
function had(): boolean {
	try {
		return localStorage.getItem(KEY) !== null;
	} catch {
		return false;
	}
}

/**
 * `field` was just saved, the first: go to where the collect widget is —
 * the place left, and `collect` brought to the front unless a fields
 * widget is drawn already — and show the tour there.
 */
export async function begin(field: string): Promise<void> {
	if (had()) return;
	leavePlace();
	if (!shown(FIELDS)) await open('collect');
	if (!(await drawn(buttonOf(field)))) return;
	try {
		localStorage.setItem(KEY, 'seen');
	} catch {}
	const shape = get(fields).find((f) => f.field === field)?.shape.kind ?? 'moment';
	tour.set(
		[
			{ at: buttonOf(field), say: TAP[shape](field) },
			{ at: NOWVIEW, say: 'Your recent history appears here. Tap a time to enter something that happened then but you forgot to record.' },
			{ at: DAYVIEW, say: 'Longer history lives here. Scroll up to see earlier days. You can select times here too.' }
		].filter((s) => shown(s.at))
	);
}

/** The first drawn widget of `kind` but `not`, by its host (`display: contents`, so drawn by its section). */
const hostOf = (kind: string, not?: Element): Element | undefined =>
	[...document.querySelectorAll(`.host[data-widget="${CSS.escape(kind)}"]`)].find((host) => host !== not && shown(WHOLE, host));

/**
 * The stops of `kind`'s tour on the widget `host` (the first drawn where none
 * is given): each step whose element is drawn, a step `with` another kind
 * where that one is, and last the widget whole, with «read more».
 */
export function tourOf(kind: string, host = hostOf(kind)): Stop[] {
	if (!host || !KINDS[kind]) return [];
	const stops: Stop[] = KINDS[kind].tour.flatMap((step) => {
		const within = step.with ? hostOf(step.with, host) : host;
		return within && shown(step.at, within) ? [{ at: step.at, in: within, say: step.say, ...(step.do ? { do: step.do } : {}) }] : [];
	});
	return [...stops, { at: WHOLE, in: host, say: `That is the ${titleOf(kind)} widget. Read more for all it does.`, more: kind }];
}

/** Start `kind`'s tour on the widget `host`: its menu's «? help», or Help then a tap on it. */
export function tourWidget(kind: string, host?: Element | null): void {
	tour.set(tourOf(kind, host ?? undefined));
}

/** How far a press moves to be a drag, as a gesture here counts one. */
const MOVED = 8;

/**
 * Listen for what `stop` invites, done on its element, and call `done` once
 * the gesture's own handlers have run; returns the listeners' removal. The
 * listeners are the window's, in the capture phase, and take nothing from
 * the page: the gesture is the widget's to act on.
 */
export function invited(stop: Stop, done: () => void): () => void {
	const on: Record<string, (event: any) => void> = {};
	const el = () => shown(stop.at, stop.in);
	const inside = (event: Event) => {
		const at = el();
		const t = event.target;
		// A scroll's target is what scrolled, which may hold the element.
		return !!at && t instanceof Node && (at.contains(t) || (stop.do === 'scroll' && t !== document && t.contains(at)));
	};
	let timer: ReturnType<typeof setTimeout> | undefined;
	let from: { x: number; y: number } | null = null;
	const off = () => {
		clearTimeout(timer);
		for (const [type, fn] of Object.entries(on)) removeEventListener(type, fn, true);
	};
	const finish = () => {
		off();
		setTimeout(done);
	};
	const when = (event: Event) => inside(event) && finish();
	if (stop.do === 'tap') on.click = when;
	if (stop.do === 'type') on.input = when;
	if (stop.do === 'scroll') on.wheel = on.scroll = when;
	if (stop.do === 'hold') {
		on.contextmenu = when;
		on.pointerdown = (event) => inside(event) && (timer = setTimeout(finish, HOLD_MS));
		on.pointerup = on.pointercancel = () => clearTimeout(timer);
	}
	if (stop.do === 'drag') {
		on.pointerdown = (event: PointerEvent) => (from = inside(event) ? { x: event.clientX, y: event.clientY } : null);
		on.pointermove = (event: PointerEvent) => from && Math.hypot(event.clientX - from.x, event.clientY - from.y) >= MOVED && finish();
		on.pointerup = on.pointercancel = () => (from = null);
	}
	for (const [type, fn] of Object.entries(on)) addEventListener(type, fn, true);
	return off;
}
