// The modal that asks «which verb?», as a function anyone can call.
//
// `TrackModal.svelte` is where a field is declared (da-94q1). Mounted
// imperatively, the way `confirm.ts` mounts Confirm and `chooser.ts`
// mounts Chooser: a modal belongs to the moment it is asked for, not to a
// place in the tree — which is what lets the fields widget's `Track +`,
// the CLI's own refusal ($lib/layout/engine.ts) and the console's
// `datum.track` all open the same one.
//
// What the four verbs ARE is `./track.js`, kept separate so node can load
// it and hold the glyphs to the CLI's render.

import TrackModal from './TrackModal.svelte';
import type { Verb } from './track.js';

/**
 * How the modal opens: with a field in it, a verb chosen, and the ignored
 * fields to re-track.
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

/** Open the Track modal; resolves with the argv to run, or null if it was dismissed. */
export function openTrack({ field = '', verb = null, ignored = [] }: TrackOptions = {}): Promise<
	string[] | null
> {
	return new Promise((resolve) => {
		const modal = new TrackModal({ target: document.body, props: { field, verb, ignored } });
		modal.$on('close', ({ detail }) => {
			modal.$destroy();
			resolve(detail);
		});
	});
}
