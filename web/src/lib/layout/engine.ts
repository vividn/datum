// What the page used to own and the panes hang off: the epoch, readiness,
// the widget bar's buttons, and the one path a widget-bar press takes.
//
// `epoch` is a single number. The root reads it once and passes it down
// the tree as a prop; no pane subscribes to changes on its own, so there
// is one refresh path and every pane is on it (the bead's trap list).
//
// `press` is the module-level dispatcher for the fields widget (da-nz29):
// the widget composes an argv and this decides where it runs. With a CLI widget on
// screen the press goes THROUGH it — `runArgv` echoes the executed argv
// into the scrollback and the recall history, which is the teach loop —
// and the CLI widget's own `ran` bumps the epoch. With none mounted (the
// reader closed it) the press runs through the seam directly and bumps
// the epoch itself, so the panes still refresh; the echo is simply
// nowhere to put, and comes back the moment a CLI widget is added.

import { tick } from 'svelte';
import { get, writable } from 'svelte/store';
import { clearUnlockedPills, commandsOpen, dropHids, focusLine, pills, showGhost } from '#lib/bar.ts';
import { openOutput } from '#lib/log.ts';
import { grammarOf, type Grammar, type GrammarEntry } from '#lib/commands.ts';
import { withPills } from '#lib/selection.ts';
import { shellJoin, shellSplit } from '#lib/shellSplit.ts';
import { initDb } from '#lib/db.ts';
import { asking, fileCommand } from '#lib/dbio.ts';
import { syncCommand } from '#lib/sync.ts';
import { parseEcho, runEchoed, type EchoDoc } from '#lib/echo.ts';
import { cli as history, record, stepped, wroteBy, type Command } from '#lib/cliStore.ts';
import { run, type RunResult } from '#lib/run.ts';
import { FROZEN_AT } from '#lib/dbProtocol.ts';
import { currentDb } from '#lib/database.ts';
import { conflictsArgv, entriesHistoryArgv, fieldsArgv, frecencyArgv, grammarArgv, nowArgv, stepArgv, type Reads } from '#lib/calls.ts';
import { listsConflicts, type Conflicted } from '#lib/conflicts.ts';
import { verbAsked } from '#lib/track.ts';
import { openField, specOpens } from '#lib/fieldWidget.ts';
import { openPlace, settledTree } from './shell.js';
import { FIELDS } from './draft.js';
import {
	buttonsFor,
	type FieldButton,
	type FieldListing,
	type NowActive,
	type NowListing,
	type Standings
} from '#lib/fields.ts';

/** Bumped after every command; the panes re-run theirs when it changes. */
export const epoch = writable(0);

/** True once `initDb()` has resolved: the CLI widget and the bar are usable. */
export const ready = writable(false);

/**
 * The bar as it stood when this database was last left (da-ch6oc): the
 * fields widget is drawn from it, live, before the CLI has answered — a
 * press then is taken, where a dimmed copy of the last visit waited out the
 * boot's queue — and the first refresh replaces it.
 */
function lastBar(): { fields: FieldButton[]; standings: Standings } {
	try {
		const kept = JSON.parse(localStorage.getItem(FIELDS) ?? 'null');
		if (kept?.db === currentDb()) return { fields: kept.fields, standings: new Map(kept.standings) };
	} catch {
		// Refused or unreadable: the bar fills when the CLI answers.
	}
	return { fields: [], standings: new Map() };
}

function keepBar(): void {
	try {
		localStorage.setItem(FIELDS, JSON.stringify({ db: currentDb(), fields: get(fields), standings: [...get(standings)] }));
	} catch {
		// Refused or full: the next load fills the bar when the CLI answers.
	}
}

if (typeof document !== 'undefined') {
	addEventListener('pagehide', keepBar);
	document.addEventListener('visibilitychange', () => document.hidden && keepBar());
}

const last = lastBar();

/** The fields widget's entries — `datum fields --json` and `now --json`, rebuilt on every beat. */
export const fields = writable<FieldButton[]>(last.fields);

/**
 * The entries with sync conflicts — `datum conflicts --json`, read on every
 * beat (da-6cutb): the commands widget's button counts them and the
 * conflicts widget lists them, so the two cannot disagree.
 */
export const conflicts = writable<Conflicted[]>([]);

/**
 * Where each field stood at the last sort (da-xtkcp) — which row of the
 * fields bar it is in and where along it. Written by `resort` alone.
 */
export const standings = writable<Standings>(last.standings);

