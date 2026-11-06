// The guides (da-oefd0): how to use datum.day, in plain words a newcomer
// reads. vividn, 2026-11-05: «the help dialogs are too geared towards the
// command line. They should be more about how to use the widgets and web
// site. Each widget should carry with it in the code how to use it, and
// document all of its features».
//
// A widget's guide is its registry entry's `guide` (layout/registry.ts); the
// page's, the Help button's general help, is here. The Help place draws one
// (layout/Guide.svelte), and the docs widget lists them all under its
// contents, as pages of their own.

import { builtinLayout } from './layout/defaults.js';
import { KINDS, titleOf } from './layout/registry.js';
import { markdown } from './markdown.js';

/** The page's guide, by the name a kind's goes by. */
export const PAGE = 'page';

export interface Guide {
	title: string;
	/** Markdown. */
	text: string;
	/** The `datum help` page it folds at its end. */
	commands?: string;
}

const PAGE_GUIDE = `datum.day is a page of widgets: each draws something of your data, or is a way to log it. You choose which widgets a page shows, and where; at the foot of every page is the bar, the command line.

## Tabs

The tabs along the top are your pages. Tap one to open it; on a phone, swipe sideways anywhere on the page to go to the next. With **Wrap tabs when swiping** in the gear's **Settings**, a swipe past the last tab goes to the first. Drag a tab sideways to move it (on a phone, hold it still first). A tab in italics was opened by a link and closes when you leave it; its menu's **Keep tab** keeps it. An orange dot on a tab says its layout changed and is not saved.

**+** at the end adds a tab: a widget on its own, a layout you saved, or **New layout**, which asks for a name and starts empty.

## The gear and Help

The gear holds **Backups**, **Sync** and **Settings**, each opening a place of its own (a modal on a desktop, a tab on a phone), then **Reset default layouts**, which puts datum's own tabs back as they shipped and leaves the layouts you saved alone, and the version.

**(?)** is Help. Press it, then tap anything: a widget, a command, a pill or a control is explained instead of used, here. Press it twice for this page. Escape lets it go.

On the right of the top bar, sync says what it is doing: «syncing», «sync paused» (tap to resume), and a ⚠ when a device joined or something needs you.

## Widgets and their menus

Each pane shows a widget, named in its title bar. Tap the name for its menu: **Reset widget** puts its options back as it started, then its options, a line each. An option changes what this tab shows on this device, and is kept across a reload; **Save options with layout**, there once you have changed one, writes them into the layout. The square before the name maximizes the pane to the whole page, and again restores it.

## Layout mode

To change a layout, hold its open tab and let go without moving (or right-click it, or press the menu key). The page turns hatched: every border is a handle to drag, each pane has **+** and **×** again, and the maximize square becomes a handle to move its bar to another edge. A pane's name now opens **Change Widget**, **Rename pane**, **Split** (an arrow each way), **Break out** (into a pane down a whole side), **Widget options ▸** and **Close pane**; holding a pane's name opens Change Widget too. The open tab's name opens **Change Widget**, **Split**, **Taller than the screen**, and the ways out:

- **Accept** keeps the change on this device, across reloads.
- **Save layout** saves it in the database, so every device you sync has it.
- **Discard changes** puts the page back as it was.

Escape, or going to another tab, asks which: Escape again discards, Enter accepts. Outside layout mode, the open tab's menu holds **Save layout**, **Restore layout** (the versions saved before), **Reset layout** for one datum ships, and **Save layout as**.

## The bar

The bar's lower row is the command line: the database's mark, the clock, **❯**, and the box you type in. Enter runs what you typed; ↑ and ↓ walk back through what you ran. **❯** slides the commands up over the bar, a round button each, and again puts them away.

Above the box is what the last command answered. Drag the bar up past it for the whole history, oldest first, and down to close it; the tab at its right edge opens and closes the answer, and PageUp and PageDown do the same from the box. **close history**, at the foot of the history, or a swipe down at its top, goes back to one row.

A widget you tap puts **pills** in the box: a time (the next entry is logged then, not now), a length, the entries you selected, or a field the widgets narrow to. Tap a pill to take it away; its lock keeps it for the next command, and after a reload. Escape takes away the pills, then the locked ones, then what you typed.

What you run goes through the same datum as a terminal's, in this browser, on data kept on this device.`;

/** The guide named `name`, the page's or a kind's. */
export const guideOf = (name: string): Guide | undefined =>
	name === PAGE
		? { title: 'Using datum.day', text: PAGE_GUIDE }
		: KINDS[name] && { title: titleOf(name), text: KINDS[name].guide, commands: KINDS[name].commands };

/**
 * The kinds whose guides the docs list: every kind any database offers, and a kit's widget, which
 * the chooser offers by its kit; not the notice a broken layout shows, which no one opens.
 * A function, read once the registry is: the registry imports the docs widget, which reads this.
 */
export const guided = (): string[] => Object.keys(KINDS).filter((kind) => !KINDS[kind].db && (!KINDS[kind].hidden || kind === 'kit'));

const OUT = { link: (href: string) => `href="${href.replace(/"/g, '&quot;')}" target="_blank" rel="noopener"`, image: (src: string) => src };
const PLACE = /\[([^\]]+)\]\(place:([\w-]+)\)/g;
const escape = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/**
 * A guide's Markdown as the Help place draws it (#lib/markdown.ts). A link to a place,
 * `[Open Sync](place:sync)`, is a `button.command[data-place]`, which Guide.svelte makes
 * open it (da-7s7xc); one to a name no place has is its text. The scheme is the page's
 * own, so it is held out before markdown, which keeps no link but http, https or mailto
 * (da-hse1u).
 */
export function guideHtml(text: string): string {
	const held: string[] = [];
	const button = (label: string, place: string) =>
		builtinLayout(place, false) ? `<button type="button" class="command" data-place="${place}">${escape(label)}</button>` : escape(label);
	const out = markdown(text.replace(PLACE, (_, label: string, place: string) => `\u0001${held.push(button(label, place)) - 1}\u0001`), OUT);
	return out.replace(/\u0001(\d+)\u0001/g, (_, i: string) => held[Number(i)]);
}
