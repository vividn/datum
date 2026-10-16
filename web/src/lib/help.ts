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
// What it says lands in the Help PLACE (da-z955f, layout/HelpWidget.svelte):
// a Help pane, if the page shows one, else wherever `openPlace` hosts the
// place — a tab beside this one on a phone, a modal on a desktop (da-jozgv)
// — and not a sheet of its own (it was one, da-35f23), nor the bar's
// output section, which is where a newcomer, the one asking, is least
// likely to look (vividn, 2026-09-25: «the output is really quite buried and
// no one will see it»). It is not recorded in the REPL:
//
// - a command: the CLI's own `datum help <command>`, run through the seam.
//   No help copy for commands lives here.
// - Help itself: `datum help`, the general help.
// - a widget: its kind's `help` in the registry, where kinds are declared —
//   or, for a kind with a GUIDE, the guide (below), which shows that help too.
// - a pill: what its KIND is and does, with its own value in it (below).
// - a flag of a held command (da-st2xl): its line of the CLI's own flag
//   table, carried on the button from `datum help --json`.
// - any other control carrying a `data-help` (da-ftj39): its `title`, and
//   that text — the top bar's layout menu, `+`, gear and ⓘ, and a widget's
//   own control, before its widget's help.
//
// Anything else is general help too: the tap asked for help, so it gets
// some.

import { get, writable } from 'svelte/store';
import { pills, spent, type Pill } from './bar.js';
import { KINDS, titleOf } from './layout/registry.js';
import { openPlace } from './layout/shell.js';
import { helpArgv } from './calls.js';
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

/**
/**
 * One page of `datum help <name> --json` (da-efruz): its header line, the
 * prose before its first heading, and its sections, the usage forms and
 * flag tables among them marked `reference`.
 */
export type HelpPage = {
	title: string;
	intro: string;
	sections: { heading: string; text: string; reference: boolean }[];
};

/**
 * The last explanation asked for, titled with what it explains: what the Help place shows.
 * `guide` names a page of `GUIDES` drawn in place of the text, which it is given.
 * A command's or a topic's is a `page`, which the place leads with its intro.
 * `help` is what `explainCommand` was given, the general help's `''`.
 */
export const explained = writable<Explanation | null>(null);
type Explanation = { title: string; text: string; guide?: string; page?: HelpPage; help?: string };

/** The Help place's width in characters, once it has drawn: a page is asked for at it (da-nrjze). */
let columns: number | null = null;

/**
 * The Help place's width in characters, as it draws and resizes: the help
 * it shows is asked for again at a new one, so it is the CLI that wraps it,
 * and replaces it there unless something else was explained meanwhile.
 */
export function fitHelp(width: number | null): void {
	if (!width || width === columns) return;
	columns = width;
	const help = get(explained)?.help;
	if (help === undefined) return;
	void helpOf(help || undefined).then((now) => {
		if (get(explained)?.help === help) explained.set(now);
	});
}

/**
 * The guides, by name, and their titles: a page in plain words of how to use a
 * widget, from the person's side, the CLI's own help folded at its end (da-oyvsz;
 * vividn, 2026-10-15: «a nice sync widget web help text should be made as well to
 * describe how to use it»). A name is the kind it is about, whose Help-button
 * tap opens it: it is drawn by layout/HelpWidget.svelte.
 */
export const GUIDES: Record<string, string> = { sync: 'Syncing your devices' };

/**
 * Land the explanation in the Help place: a Help pane in view, if there is
 * one, else the place `openPlace` opens. A pane's tabs behind the front one stay
 * mounted, hidden (TabBody.svelte), so a Help widget in one is not in view.
 */
function show(title: string, text: string, more: Omit<Explanation, 'title' | 'text'> = {}): void {
	explained.set({ title, text, ...more });
	const hosts = [...document.querySelectorAll('.host[data-widget="help"]')];
	if (!hosts.some((host) => !host.closest('.tab-body.behind'))) void openPlace('help');
}

async function explain(target: Element | null): Promise<void> {
	const command = target?.closest<HTMLElement>('[data-command]')?.dataset.command;
	const pillId = target?.closest<HTMLElement>('[data-pill]')?.dataset.pill;
	const kind = target?.closest<HTMLElement>('[data-widget]')?.dataset.widget;
	const pill = pillId && [...get(pills), ...get(spent)].find((p) => p.id === pillId);
	const flag = target?.closest<HTMLElement>('[data-flag]');
	const helped = target?.closest<HTMLElement>('[data-help]');
	if (pill) return show(pill.text, pillHelp(pill));
	if (flag) return show(`datum ${flag.dataset.flag}`, flag.dataset.help ?? '');
	// A control's own help before its widget's: the setup widget's tracked/ignored (da-iggnd).
	if (!command && helped) return show(helped.title, helped.dataset.help ?? '');
	if (!command && kind && GUIDES[kind]) return explainGuide(kind);
	if (!command && kind && KINDS[kind]) return show(titleOf(kind), KINDS[kind].help);
	return explainCommand(command);
}

/** A guide in the Help place, given its kind's help: the welcome's link, the Help button and the widget's own `?` open it. */
export function explainGuide(name: string): void {
	show(GUIDES[name], KINDS[name]?.help ?? '', { guide: name });
}

/**
 * `datum help <name>` in the Help place, as the Help button lands a command's: the
 * welcome's «Learn more» links take this path, so what they open is the CLI's own help.
 * A named page is asked for as JSON, to lead with its intro (da-efruz); the general
 * help, the list of commands, is its text. Either is asked for at the place's
 * width, `COLUMNS` as a terminal that wide sets it, once it has one (da-nrjze).
 */
export async function explainCommand(name?: string): Promise<void> {
	const { title, text, ...more } = await helpOf(name);
	show(title, text, more);
}

async function helpOf(name?: string): Promise<Explanation> {
	const argv = helpArgv(name);
	const paged = argv.length > 1;
	const env = columns ? [`COLUMNS=${columns}`] : [];
	const { stdout, stderr } = await run(paged ? [...argv, '--json'] : argv, undefined, undefined, undefined, undefined, env);
	const title = `datum ${argv.join(' ')}`;
	const help = name ?? '';
	if (paged && stdout) return { title, text: '', page: JSON.parse(stdout), help };
	return { title, text: stdout || stderr, help };
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
