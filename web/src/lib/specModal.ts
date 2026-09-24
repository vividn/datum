// The modal that asks «which verb?» — and, on a field that has a spec,
// everything else it is — as functions anyone can call.
//
// `SpecModal.svelte` is where a field is declared (da-94q1) and where its
// spec is edited (da-ci85o): one modal, because the two ask the same first
// question. Mounted imperatively, the way `confirm.ts` mounts Confirm and
// `chooser.ts` mounts Chooser: a modal belongs to the moment it is asked
// for, not to a place in the tree — which is what lets the fields widget's
// `Track +`, the CLI's own refusal ($lib/layout/engine.ts) and the
// console's `datum.track` and `datum.spec` all open the same one.
//
// What the four verbs ARE is `./track.js`, kept separate so node can load
// it and hold the glyphs to the CLI's render.

import SpecModal from './SpecModal.svelte';
import { run } from './run.js';
import type { Spec, Verb } from './track.js';

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

function mount(props: TrackOptions & { spec?: Spec | null }): Promise<string[] | null> {
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

/**
 * Open the modal on `field`'s spec, read through the seam; resolves with
 * the ONE `spec set` that saves it, or null if it was dismissed. A field
 * with no spec yet has nothing to edit, and is declared instead.
 */
export async function openSpec(field: string): Promise<string[] | null> {
	const shown = await run(['spec', 'show', field]);
	let spec: Spec | null = null;
	try {
		if (shown.code === 0) spec = JSON.parse(shown.stdout);
	} catch {
		// Not a spec: declare it, as a field without one would be.
	}
	return spec ? mount({ field, spec }) : openTrack({ field });
}
