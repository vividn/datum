// The dialog that asks, as a function anyone can call.
//
// `Confirm.svelte` is the web's answer to a CLI prompt that wants a TTY
// the browser has not got (da-podv, da-axrk). Mounted imperatively, the
// way `chooser.ts` mounts `Chooser.svelte`: a modal belongs to the moment
// it is asked for, not to a place in the tree — which is what lets a
// rewritten command (`dbio.ts`: `db purge`, installing a kit) ask from
// wherever it was run.

import Confirm from './Confirm.svelte';

/** Whether the reader pressed `action` in a dialog saying `text` (and typed `name`, if one is given). */
export function confirm(text: string, action: string, name = ''): Promise<boolean> {
	return new Promise((resolve) => {
		const dialog = new Confirm({ target: document.body, props: { text, action, name } });
		dialog.$on('close', ({ detail }) => {
			dialog.$destroy();
			resolve(detail);
		});
	});
}
