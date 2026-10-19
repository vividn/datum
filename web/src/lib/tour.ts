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

import { get, writable } from 'svelte/store';
import type { FieldButton } from './fields.js';
import { fields } from './layout/engine.js';
import { leavePlace, open } from './layout/shell.js';

export interface Stop {
	/** The element the stop points at: the first match that is drawn. */
	at: string;
	say: string;
}

/** The stops still to show, the one on screen first; empty when no tour is running. */
export const tour = writable<Stop[]>([]);

const KEY = 'datum.tour';

/**
 * What a tap on the button does, for each shape of field: said, not asked
 * for, since a tap in the tour only moves it on (da-q8koz).
 */
const TAP: Record<FieldButton['shape']['kind'], (f: string) => string> = {
	point: (f) => `A tap on ${f} logs it.`,
	state: (f) => `A tap on ${f} turns it on, and a second tap turns it off.`,
	states: (f) => `A tap on ${f} chooses what it is now.`,
	record: (f) => `A tap on ${f} starts a ${f} entry in the bar below.`
};

// The section and not its host: the host is `display: contents`, never drawn.
export const FIELDS = '[data-widget="widgetbar"] > section';
export const NOWVIEW = '[data-widget="nowview"] > section';
export const DAYVIEW = '[data-widget="daystrip"] > section';
export const buttonOf = (field: string) => `[data-widget="widgetbar"] button[data-field="${CSS.escape(field)}"]`;

/** The first element `at` names that is drawn, if any. */
export function shown(at: string): Element | null {
	for (const el of document.querySelectorAll(at)) {
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
 * `field` was just saved, the first: go to where the fields widget is —
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
	const shape = get(fields).find((f) => f.field === field)?.shape.kind ?? 'point';
	tour.set(
		[
			{ at: buttonOf(field), say: TAP[shape](field) },
			{ at: NOWVIEW, say: 'Your recent history appears here. Tap a time to enter something that happened then but you forgot to record.' },
			{ at: DAYVIEW, say: 'Longer history lives here. Scroll up to see earlier days. You can select times here too.' }
		].filter((s) => shown(s.at))
	);
}
