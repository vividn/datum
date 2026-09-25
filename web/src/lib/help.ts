// The Help button (da-vjwid): press it, then tap a command, a widget or a
// pill, and that thing is EXPLAINED instead of acted on.
//
// vividn, 2026-09-24: «you can click it and then another command to get help
// and understanding about what each command does. maybe you can then also
// click on a widget … or click on a pill … if someone hits the help button
// again maybe a general help?»
//
// ARMED is one store the button draws its label from. While it is set the
// page's next tap is caught at the WINDOW, in the capture phase, before any
// widget sees it — so nothing runs, fills, pends or starts a hold — and
// explained; then the page disarms (one-shot). Escape disarms too.
//
// What it says opens in a modal, $lib/HelpSheet.svelte (da-35f23) — not
// the bar's output section, which is where a newcomer, the one asking, is
// least likely to look (vividn, 2026-09-25: «the output is really quite
// buried and no one will see it»). It is not recorded in the REPL:
//
// - a command: the CLI's own `datum help <command>`, run through the seam.
//   No help copy for commands lives here.
// - Help itself: `datum help`, the general help.
// - a widget: its kind's `help` in the registry, where kinds are declared.
// - a pill: what its KIND is and does, with its own value in it (below).
//
// Anything else is general help too: the tap asked for help, so it gets
// some.

import { get, writable } from 'svelte/store';
import { pills, spent, type Pill } from './bar.js';
import HelpSheet from './HelpSheet.svelte';
import { KINDS, titleOf } from './layout/registry.js';
import { run } from './run.js';

export const helping = writable(false);

/** What each kind of pill is, and what it does to the next command, with its own text in it. */
const PILL_HELP: Record<string, (text: string) => string> = {
	time: (t) => `${t}: the time cursor. The next command is logged at this time instead of now.`,
	dur: (t) => `${t}: a span of time. The next start or switch covers it.`,
	hid: (t) => `${t}: the selected entries. The next command acts on them: get, delete, retime…`,
	field: (t) =>
		`${t}: the field in focus. The widgets narrow to it, and the next command names it.`
};

const PILL_COMMON =
	'Tap it to remove it. Locked, it stays for the next command, and comes back after a reload.';

/** The explanation of a pill, from its kind. */
export const pillHelp = (pill: Pill): string =>
	`${(PILL_HELP[pill.kind] ?? ((t: string) => `${t}: a pill, added to the next command.`))(pill.text)}\n${PILL_COMMON}`;

let sheet: HelpSheet | undefined;

/** Open the explanation, titled with what it explains. One sheet, mounted the first time and opened again after. */
function show(title: string, text: string): void {
	sheet ??= new HelpSheet({ target: document.body });
	sheet.$set({ title, text, open: true });
}

async function explain(target: Element | null): Promise<void> {
	const command = target?.closest<HTMLElement>('[data-command]')?.dataset.command;
	const pillId = target?.closest<HTMLElement>('[data-pill]')?.dataset.pill;
	const kind = target?.closest<HTMLElement>('[data-widget]')?.dataset.widget;
	const pill = pillId && [...get(pills), ...get(spent)].find((p) => p.id === pillId);
	if (pill) return show(pill.text, pillHelp(pill));
	if (!command && kind && KINDS[kind]) return show(titleOf(kind), KINDS[kind].help);
	const argv = command && command !== 'help' ? ['help', command] : ['help'];
	const { stdout, stderr } = await run(argv);
	show(`datum ${argv.join(' ')}`, stdout || stderr);
}

/** Every event a tap is made of — none of them reaches the page while armed. */
const SWALLOWED = ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'touchstart', 'touchend', 'dblclick', 'contextmenu'];

function swallow(event: Event): void {
	event.stopImmediatePropagation();
	// Not a touch's: that would cancel the click this is waiting for. A
	// mousedown's keeps the focus, and with it a phone's keyboard, where it is.
	if (event.type === 'mousedown' || event.type === 'contextmenu') event.preventDefault();
}

function onClick(event: MouseEvent): void {
	swallow(event);
	event.preventDefault();
	disarm();
	void explain(event.target instanceof Element ? event.target : null);
}

function onKey(event: KeyboardEvent): void {
	if (event.key !== 'Escape') return;
	event.stopImmediatePropagation();
	disarm();
}

/**
 * Arm the page. Called from the Help button's own click, which has already
 * passed the window on its way in — so the listener added here waits for
 * the NEXT one.
 */
export function arm(): void {
	if (get(helping)) return;
	helping.set(true);
	for (const type of SWALLOWED) addEventListener(type, swallow, true);
	addEventListener('click', onClick, true);
	addEventListener('keydown', onKey, true);
}

export function disarm(): void {
	helping.set(false);
	for (const type of SWALLOWED) removeEventListener(type, swallow, true);
	removeEventListener('click', onClick, true);
	removeEventListener('keydown', onKey, true);
}