/**
 * Sort the fields bar again: on load, and when the app comes back to the
 * foreground (vividn, 2026-09-24: a stable order, re-sorted at quiet
 * moments, not while in use). `fields --sort frecency` is asked here and
 * only here, so the refresh every command sets off never pays for the read
 * — and never moves a button.
 */
export async function resort(): Promise<void> {
	const sorted = await run(frecencyArgv());
	try {
		if (sorted.code === 0)
			standings.set(
				new Map(
					(JSON.parse(sorted.stdout) as FieldListing[]).map(({ field, y, frecency }) => [
						field,
						{ y, frecency: frecency ?? 0 }
					])
				)
			);
	} catch {
		// The bar keeps the order it had.
	}
}

export interface CliLike {
	runArgv(argv: string[], line?: string, stdin?: string, env?: string[]): Promise<RunResult>;
}

let cli: CliLike | null = null;

/** The CLI widget on screen announces itself; the newest one mounted wins. */
export function registerCli(instance: CliLike): () => void {
	cli = instance;
	return () => {
		if (cli === instance) cli = null;
	};
}

/**
 * The CLI's command table, `datum help --json` (da-r13j): which command
 * takes an id, which takes `-t`. Null until it is read, or if it could not
 * be — and a press then carries every pill but a selected hid, which no
 * command is known to take.
 */
export const grammar = writable<Grammar | null>(null);

/** Resolved once `boot` has read the grammar, or failed to. */
let booting = () => {};
const booted = new Promise<void>((resolve) => (booting = resolve));

export async function boot(): Promise<void> {
	// A worker that failed to load rejects this. The bar is enabled all the
	// same (da-89kom): a command then says what failed, and the next one
	// starts a fresh worker, where a rejection here left «opening database…»
	// up for good.
	await initDb().catch((e) => console.error('datum: the database did not open:', e));
	const help = await run(grammarArgv());
	try {
		if (help.code === 0) grammar.set(grammarOf(JSON.parse(help.stdout) as GrammarEntry[]));
	} catch {
		// No grammar: every command stays active.
	}
	ready.set(true);
	booting();
	// The line takes the typing once it can (da-wnjkf): after the flush that
	// enables it, once, and only where nothing has the focus yet and the
	// pointer is fine — on a phone a focused box is a keyboard over the page.
	setTimeout(() => {
		if (matchMedia('(pointer: fine)').matches && document.activeElement === document.body) focusLine();
	});
	await Promise.all([refresh(), resort()]);
	document.addEventListener('visibilitychange', () => {
		if (!document.hidden) void resort();
	});
}

/**
 * The widget's entries are built from `datum fields` — what has been logged
 * and what has been declared — and painted by `datum now`, what is on: so
 * a newly tracked field has to grow one and a started block has to fill,
 * hence a rebuild on the same beat the panes refresh on.
 */
export async function refresh(): Promise<void> {
	// Both asked at once (da-70g2u): `now` waiting for `fields` to answer
	// queued it behind every pane the same beat set off — 14 s on five years.
	const [listed, now, clashing] = await Promise.all([run(fieldsArgv()), run(nowArgv()), run(conflictsArgv())]);
	try {
		if (clashing.code === 0) conflicts.set(JSON.parse(clashing.stdout) as Conflicted[]);
	} catch {
		// The list stays as it was.
	}
	if (listed.code !== 0) return;
	// Guarded like the tail pane's own listing: stdout that is not the JSON
	// asked for (a partial write, a crashed process) keeps the bar it had
	// rather than throwing out of the one refresh path every pane is on.
	try {
		fields.set(buttonsFor(JSON.parse(listed.stdout) as FieldListing[], activeOf(now)));
	} catch {
		// The bar stays as it was.
	}
}

/** `now --json`'s active fields; none when it failed, so every field is drawn off rather than lost. */
function activeOf(now: RunResult): NowActive[] {
	try {
		return now.code === 0 ? ((JSON.parse(now.stdout) as NowListing).active ?? []) : [];
	} catch {
		return [];
	}
}

/**
 * A command ran, here or in another tab: the panes and the fields bar read
 * again when it CHANGED a database (da-fprk0). By effect — the worker's
 * change counters, `result.changed` — never by verb, so no list of which
 * verbs write can drift out of step with the CLI, and a failed write that
 * still moved a row counts. A read moves nothing: on the deployed layout
 * one cost nine pane runs, most of a keystroke. Without a result (another
 * tab's write, a seed) something is known to have changed.
 */
