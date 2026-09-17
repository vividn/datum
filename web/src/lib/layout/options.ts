// `Options…` (da-gvdc): the pane menu's door to a widget's options modal.
//
// Mounted imperatively, as `confirm.ts` mounts its dialogs: the modal
// belongs to the moment it is opened, not to a place in the tree.

import Options from './Options.svelte';
import { optionsOfKind } from './registry.js';
import type { Tab } from './types.js';
import { isWidget } from './types.js';

/**
 * The act that opens the options of the widget `tab` holds, or undefined —
 * and so no item — when the tab holds no widget of its own or its kind
 * declares no options. The rule `resetWidget` (shell.ts) reads, asked of
 * the options instead of the defaults.
 *
 * The question is the tab's SHAPE and not whether it names a layout
 * (da-ze1x): a saved layout of ONE widget is lifted into the tab that
 * holds it (`inline`, shell.ts), so the widget is that tab's own content
 * and this is the door to it. `datum.day/repl` and `/nowview` are exactly
 * that tab, and on a phone they are the whole page. A layout of more than
 * one widget arrives under a split instead — `isWidget` is false, there is
 * no one widget to mean, and the panes inside carry their own.
 */
export function widgetOptions(tab: Tab | undefined): (() => void) | undefined {
	if (!tab || !isWidget(tab.content) || !optionsOfKind(tab.content.kind)) return undefined;
	const tabId = tab.id;
	return () => {
		const modal = new Options({ target: document.body, props: { tabId } });
		modal.$on('close', () => modal.$destroy());
	};
}
