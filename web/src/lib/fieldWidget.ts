// The setup widget's door (da-fu29p): which field it is on, and opening it.
//
// vividn, 2026-10-08: «let's fully combine track/spec widgets into just 1 …
// make it so that you can switch the field you're editing inside of it more
// easy and intuitive». So there is ONE widget (`setup`,
// $lib/layout/FieldWidget.svelte) and one answer to «which field?»:
// `editing`, a store every copy of the widget draws rather than a setting
// of each — so two copies on one page cannot be on two fields: a chip
// pressed in one moves the other. Its chips write it, a field pill minted beside it writes it, and
// `openField` writes it and brings the widget into view — which is what the
// fields widget's `+`, the commands widget's `track` and `setup`, and the
// CLI's «which verb?» refusal ($lib/layout/engine.ts) all call. The empty
// name is a NEW field, with its kind chosen when the door knew one.
//
// There is no modal that asks (da-94q1's was the last): a field is made and
// edited in a PLACE (da-hbrfg), and the widget runs its own save.
//
// What the four kinds ARE is `./track.js`, kept separate so node can load
// it and hold the glyphs to the CLI's render.

import { derived, get, writable } from 'svelte/store';
import { layout } from './layout/current.js';
import { isPreview, modalPlace, openPlace } from './layout/shell.js';
import type { RunResult } from './run.js';
import type { Verb } from './track.js';

/** The widget's kind, and the name of its place. */
const FIELD = 'setup';

/**
 * The field the widget is on — `''` is a new one — and the kind a new one
 * opens with. Every set is a new object, which is what draws the form
 * afresh: the (+) pressed twice is an empty form twice.
 */
export const editing = writable<{ field: string; verb?: Verb | null }>({ field: '' });

/** Who waits on the place: answered once, by a save or by its leaving. */
let waiting: ((result: RunResult | null) => void) | null = null;

/**
 * The field a `setup` line opens the widget on (da-ez40f): `''`, a new one,
 * for `setup` alone, F for `setup F`, and null for any other line — a
 * change or a `--delete` runs and opens nothing.
 */
export const specOpens = (argv: string[]): string | null =>
	argv[0] !== 'setup' || argv.length > 2 || argv[1]?.startsWith('-') ? null : (argv[1] ?? '');

/** The widget ran `result`, or was left running nothing: what `openField` resolves with. */
export function answerField(result: RunResult | null): void {
	waiting?.(result);
	waiting = null;
}

/**
 * Put the setup widget on `field` — none is a new one, `verb` its kind —
 * and bring it into view: a pane showing it already, else its place
 * (`openPlace`). Resolves with what the place's next save ran — a refused
 * one leaves it open, to be put right — or null when it is left without
 * one. A widget someone keeps, in a pane or a tab of its own, answers in
 * its own time: null at once.
 */
export async function openField(field = '', verb: Verb | null = null): Promise<RunResult | null> {
	answerField(null);
	editing.set({ field, verb });
	// A pane's tabs behind the front one stay mounted, hidden (TabBody.svelte): not in view.
	const hosts = [...document.querySelectorAll(`.host[data-widget="${FIELD}"]`)];
	if (hosts.some((host) => !host.closest('.tab-body.behind'))) return null;
	await openPlace(FIELD);
	if (get(modalPlace)?.name !== FIELD && !isPreview(FIELD)) return null;
	let answer!: (result: RunResult | null) => void;
	const left = new Promise<RunResult | null>((resolve) => (answer = resolve));
	waiting = answer;
	// Left is the place gone from both hosts, the desktop's modal and a
	// root tab (da-jozgv) — not a widget unmounting, which the copy a tab
	// slides in as does when it lands (da-6du3o). `Open as tab` is a
	// leaving too: the modal closes before the tab is built. Only this
	// open's own waiter, since a later open answers this one itself.
	const open = derived([layout, modalPlace], ([l, m]) => m?.name === FIELD || l.root.tabs.some((t) => t.layout === FIELD));
	const stop = open.subscribe((here) => {
		if (!here && waiting === answer) answerField(null);
	});
	return left.finally(stop);
}
