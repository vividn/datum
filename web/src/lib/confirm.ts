// The dialog that asks, as a function anyone can call.
//
// `Confirm.svelte` is the web's answer to a CLI prompt that wants a TTY
// the browser has not got (da-podv, da-axrk). Mounted imperatively, the
// way `chooser.ts` mounts `Chooser.svelte`: a modal belongs to the moment
// it is asked for, not to a place in the tree — which is what lets a
// rewritten command (`dbio.ts`: `db purge`, installing a kit) ask from
// wherever it was run.

import { mount, unmount } from 'svelte';
import type { ActKind } from './act.js';
import Ask from './Ask.svelte';
import Confirm from './Confirm.svelte';

/**
 * Whether the reader pressed `action` in a dialog saying `text` (and typed
 * `name`, if one is given). `cancel` is what the other button reads.
 */
export function confirm(text: string, action: string, name = '', cancel = 'cancel'): Promise<boolean> {
	return new Promise((resolve) => {
		const dialog = mount(Confirm, {
			target: document.body,
			props: {
				text,
				action,
				name,
				cancel,
				onclose: (answer) => {
					unmount(dialog);
					resolve(answer);
				}
			}
		});
	});
}

/**
 * Which of `actions` the reader pressed in a dialog saying `text`, or null
 * for Escape or cancel: a CLI question with more answers than yes, as a
 * chained purge's leave, rejoin or chain (da-0hay9). Each answer opens the next
 * dialog there, so each wears `open`'s chevron; answers that act at once
 * (layout mode's Accept and Save, da-vvcxc) pass `run`. No answer is the
 * danger, so all are the accent, unless `dangerousCancel`: the cancel throws
 * work away and is red (layout mode's Discard changes).
 */
export function choose(
	text: string,
	actions: string[],
	cancel = 'cancel',
	kind: ActKind = 'open',
	dangerousCancel = false
): Promise<string | null> {
	const [action, ...also] = actions;
	return new Promise((resolve) => {
		const dialog = mount(Confirm, {
			target: document.body,
			props: {
				text,
				action,
				also,
				cancel,
				kind,
				danger: dangerousCancel ? 'cancel' : null,
				onchose: (pressed) => {
					unmount(dialog);
					resolve(pressed);
				}
			}
		});
	});
}

/**
 * What the reader typed in answer to `text`, trimmed — or null for Escape,
 * cancel, or nothing typed (`Ask.svelte`, da-y992). The app's own `prompt()`.
 * The field starts at `value`; `optional` takes an empty field as `''`.
 */
export function ask(text: string, action: string, value = '', optional = false): Promise<string | null> {
	return new Promise((resolve) => {
		const dialog = mount(Ask, {
			target: document.body,
			props: {
				text,
				action,
				value,
				optional,
				onclose: (answer) => {
					unmount(dialog);
					resolve(answer);
				}
			}
		});
	});
}