export async function ran(result?: RunResult): Promise<void> {
	if (result && !result.changed?.length) return;
	if (result) guess(result);
	// The bar's own reads go into the queue AHEAD of the panes' (da-70g2u):
	// the epoch is bumped after, so what it sets off queues behind them.
	const bar = refresh();
	epoch.update((n) => n + 1);
	await bar;
}

/**
 * The bar as the write's own echo says it now stands, at once (da-70g2u;
 * vividn: «the whole app must feel very snappy»): a CREATE in `true` or a
 * state turns its field's button on, in the state's colour; one in `false`
 * turns it off; anything else (an occur's `null` or quantity, a point or a
 * record field) changes nothing. A guess — a block with a duration, or a
 * backdated switch, may not be what `now` says — so `refresh` still runs
 * and what it reads wins.
 */
function guess(result: RunResult): void {
	const docs = (parseEcho(result.stdout) ?? []).filter((d) => d.action === 'CREATE');
	if (!docs.length) return;
	fields.update((buttons) =>
		buttons.map((b) => {
			const doc = docs.findLast((d) => d.field === b.field);
			if (!doc || b.shape.kind === 'point' || b.shape.kind === 'record') return b;
			const { state, state_color } = doc;
			if (state === false) return { ...b, active: false, current: undefined };
			if (state === true) return { ...b, active: true, current: true };
			if (typeof state !== 'string' || state === '') return b;
			return { ...b, active: true, current: state, stateColors: state_color ? { ...b.stateColors, [state]: state_color } : b.stateColors };
		})
	);
}

/**
 * A command from anywhere but a prompt of its own — a widget-bar press,
 * the bottom bar's line (da-0a5o), the browser console's `datum.run`
 * (da-6918) — which is echoed and recalled as typed. Through the CLI widget
 * when there is one; see above. Without one it is recorded here, so the
 * shared history and the bottom bar's receipt still see it, and the epoch
 * is bumped here.
 *
 * `stdin` is what the command reads on its standard input, for a caller
 * that has the bytes already (`db restore -` from the console). Supplying
 * it is also what says the browser's file dialog is not wanted: a file
 * command is only rewritten (#lib/dbio.ts) when the file is still to be
 * chosen.
 *
 * `typed` says the reader typed this at a prompt — only the bottom bar's
 * line is (`pressWithPills`). Everything else showed up without anyone
 * typing it, so the prompt says what ran, as a ghost that fades (da-jprg);
 * a typed line is not echoed back at the person who just typed it.
 *
 * `echo` is what to SAY ran, when that is not the whole argv: the bar's
 * pills go into the argv as words but are shown beside the ghost as pills
 * (da-xfa4), so saying them twice would be saying them twice.
 *
 * Pressed by a person — a tap, Enter, a Save, whose event is still being
 * dispatched — the command runs on a clock frozen at that event's stamp
 * (`gesture`), so what it logs is when they acted.
 */
export async function press(
	argv: string[],
	line?: string,
	stdin?: string,
	typed = false,
	echo?: string,
	env = gesture()
): Promise<RunResult> {
	// The argv the CALLER asked for, never the one the runner received: the
	// web adds flags of its own on the way through (`--width`), and those
	// are the web's, not a command anyone should be taught to type (da-xfa4).
	//
	// At the PRESS, not when the command lands (da-jrozq): the ghost is the
	// feedback that something was pressed, and it waited out the whole
	// round trip — 1.5s on the deployed app. Its second runs from here, so
	// a slow command's ghost is gone before its result is; vividn chose that:
	// «the output section shows the command that was run again, so it is
	// okay for it to fade before then».
	if (!typed) showGhost(echo ?? line ?? shellJoin(argv));
	// Typed before the tree is filled, the CLI widget it is for may not be
	// mounted yet: wait for the tree, and for that widget to announce itself.
	if (!cli) await settledTree().then(tick);
	const result = await (cli ? cli.runArgv(argv, line, stdin, env) : alone(argv, line, stdin, env));
	// The listing, typed, opens the widget they are resolved in (da-6cutb).
	if (result.code === 0 && listsConflicts(argv)) void openPlace('conflicts');
	// So do `setup` and `setup F` the field widget, on F (da-ez40f).
	const field = specOpens(argv);
	if (result.code === 0 && field !== null) void openField(field);
	return (await answerVerb(argv, result)) ?? result;
}

