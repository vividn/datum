// A widget's options, as items of the menu its NAME opens (da-vhwq).
//
// vividn, 2026-09-21: «Clicking on the widget name instead opens a menu to
// select session-level widget options, most of which can be changed and
// manipulated directly in the menu/submenus without having to open a new
// modal», and «when unlocked, maybe we have an extra option in this menu,
// something like "save options with layout"».
//
// So the modal `Options…` opened (da-gvdc) is gone and its rows are items:
// a switch is one item that carries its own state, a choice opens a
// submenu of its values. Neither needs room — the five options declared
// today are four switches and two named choices, the widest of them
// nowview's thirteen windows, which is a list and not a control. The door
// back to a modal is one `run` on an item, for the first option that
// genuinely needs one.
//
// The options themselves are still declared in ONE place, the registry
// (`optionsOfKind`): nothing about a particular kind is written here.
// Nothing is copied, either — every item reads the widget as the live tree
// and the live session hold it now, so a setting the widget changes itself
// (nowview's −) is what the menu says the moment it is opened.
//
// Every change is the SESSION's (`session.ts`). `saveOptions` is the one
// writer to the tree, and the lock hides it.

import { apply } from './current.js';
import type { Item } from './menu.js';
import { KINDS, optionsOfKind } from './registry.js';
import { clearSession, setSession, withSession } from './session.js';
import { findTab, setWidget } from './tree.js';
import type { Tab, WidgetOption } from './types.js';
import { isWidget } from './types.js';

/** What «Save options with layout» is called, in one place: the menu and its tests. */
export const SAVE_OPTIONS = 'Save options with layout';

type Settings = Record<string, unknown>;

/**
 * The option items for the tab's widget, and — unless the layout is locked
 * — the act that writes what this session holds into it.
 *
 * Empty for a tab that holds no one widget (a split, and so a layout of
 * more than one: its panes carry their own), for a kind that declares no
 * options, and for a kind that does when nothing is held and the layout is
 * locked. The SAVE is not gated on the declarations: a widget that moves a
 * setting of its own without declaring it an option — the day strip's
 * height, the stats widget's range chips — still has something to save.
 */
export function widgetOptions(tab: Tab | undefined, held: Settings, key: string, locked: boolean): Item[] {
	if (!tab || !isWidget(tab.content)) return [];
	const widget = tab.content;
	const defaults = KINDS[widget.kind]?.meta.defaultSettings ?? {};
	const shown = withSession(widget.settings, held);
	const valueOf = (option: WidgetOption) => shown[option.key] ?? defaults[option.key];
	const set = (option: WidgetOption, value: unknown) =>
		setSession(key, widget.settings, { ...shown, [option.key]: value });

	const rows = (optionsOfKind(widget.kind) ?? []).map((option): Item =>
		option.type === 'boolean'
			? {
					label: option.label,
					toggled: valueOf(option) === true,
					run: () => set(option, valueOf(option) !== true)
				}
			: {
					label: option.label,
					list: option.choices.map((choice) => ({
						label: choice.label + (choice.value === defaults[option.key] ? ' · default' : ''),
						title: `${option.label}: ${choice.label}`,
						checked: valueOf(option) === choice.value,
						run: () => set(option, choice.value)
					}))
				}
	);

	const savable = !locked && Object.keys(held).length > 0;
	return [
		...rows,
		...(savable
			? [
					{
						label: SAVE_OPTIONS,
						run: () => {
							// The tab is read FRESH inside the change, not from the
							// `widget` this menu was built over (the panel on #64):
							// the menu can be open while the tree moves under it — a
							// Restore in another root tab, a concurrent edit — and
							// merging `held` over a stale snapshot would put every
							// OTHER key back as it was. «Copies nothing» has to hold
							// at the moment of the act, not at the moment of the menu.
							apply((l) => {
								const now = findTab(l, tab.id);
								if (!now || !isWidget(now.content)) return l;
								return setWidget(l, tab.id, {
									...now.content,
									settings: { ...now.content.settings, ...held }
								});
							});
							clearSession(key);
						}
					}
				]
			: [])
	];
}
