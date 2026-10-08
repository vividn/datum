// The modal that asks «which verb?», as functions anyone can call.
//
// `SpecModal.svelte` is where a field is declared (da-94q1) and where its
// spec is edited (da-ci85o): one modal, because the two ask the same first
// question. Mounted imperatively, the way `confirm.ts` mounts Confirm and
// `chooser.ts` mounts Chooser: a modal belongs to the moment it is asked
// for, not to a place in the tree — which is what lets the fields widget's
// `Track +`, the CLI's own refusal ($lib/layout/engine.ts) and the
// console's `datum.track` all open the same one.
//
// Editing a spec is not a question but a PLACE (da-hbrfg): `openSpec`
// opens the field's `spec:<field>` tab, the same form in the `spec`
// widget ($lib/layout/SpecWidget.svelte), which runs its own save.
//
// What the four verbs ARE is `./track.js`, kept separate so node can load
// it and hold the glyphs to the CLI's render.

import SpecModal from './SpecModal.svelte';
import { specShowArgv } from './calls.js';
import { run, type RunResult } from './run.js';
import { specPlace } from './layout/defaults.js';
import { derived } from 'svelte/store';
import { layout } from './layout/current.js';
import { press } from './layout/engine.js';
import { modalPlace, openPlace } from './layout/shell.js';
import type { Verb } from './track.js';

/**
 * How the modal opens to declare a field: with a field in it, a verb
 * chosen, and the ignored fields to re-track.
 *
 * `ignored` is a snapshot, as Chooser's layout list is: the modal shows
 * what the bar behind it was showing when it was opened, rather than
 * growing a line under the pointer.
 */
export interface TrackOptions {
	field?: string;
	verb?: Verb | null;
	ignored?: string[];
}

function mount(props: TrackOptions): Promise<string[] | null> {
	return new Promise((resolve) => {
		const modal = new SpecModal({ target: document.body, props });
		modal.$on('close', ({ detail }) => {
			modal.$destroy();
			resolve(detail);
		});
	});
}

/** Open the modal on Basics to declare a field; resolves with the argv to run, or null if it was dismissed. */
export function openTrack({ field = '', verb = null, ignored = [] }: TrackOptions = {}): Promise<
	string[] | null
> {
	return mount({ field, verb, ignored });
}

/** Who waits on a field's place: answered once, by its Done or its leaving (`answerSpec`). */
const waiting = new Map<string, (result: RunResult | null) => void>();

/**
 * Open `field`'s spec: its place (`openPlace`), resolving with what its
 * Done ran — a refused save leaves the place open, to be put right — or
 * with null when it is left without one. A field with no spec yet has nothing to
 * edit and is declared instead, which is a question: resolves with the
 * `track` it ran, or null if it was dismissed.
 */
export async function openSpec(field: string): Promise<RunResult | null> {
	const shown = await run(specShowArgv(field));
	let spec = false;
	try {
		spec = shown.code === 0 && !!JSON.parse(shown.stdout);
	} catch {
		// Not a spec: declare it, as a field without one would be.
	}
	if (!spec) {
		const argv = await openTrack({ field });
		return argv && press(argv);
	}
	answerSpec(field, null);
	let answer!: (result: RunResult | null) => void;
	const left = new Promise<RunResult | null>((resolve) => (answer = resolve));
	waiting.set(field, answer);
	const name = specPlace(field);
	await openPlace(name);
	// Left is the place gone from both hosts, the desktop's modal and a
	// root tab (da-jozgv) — not a widget unmounting, which the copy a tab
	// slides in as does when it lands (da-6du3o). `Open as tab` is a
	// leaving too: the modal closes before the tab is built. Only this
	// open's own waiter, since a later open answers this one itself.
	const open = derived([layout, modalPlace], ([l, m]) => m?.name === name || l.root.tabs.some((t) => t.layout === name));
	const stop = open.subscribe((here) => {
		if (!here && waiting.get(field) === answer) answerSpec(field, null);
	});
	return left.finally(stop);
}

/** `field`'s place ran `result`, or was left running nothing: what `openSpec` resolves with. */
export function answerSpec(field: string, result: RunResult | null): void {
	waiting.get(field)?.(result);
	waiting.delete(field);
}