/**
 * The CLI's «which verb?» refusal is a question, and the web has somewhere
 * to ask it: `track <field>` on a field with neither a spec nor a log
 * comes back asking for `--verb`, and that is the setup widget's own
 * question (da-94q1, da-fu29p). Its place opens with the field already
 * named, and what it saves REPLACES the refused command — the answer is a
 * `track` with the verb on it, so there is nothing left to retry.
 *
 * Null whenever the result was not that refusal, or the place was left
 * without a save: the refusal then stands, as any other error does. It
 * stands too where a setup widget was in view already — that one is put on
 * the field and saves in its own time, which nothing here waits for.
 */
async function answerVerb(argv: string[], result: RunResult): Promise<RunResult | null> {
	const field = verbAsked(argv, result);
	return field === null ? null : openField(field);
}

/**
 * The keydown being handled, until the next task. Enter submits a form
 * after its keydown, and the `submit` is stamped then, past whatever kept a
 * busy page from handling the key: the person pressed it at the keydown.
 * A tap is its own: a busy page runs a waiting tap ahead of that timer, so
 * its `pointerdown` lets go of the key first, and a key pressed between a
 * tap's down and up, after its `pointerdown`, is not the tap's (da-j0l7r).
 * A click the key made (`detail` 0, Enter on a button) is the key's.
 */
let key: Event | undefined;
globalThis.addEventListener?.(
	'keydown',
	(e) => {
		key = e;
		setTimeout(() => (key = undefined));
	},
	true
);
globalThis.addEventListener?.('pointerdown', () => (key = undefined), true);

/**
 * The CLI's clock frozen at the gesture being dispatched, if one is
 * (da-szzgy): the person's tap or keypress, by the event's own stamp, so
 * neither a busy page nor the worker's queue makes the entry later. `[]`
 * from a console or a timer. On the page's `Date`, which the tests hold.
 *
 * The event is `window.event`, the DOM's current event: legacy, but in the
 * standard and in every engine, and set only while it is dispatched. So a
 * press freezes only when called in its handler, before any await; a caller
 * that awaits first runs on the wall clock, as before (da-szzgy).
 */
function gesture(): string[] {
	const event = (globalThis as { event?: Event }).event;
	if (!event?.isTrusted) return [];
	const age = Math.max(0, performance.now() - (event instanceof MouseEvent && event.detail > 0 ? event : (key ?? event)).timeStamp);
	return [`${FROZEN_AT}${Math.round(Date.now() - age)}`];
}

/** With no CLI widget mounted: run it, record it, and bump the epoch here. */
async function alone(argv: string[], line?: string, stdin?: string, env?: string[]): Promise<RunResult> {
	const ranAt = Date.now();
	const result = await ((stdin === undefined ? (fileCommand(argv) ?? syncCommand(argv)) : null) ?? runEchoed(asking(run), argv, stdin, env));
	record(line ?? shellJoin(argv), ranAt, result);
	await ran(result);
	return result;
}

/**
 * What this page does as it loads about a device in another zone than the
 * `timezone` field (da-mvt5i): the CLI's question, a dialog like any, or with
 * `timezone_follow = auto` no question, and the switch in the bar and the
 * log. With nothing to do it prints nothing, and nothing shows that it ran.
 *
 * It waits until the tree has mounted and its widgets have asked, past the
 * two frames `Stale.svelte` gives them: its answer empties the CLI's queue,
 * which takes a widget's copy from the last visit down, and on a tab of a CLI
 * widget, which asks nothing, it took it down before it was ever drawn. A tab
 * opened in the background waits for its first frame, so asks once it is seen.
 */
export async function followZone(): Promise<void> {
	await settledTree();
	await new Promise<void>((settled) => {
		let off = () => {};
		off = ready.subscribe((on) => {
			if (!on) return;
			settled();
			queueMicrotask(() => off());
		});
	});
	for (let frames = 3; frames > 0; frames--) await new Promise(requestAnimationFrame);
	const argv = ['follow-zone'];
	const ranAt = Date.now();
	const result = await runEchoed(asking(run), argv);
	if (result.code === 0 && !result.stdout.trim()) return;
	record(shellJoin(argv), ranAt, result);
	await ran(result);
}

/**
 * A command's undo or redo button (da-25q3k): every entry it wrote stepped
 * back, or forward again, by hid — `redo <hid>` still puts back an undo a
 * later write has followed. Recorded as OF that command, so the strip marks
 * it there instead of showing a command of its own; the log shows what the
 * CLI printed.
 *
 * Null, running nothing, when an entry's newest change is not this page's.
 */
