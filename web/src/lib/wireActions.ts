// The actions and embeds in markdown drawn with `act` (#lib/markdown.ts,
// da-uzl16), made live: each action's button does it on a tap, the tap being
// the reader's yes, and wears the outline its kind's tap means (#lib/act.ts);
// each embed's placeholder gets its component. `use:wireActions={html}` on
// the element the HTML is drawn in, so a new HTML wires again.

import { mount, unmount } from 'svelte';
import { act } from './act.js';
import { TAP, perform, type Action } from './actions.js';
import FieldEmbed from './FieldEmbed.svelte';

export function wireActions(node: HTMLElement, _html: string) {
	let embeds: ReturnType<typeof mount>[] = [];
	const wire = () => {
		for (const embed of embeds) void unmount(embed);
		embeds = [];
		for (const button of node.querySelectorAll<HTMLButtonElement>('button[data-action]')) {
			const action = JSON.parse(button.dataset.action!) as Exclude<Action, { kind: 'field' }>;
			act(button, TAP[action.kind]);
			button.onclick = () => void perform(action);
		}
		for (const at of node.querySelectorAll<HTMLElement>('[data-embed="field"]'))
			embeds.push(mount(FieldEmbed, { target: at, props: { name: at.dataset.name! } }));
	};
	wire();
	return { update: wire, destroy: () => embeds.forEach((embed) => void unmount(embed)) };
}
