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
 */
export function widgetOptions(tab: Tab | undefined): (() => void) | undefined {
	if (!tab || tab.layout !== undefined || !isWidget(tab.content) || !optionsOfKind(tab.content.kind)) return undefined;
	const tabId = tab.id;
	return () => {
		const modal = new Options({ target: document.body, props: { tabId } });
		modal.$on('close', () => modal.$destroy());
	};
}