export async function step(entry: Command, verb: 'undo' | 'redo'): Promise<RunResult | null> {
	const hids = wroteBy(entry);
	if (!(await ours(entry, hids))) return null;
	const argv = stepArgv(verb, hids);
	const ranAt = Date.now();
	const result = await runEchoed(run, argv);
	record(shellJoin(argv), ranAt, result, entry.ranAt);
	await ran(result);
	return result;
}

/**
 * Whether each entry's newest change is still the one this page made: the
 * command, or its button's last step (vremya on #1362). A second tab, the
 * terminal or a synced device may have changed one since, which no command
 * here buries and `undo <hid>` would step back instead. Read off the
 * trail's newest version by the words of the line that wrote it, which the
 * CLI quotes in its own way: two writes of the same words, here and elsewhere,
 * are not told apart. An entry gone (`history` exits 1) went elsewhere too.
 */
async function ours(entry: Command, hids: string[]): Promise<boolean> {
	const last = get(history).scrollback.findLast((c) => c.of === entry.ranAt && stepped(c)) ?? entry;
	const { code, stdout } = await run(entriesHistoryArgv(hids));
	if (code !== 0) return false;
	const trails: Reads['entriesHistoryArgv'] = JSON.parse(stdout);
	const newest = trails.map((t) => ('versions' in t ? t.versions[0] : t)).slice(0, hids.length > 1 ? undefined : 1);
	const words = JSON.stringify(shellSplit(last.line));
	return newest.every((v) => typeof v?.raw === 'string' && JSON.stringify(shellSplit(v.raw)) === words);
}

/**
 * A command from the bottom bar's line or the widgets, with the bar's pills
 * — the ones its grammar takes (`withPills` in #lib/selection.ts): a `-t
 * 14:30` cursor after the words, the selected hids right after the
 * command's name. The one path they all take (da-c7u8), so a pill lands on
 * a press exactly as on a typed line. A typed `line` is echoed as typed,
 * with the pills where the argv has them.
 *
 * A SUCCEEDING press clears the unlocked pills it spent, so the bar is
 * ready for the next line; a failure leaves them, so a retry keeps its
 * time. A pill the command did not take stays loaded. An entry the press
 * DELETED leaves the selection, locked or not, even when the run failed.
 *
 * The GHOST is the argv alone: the pills that went with it are drawn as
 * pills beside it and fade with it (da-xfa4, `spent` in #lib/bar.ts).
 */
export const pressWithPills = async (argv: string[], line?: string): Promise<RunResult> => {
	// A run closes the command widget over the output, which then shows it (da-xw655).
	if (get(commandsOpen)) {
		commandsOpen.set(false);
		openOutput();
	}
	// Pressed while datum boots (da-ch6oc), a press with pills waits for the
	// grammar that places them; on the clock it was pressed at, either way.
	const env = gesture();
	if (get(pills).length) await booted;
	const { argv: full, used } = withPills(get(grammar), argv, get(pills));
	const pressed = Date.now();
	const result = await press(
		full,
		line === undefined ? undefined : echoed(line, used),
		undefined,
		line !== undefined,
		line ?? shellJoin(argv),
		env
	);
	if (result.code === 0) {
		// A press nobody typed ghosted when it was pressed, and its pills
		// fade on that ghost's clock; a typed line has no ghost to keep time.
		clearUnlockedPills(used, line === undefined ? pressed : Date.now());
	}
	// Whatever the exit: a run that wrote and then failed still echoes its
	// writes (da-0uvyr), and `del <good>,<bad>` has deleted the good one.
	// An undone create is gone as a delete is (da-3mryj).
	const gone = (d: EchoDoc) => d.action === 'DELETE' || d.undid?.action === 'CREATE';
	dropHids((parseEcho(result.stdout) ?? []).filter(gone).map((d) => d.hid));
	return result;
};

/**
 * A typed line with its pills where `withPills` put them: the hids after the first word.
 * The line is as typed, quotes and all; a field pill is ONE token and is quoted, the
 * others are words as the bar holds them (`-t 14:30`), which quoting would join.
 */
function echoed(line: string, used: { kind: string; text: string }[]): string {
	const hid = used.find((p) => p.kind === 'hid');
	const [, word, rest] = /^\s*(\S*)(.*)$/.exec(line) ?? ['', line, ''];
	const head = hid ? `${word} ${hid.text}${rest}` : line;
	return [head, ...used.filter((p) => p !== hid).map((p) => (p.kind === 'field' ? shellJoin([p.text]) : p.text))].join(' ');
}
