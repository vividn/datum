// The one chooser (da-uyx1), as a function anyone can call.
//
// `choose(ancestors)` opens `Chooser.svelte` on the page and resolves with
// what was picked — a saved layout's name or a widget kind — or null if
// the reader closed it and changed nothing. One implementation: the empty
// pane's button, the root bar's `+` and (the beads after this one) the
// pane and root menus all call this.
//
// Mounted imperatively, as `dbio.ts` mounts Confirm: a modal belongs to
// the moment it is asked for, not to a place in the tree. The saved
// layouts are read BEFORE it is mounted, so the list it shows is whole
// the instant it appears rather than growing a line under the pointer.

import Chooser from './Chooser.svelte';
import { BUILTIN_NAMES } from './defaults.js';
import { ancestorsOf, inlineInto, widgetInto } from './shell.js';
import { listLayouts } from './store.js';
import type { NodeId } from './types.js';

/** What the modal came back with: a layout to inline, or a widget kind to fill with. */
export type Choice = { layout: string } | { kind: string };

/**
 * The layouts the modal offers, in the one order: `day`, then the saved
 * layouts that do not shadow a built-in's name — every built-in but `day`
 * IS a kind's one-widget layout, and is listed once, among the widgets —
 * less the names already open above the tab, since `day` may not hold
 * `day` (the shell refuses such names too; the modal does not offer them).
 */
async function layoutsFor(ancestors: string[]): Promise<string[]> {
	const saved = (await listLayouts().catch(() => [])).map((l) => l.name);
	return ['day', ...saved.filter((n) => !BUILTIN_NAMES.includes(n))].filter(
		(n) => !ancestors.includes(n)
	);
}

/** Open the chooser; `ancestors` are the layout names it must not offer. */
export async function choose(ancestors: string[] = []): Promise<Choice | null> {
	const layouts = await layoutsFor(ancestors);
	return new Promise((resolve) => {
		const modal = new Chooser({ target: document.body, props: { layouts } });
		modal.$on('close', ({ detail }) => {
			modal.$destroy();
			resolve(detail);
		});
	});
}

/**
 * What a choice does to a tab: a layout inlined into it, or a widget put
 * in it — and a widget makes the tab THE WIDGET, shedding a layout it held
 * (da-8o07), which is why this is `widgetInto` rather than a bare
 * `setWidget`: `setWidget` is also how a widget's own settings are folded
 * back in (Layout.svelte), where the identity must survive.
 */
export async function put(tabId: NodeId, picked: Choice): Promise<void> {
	if ('layout' in picked) await inlineInto(tabId, picked.layout);
	else widgetInto(tabId, picked.kind);
}

/**
 * Fill a tab from the chooser: the empty pane's one button and the pane
 * menu's `Choose…` (da-9vtw), which replaces what the tab holds. The
 * names open above the tab are the ones it may not hold; a dismissed
 * chooser changes nothing.
 */
export async function fillTab(tabId: NodeId): Promise<void> {
	const picked = await choose(ancestorsOf(tabId));
	if (picked) await put(tabId, picked);
}
