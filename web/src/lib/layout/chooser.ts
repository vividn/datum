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
import { ancestorsOf, device, inlineInto, widgetInto } from './shell.js';
import { listLayouts, refreshKitLayouts } from './store.js';
import type { LayoutSource } from './store.js';
import type { NodeId } from './types.js';

/** What the modal came back with: a layout to inline, or a widget kind to fill with. */
export type Choice = { layout: string } | { kind: string };

/** One line of the modal's layout list: the name, and where it came from. */
export interface Offer {
	name: string;
	source: LayoutSource;
}

/**
 * The layouts the modal offers, in the one order: `main` and `collect`
 * (da-u7vz8: a first visit's tab, closeable like any, so it has to be
 * here to come back), then the rest — every other built-in IS a kind's
 * one-widget layout, and is listed once, among the widgets, or is `mobile`, `collect`'s tree on a phone
 * — less the names already open above the tab, since `main` may not hold
 * `main` (the shell refuses such names too; the modal does not offer
 * them).
 *
 * The rest, which is the person's saves and whatever a kit installed into
 * this database (da-gg0v), are ordered BY DEVICE: the ones designed for
 * this device first, then the ones designed for the other or for neither.
 * vividn, 2026-09-15 18:01: "load a different set of layouts by default when
 * the device is detected to be mobile, but still let them both load each
 * other's custom layouts". Both halves are there, in that order — nothing
 * is hidden from a device.
 *
 * The kit half is re-read here rather than trusted from boot, so a kit
 * installed at the prompt a moment ago is in the list.
 */
async function layoutsFor(ancestors: string[]): Promise<Offer[]> {
	await refreshKitLayouts();
	const listed = (await listLayouts().catch(() => [])).filter(
		(l) => !BUILTIN_NAMES.includes(l.name)
	);
	const here = device();
	const mine = listed.filter((l) => l.designedFor === here);
	const rest = listed.filter((l) => l.designedFor !== here);
	return [...['main', 'collect'].map((name) => ({ name, source: 'builtin' as const })), ...mine, ...rest]
		.map(({ name, source }) => ({ name, source }))
		.filter((o) => !ancestors.includes(o.name));
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
 *
 * At the root a widget is its built-in one-widget layout, which is why
 * `widgetInto` is a promise too (da-y2f6).
 */
export async function put(tabId: NodeId, picked: Choice): Promise<void> {
	if ('layout' in picked) await inlineInto(tabId, picked.layout);
	else await widgetInto(tabId, picked.kind);
}

/**
 * Fill a tab from the chooser: the empty pane's one button and the pane
 * menu's `Change Widget` (da-9vtw), which replaces what the tab holds. The
 * names open above the tab are the ones it may not hold; a dismissed
 * chooser changes nothing.
 */
export async function fillTab(tabId: NodeId): Promise<void> {
	const picked = await choose(ancestorsOf(tabId));
	if (picked) await put(tabId, picked);
}
