// The page's ONE command line, as state (da-0a5o, da-3zon): the text
// being typed, the pills riding after it, the ghost over it, the submit
// that runs it, and the last command run anywhere on the page.
//
// Called `bar.ts` because the bottom bar was its only view. It is not any
// more: an `input` WIDGET is the same command line drawn in a pane
// (da-3zon), so the buffer lives here and both views bind to it — type in
// either and both show it, Enter in either runs it. Two local strings
// synced by effects is how two boxes drift by a keystroke; there is one
// string and it is `text`.
//
// Pills are what the bar is built to receive and nothing here sets: the
// shared time cursor vividn named next — a click on the dayview loading a
// `-t 14:30` pill, x-able — puts them here, and a line submitted from the
// bar, like a press in the fields widget, is what was typed followed by
// the pills' text (`pressWithPills` in layout/engine.ts: the CLI takes no
// flag before its verb). `setPills` and `removePill` are the whole of the
// contract. A LOCKED pill outlives a press, and (da-1py00) a reload too —
// see the section on `PILLS_KEY` below.
//
// `prefill` puts a line into the command line without running it — the
// fields widget's `add <field> ` (da-c7u8), or a command button's `occur `;
// the bar takes the signal and clears it, and `focusLine` decides which box
// the caret lands in and whether the phone's keyboard comes with it.
//
// `last` is every command run anywhere — a widget-bar press, a command
// pane's ▶, the bar itself — because every one
// of them goes through `record` in #lib/replStore.ts, which tells this
// store. It is what the bar's result area is drawn from (#lib/BarResult
// .svelte): the CLI's own documents, or its own lines when it echoed
// none. Not restored across a reload, unlike the scrollback: a result is
// about the command just run, and on a fresh page none has been.
//
// `ghost` is the narrower thing the prompt itself shows (da-jprg) — see
// below.

import { derived, get, writable } from 'svelte/store';
import { hidsOf, pendingVerb as verbOf, roleOf, type Verb } from './commands.js';
import { parseEcho } from './echo.js';
import { pressWithPills } from './layout/engine.js';
import type { RunResult } from './run.js';
import { hidPill, pickOf, picked, same } from './selection.js';
import { shellSplit } from './shellSplit.js';

/**
 * How long a ghost is on screen, and so how long a spent pill takes to
 * fade with it (da-xfa4). One constant: the fade is in Prompt.svelte's
 * CSS and the two timers that take the ghost and the pills away are both
 * below, and all three have to agree.
 */
export const GHOST_MS = 1000;

export interface Pill {
	id: string;
	/** What goes on the command line, verbatim (`-t 14:30`). */
	text: string;
	/** What kind of thing it is (`time`, `range`), for the bar to colour. */
	kind: string;
	/** A locked pill survives a successful press instead of clearing. */
	locked?: boolean;
	/**
	 * The colour this pill is painted, when its KIND's is not it
	 * (`#rrggbb`).
	 *
	 * One pill per field would otherwise need one CSS rule per field: a
	 * `field` pill is drawn in the FIELD's own colour (da-8810n, vividn:
	 * "let's color them with the field color"), which is
	 * `fields --json`'s resolved `color` and so is data, not a token.
	 * `Pill.svelte` sets `--pill` from it and derives the fill, the ink and
	 * the locked ring from that as it does for every other pill.
	 */
	color?: string;
	/**
	 * How the pill is DRAWN, when that is not its `text`: one pill divided
	 * into sections, each pressable on its own (da-ui4i — the time cursor's
	 * date and its time). Absent, the pill is one section saying `text`.
	 */
	sections?: PillSection[];
}

/**
 * A pressable part of a pill (da-ui4i).
 *
 * `text` is drawn, never sent: the pill's `text` remains the whole of what
 * the argv gets, so a section is a way of READING the pill, not a second
 * meaning for it. `rest` is what the pill becomes when this section is
 * pressed — without one the press takes the whole pill away, which is what
 * a pill with no sections at all has always done.
 */
export interface PillSection {
	/** What this section says (`-y`, `09:00`). */
	text: string;
	/** The pill left behind by a press here; absent → the pill goes. */
	rest?: { text: string; sections?: PillSection[] };
}

/** The sections a pill is drawn as — a pill without them is one section. */
export const sectionsOf = (pill: Pill): PillSection[] => pill.sections ?? [{ text: pill.text }];

// ---- A LOCKED PILL IS THIS DEVICE'S, AND COMES BACK (da-1py00) --------
//
// vividn, 2026-09-22: «this is a question if the pills should persist across
// reload essentially. Maybe if they are locked? Yeah, let's try that.»
//
// The lock already means «this one stays across presses» (`clearUnlockedPills`
// below). This is that same sentence one scope wider — it stays across
// RELOADS too — and an unlocked pill is still gone with the ghost.
//
// In `localStorage`, because a locked pill is what belongs to this device
// and this device only (§2, the rule `log.ts` and `tuning.ts` each say in
// their own words), and never an entry. NOT the layout: a widget option
// reaches the layout only on ask (da-vhwq), and nothing here asks, so a
// locked pill riding into a saved layout would be a second, quieter way
// for session state to become layout state.
//
// ⚑ It is THE PILLS that come back and not a filter beside them. Every
// reader — the stats widget, the tail table, the pictures — derives from
// this one store, so a restored pill narrows exactly as a freshly minted
// one does by construction, and there is no second path for it to be
// right-looking and wrong in.

/** Where the locked pills are kept (never synced: they are a screen's). */
export const PILLS_KEY = 'datum.bar.pills.v1';

/** What is written: the lock is implied — only locked pills are stored. */
type KeptPill = Pick<Pill, 'id' | 'kind' | 'text' | 'color'>;

/** What this device already holds, exactly as it was written; none, or unreadable, is null. */
function storedPills(): string | null {
	try {
		return localStorage.getItem(PILLS_KEY);
	} catch {
		// Blocked, or absent (the prerender).
		return null;
	}
}

/**
 * The locked pills `raw` holds — anything else it holds is dropped.
 *
 * `id`, `kind` and `text` are REQUIRED and `color` is not, because that is
 * the `Pill` contract above: the three are what a pill IS (an id to press,
 * a kind to colour it by, and the word that goes on the command line),
 * while a colour is what a `field` pill alone carries and every other kind
 * takes from its CSS. So a pill missing one of the three is not a pill and
 * goes; one missing a colour is the ordinary case.
 *
 * `sections` are never stored, because nothing that draws them keeps its
 * state in them: the `hid` pill's are redrawn from its text by the
 * subscriber below, and the cursor's pills are the CURSOR's — `#lib/cursor
 * .ts` reconciles them against it on this same load, which is what drops a
 * `time` pill whose cursor did not come back. So bar.ts needs no list of
 * which kinds may be restored; each owner still answers for its own.
 */
function keptPills(raw: string | null): Pill[] {
	if (!raw) return [];
	try {
		const held: unknown = JSON.parse(raw);
		if (!Array.isArray(held)) return [];
		return (held as KeptPill[]).flatMap((p) =>
			p && typeof p.id === 'string' && typeof p.kind === 'string' && typeof p.text === 'string'
				? [
						{
							id: p.id,
							kind: p.kind,
							text: p.text,
							locked: true,
							...(typeof p.color === 'string' ? { color: p.color } : {})
						}
					]
				: []
		);
	} catch {
		// Half-written: none, which is what every load had before this.
		return [];
	}
}

const stored = storedPills();

export const pills = writable<Pill[]>(keptPills(stored));

/**
 * Remember them, whenever the LOCKED ones change.
 *
 * Whenever they change and not on every change to `pills`: the time cursor
 * rewrites its pills on every press on a picture, and none of that is a
 * storage write. So the serialized locked set is compared with what was
 * last written and an identical one costs nothing — the press → feedback
 * chain is measured (da-a39xn), and this sits in it.
 *
 * `written` starts as what the device ALREADY HOLDS, so an ordinary load —
 * where what is stored is what this same code wrote last time — costs no
 * storage write at all, not even the subscriber's first call (canary
 * minimax-m3-preview on PR #86). What is stored and what would be written
 * differ only when storage holds something this build did not write: a
 * half-written or stale-shaped entry that `keptPills` dropped. Then they
 * are unequal, and the first call normalizes the device — which is the
 * behaviour that mattered, and it is kept. Nothing stored reads as `''`
 * rather than null, because an empty locked set is what `''` means below.
 */
let written: string = stored ?? '';

pills.subscribe((all) => {
	const locked: KeptPill[] = all
		.filter((p) => p.locked)
		.map(({ id, kind, text, color }) => ({ id, kind, text, ...(color ? { color } : {}) }));
	const json = locked.length ? JSON.stringify(locked) : '';
	if (json === written) return;
	written = json;
	try {
		if (json) localStorage.setItem(PILLS_KEY, json);
		else localStorage.removeItem(PILLS_KEY);
	} catch {
		// Unremembered is not unusable: they last the visit.
	}
});

export const setPills = (next: Pill[]): void => pills.set(next);

export const removePill = (id: string): void => pills.update((all) => all.filter((p) => p.id !== id));

/**
 * A press on one section of a pill: the pill becomes that section's `rest`,
 * or goes when it has none (da-ui4i). The producer of the pill says what a
 * section leaves behind; whoever derives the pill follows it from here (the
 * time cursor re-reads its instant off the text, #lib/cursor.ts).
 */
export const dropSection = (id: string, index: number): void =>
	pills.update((all) =>
		all.flatMap((p) => {
			if (p.id !== id) return [p];
			const rest = sectionsOf(p)[index]?.rest;
			return rest ? [{ ...p, text: rest.text, sections: rest.sections }] : [];
		})
	);

export const toggleLockPill = (id: string): void =>
	pills.update((all) => all.map((p) => (p.id === id ? { ...p, locked: !p.locked } : p)));

// ---- the FIELD pill (da-8810n) -----------------------------------------
//
// vividn, 2026-09-21: «Create a field pill for this field and let's filter
// widgets accordingly. So stats widget would filter to this field. tail
// widget too. command widget would filter to commands that take fields.»
//
// ONE field at a time, as the time cursor holds one instant: the id is
// fixed, so minting on a second field REPLACES the first rather than
// asking every reader what two of them would mean. Its `text` is the bare
// field name, the word a command's field positional takes — where
// that word lands in an argv, and which commands may have it at all, is
// `fits` and `withPills` (#lib/commands.ts, #lib/selection.ts).
//
// UNLOCKED, because vividn said it behaves like the others: a successful
// press spends it, so running a command drops the filter (`clearUnlockedPills`).

/** The one field pill's id. */
export const FIELD_ID = 'field';

/**
 * The field the bar is narrowed to, or `null`. Every widget that CAN
 * narrow reads this one store — the stats widget, the tail table, and the
 * pictures, which outline that field's own marks instead.
 */
export const fieldPill = derived(pills, (all) => all.find((p) => p.kind === 'field')?.text ?? null);

/**
 * Mint the field pill — `null` takes it away. `color` is the field's own
 * resolved colour, which is what the pill is painted in.
 */
export const setFieldPill = (field: string | null, color?: string): void =>
	pills.update((all) => [
		...all.filter((p) => p.kind !== 'field'),
		...(field ? [{ id: FIELD_ID, kind: 'field', text: field, ...(color ? { color } : {}) }] : [])
	]);

/**
 * The pills a press has just spent, on their way out (da-xfa4).
 *
 * A SEPARATE store, not a flag on `pills`: a spent pill is no longer in
 * the argv a press builds and no longer the time cursor's other face
 * (#lib/cursor.ts) — it is a receipt fading beside the ghost of the
 * command it went into. Keeping it out of `pills` is what lets both of
 * those readers stay exactly as they were.
 */
export const spent = writable<(Pill & { age: number })[]>([]);

/**
 * After a press SUCCEEDS: unlocked pills clear, locked ones stay for the
 * next line.
 *
 * The cleared ones move to `spent` for what is left of a ghost that
 * started at `since` — vividn: "The pills should be used with the ghost
 * text, so they can be part of it and fade with it" — and are gone with
 * it. The ghost starts at the press and the pills can only be spent once
 * the press has succeeded (da-jrozq), so they join it `age` ms into its
 * fade, and a command slower than the ghost leaves none to fade.
 */
export const clearUnlockedPills = (used: Pill[] = get(pills), since = Date.now()): void => {
	const doomed = used.filter((p) => !p.locked);
	if (!doomed.length) return;
	pills.update((all) => all.filter((p) => !doomed.includes(p)));
	const age = Date.now() - since;
	if (age >= GHOST_MS) return;
	const fading = doomed.map((p) => ({ ...p, age }));
	spent.set(fading);
	setTimeout(() => spent.update((all) => (all === fading ? [] : all)), GHOST_MS - age);
};

/**
 * Escape's turn at the input, once nothing else on the page took the key
 * (da-al892, PromptBar.svelte). vividn: «escape should then clear unlocked
 * pills -> clear locked pills too -> clear typed in text, in that
 * progression». Each press peels the first layer that has anything, and
 * a layer's arguments before its context: a field under an hid goes with
 * the NEXT press, once the hid has gone and it is an argument again.
 *
 * Not `clearUnlockedPills`: that one hands its pills to `spent` to fade
 * with the ghost of the command they went into, and an Escape ran no
 * command — so they go outright. The stored locks follow `pills` (the
 * subscriber above), so a cleared locked pill stays gone on a reload.
 */
export function peel(): void {
	const all = get(pills);
	for (const layer of [all.filter((p) => !p.locked), all]) {
		if (!layer.length) continue;
		// Within a layer, the stack from the top (da-tu30m): the arguments
		// go first, and a context pill under them is an argument again.
		const args = layer.filter((p) => roleOf(p, all) === 'argument');
		const doomed = args.length ? args : layer;
		return pills.set(all.filter((p) => !doomed.includes(p)));
	}
	text.set('');
}

/** The ×'s press: the whole input at once — the text and every pill, locked too. */
export function clearLine(): void {
	pills.set([]);
	text.set('');
}

/**
 * The selected entries' hids — the `hid` pill's, and none without one
 * (da-r13j, #lib/selection.ts). Every surface that draws an entry reads it,
 * so a selection is shown in tail and on the documents at once.
 */
export const selected = derived(pills, (all) => {
	const pill = all.find((p) => p.kind === 'hid');
	return pill ? hidsOf(pill) : [];
});

// A section dropped from the `hid` pill leaves the others as text alone
// (`hidPill` in #lib/selection.ts): draw them as sections again.
pills.subscribe((all) => {
	const pill = all.find((p) => p.kind === 'hid' && !p.sections);
	if (pill) setHids(hidsOf(pill), !!pill.locked);
});

/** The last entry tapped: where a shift-click's range starts. */
let anchor: string | null = null;

/** Put `hids` on the `hid` pill, where it was, keeping its lock; none takes it away. */
function setHids(hids: string[], locked: boolean): void {
	const next = hidPill(hids, locked);
	pills.update((all) => {
		if (!all.some((p) => p.kind === 'hid')) return next ? [...all, next] : all;
		return all.flatMap((p) => (p.kind !== 'hid' ? [p] : next ? [next] : []));
	});
}

/**
 * A tap on an entry — a tail row, a document, a mark in a picture.
 * `order` is the hids of the entries that surface draws, in its order,
 * for a shift-click's range.
 *
 * The MODIFIERS and not the event: a picture's inspect gesture takes
 * shift for itself and hands this the same press with shift off
 * (`inspectMods`, #lib/inspect.ts), and there is nothing else of an event
 * a selection ever read.
 */
export function select(
	hid: string,
	e: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean },
	order: string[]
): void {
	const pill = get(pills).find((p) => p.kind === 'hid');
	const locked = !!pill?.locked;
	setHids(picked(get(selected), hid, pickOf(e, locked), order, anchor, locked), locked);
	anchor = hid;
}

/**
 * A long press on an entry (da-x19yo): it is selected and the pill locked,
 * so the taps after it add and remove entries. vividn, 2026-10-22: «If you
 * long press on a piece of data, it locks the pill, letting you then do
 * multi select». Added to a selection already locked; otherwise alone.
 */
export function selectHeld(hid: string): void {
	const pill = get(pills).find((p) => p.kind === 'hid');
	const kept = pill?.locked ? get(selected).filter((h) => !same(h, hid)) : [];
	setHids([...kept, hid], true);
	anchor = hid;
}

/** Entries that are gone (a `delete` ran) leave the selection. */
export const dropHids = (gone: string[]): void => {
	const pill = get(pills).find((p) => p.kind === 'hid');
	if (pill) setHids(hidsOf(pill).filter((h) => !gone.some((g) => same(g, h))), !!pill.locked);
};

/**
 * The command widget slid up over the output section (da-xw655), opened
 * and closed by the `❯` (Prompt.svelte). vividn, 2026-10-27: «the ❯ become
 * a button … When it is clicked … the command widget slides up overtop the
 * output section»; it closes on the `❯` again, on Escape, on a run
 * (`pressWithPills`) and on a choice that fills the line (CommandPane).
 */
export const commandsOpen = writable(false);

/**
 * What is typed at the command line, wherever it is drawn. THE buffer:
 * every `Prompt.svelte` binds its box straight to this store.
 */
export const text = writable('');

/**
 * Run a line the reader typed — the one submit path, for the bar and for
 * every input widget (da-3zon).
 *
 * `pressWithPills` is the rest of it: the pills go into the argv after the
 * verb, the line is echoed as typed, the receipt lands on `last` and in
 * the shared history, and a success spends the unlocked pills. Clearing
 * the box is the caller's, because the box is `text` and both views empty
 * together the moment it is set.
 *
 * The import back into `layout/engine.ts` closes a cycle — engine reads
 * the pills from here — which ESM allows because neither module CALLS the
 * other while it is loading.
 */
export const submitLine = (line: string): Promise<RunResult> =>
	pressWithPills(shellSplit(line), line);

/**
 * The boxes the command line is drawn in, in the order they mounted: the
 * bar's, and one per input widget on screen.
 *
 * `focusLine` is where a focus that is not a click goes — a `prefill` from
 * the fields widget today. It lands in the last WIDGET mounted, and in the
 * bar when there is none: the box a reader put on the page is the one they
 * are looking at, and the bar is the fallback that is always there.
 */
const lines: { el: HTMLInputElement; widget: boolean }[] = [];

export function registerLine(el: HTMLInputElement, widget: boolean): () => void {
	const entry = { el, widget };
	lines.push(entry);
	return () => {
		const i = lines.indexOf(entry);
		// Guarded: a `splice(-1, 1)` on a second unregister would drop
		// somebody else's box instead of nothing.
		if (i >= 0) lines.splice(i, 1);
	};
}

/**
 * Put the caret in the command line. `keyboard: false` focuses it with
 * `inputmode="none"` (da-k34p): the caret, a hardware key and Enter all
 * work, and a phone's on-screen keyboard stays down — a tap on `occur` is
 * far more often followed by a tap on a field than by typing. The box
 * gives the keyboard back itself (`Prompt.svelte`): on a tap on it, on
 * blur, and whenever the line empties — run, cleared, or a verb spent.
 */
export const focusLine = (keyboard = true): void => {
	// A widget in a tab behind is mounted too (da-rjmg), and inert: it can
	// take no caret, so the next widget line or the bar's takes it.
	const widgets = lines.filter((l) => l.widget && !l.el.closest('[inert]'));
	const el = (widgets.at(-1) ?? lines.find((l) => !l.widget))?.el;
	if (!el) return;
	if (keyboard) el.removeAttribute('inputmode');
	else el.inputMode = 'none';
	el.focus();
};

/**
 * A line to put in the command line unrun, and whether the person is
 * expected to TYPE next: the fields widget's `add <field> ` is (the
 * keyboard opens), a command button's `occur ` is not (da-k34p).
 */
export const prefill = writable<{ line: string; keyboard: boolean } | null>(null);

/**
 * The verb the command line is PENDING (da-ltux) — the fields widget
 * filters itself to what that verb can be spent on.
 *
 * Derived from `text` and stored nowhere, so a tap on the commands
 * widget's `occur` and a person typing `occur` are the same event: both
 * put the word in the one buffer, and this reads it back. A second store
 * a tap had to remember to set is a second store to fall out of step.
 */
export const pending = derived(text, verbOf);

/**
 * Spend the pending verb: the word a tap put in the box has been used, so
 * the box is empty again and the fields widget is whole (C1).
 *
 * Only when the line is that word ALONE. A box with more in it is a line
 * the reader is in the middle of typing, and a press must never take that
 * away (`ghost.test.mjs`: «a line typed while a command is in flight is
 * not overwritten, by the command or by a press»). The verb stays pending
 * while the box still says it, which is the same rule read the same way.
 */
export const clearPending = (): void => {
	const line = get(text).trim();
	if (verbOf(line) !== null && !/\s/.test(line)) text.set('');
};

export type { Verb };

export interface Ghost {
	/** The command, as the scrollback echoes it. */
	line: string;
}

/**
 * The last command the reader did NOT type into a prompt — a widget-bar
 * press, the console (`press` in layout/engine.ts, da-jprg).
 *
 * The bar shows it where the placeholder goes, dim, and lets it fade: the
 * GUI teaches the CLI, so a press has to say what it ran, and the REPL's
 * scrollback is behind a tab on a phone. Never the input's value — a box
 * that refills itself after Enter reads as text the reader typed and may
 * edit, which is the bug this store replaced. A line the reader typed
 * never comes back to the box at all.
 *
 * A fresh object every time, identical line or not, so a second press
 * restarts the fade instead of riding out the first one's.
 *
 * The timer is HERE, as `spent`'s is, rather than in a view: there is more
 * than one prompt on the page now (da-3zon) and a ghost taken by whichever
 * one mounted first would be a ghost the others never saw.
 */
export const ghost = writable<Ghost | null>(null);

let haunting: ReturnType<typeof setTimeout> | undefined;

/** `press` says a command nobody typed was pressed — at the press, before it runs (da-jrozq). */
export const showGhost = (line: string): void => {
	const next: Ghost = { line };
	ghost.set(next);
	clearTimeout(haunting);
	haunting = setTimeout(() => ghost.update((g) => (g === next ? null : g)), GHOST_MS);
};

export interface LastRun {
	line: string;
	ranAt: number;
	result: RunResult;
}

export const last = writable<LastRun | null>(null);

/** `record` says a command ran. */
export const noteRun = (line: string, ranAt: number, result: RunResult): void => {
	last.set({ line, ranAt, result });
	mark(result);
};

/**
 * How long a command's ACTION words stay on a FOLDED bar (da-ckw3).
 *
 * vividn: "ACTION words should fade, but on a longer timescale than the input
 * text, maybe over 15 seconds or something." The ghost's `GHOST_MS` is the
 * input text's second and is untouched.
 */
export const ACTION_MS = 15_000;

/**
 * And how long a failed command's `ERROR` stays: "Maybe FAILED should take
 * even longer?" A minute — long enough to come back to the phone and still
 * find out that the thing you logged did not land.
 */
export const ERROR_MS = 60_000;

/**
 * What a folded bar says about the command just run (da-ckw3).
 *
 * `words` is the action of each document the CLI echoed, in the CLI's own
 * order — CREATE, UPDATE, DELETE — each with the colour datum paints it
 * (`action_color`, da-ckw3), and nothing else: a command that echoes
 * no document (a `tail`, a `now`) leaves the corner empty, and the caret is
 * one tap from what it actually answered.
 *
 * `failed` replaces them with one red `ERROR`, which opens the output on a
 * tap. It does NOT open by itself: vividn chose that word over an automatic
 * unfold, and the whole point of the fold is that the dashboard reflows
 * when a PERSON says so.
 *
 * The timer is here rather than in the view, as `ghost`'s and `spent`'s
 * are: the bar folds and unfolds under it, and a countdown a view owned
 * would start again every time the section it lives in came back.
 */
export interface Marks {
	words: { text: string; color?: string }[];
	failed: boolean;
}

export const marks = writable<Marks | null>(null);

let fading: ReturnType<typeof setTimeout> | undefined;

function mark(result: RunResult): void {
	clearTimeout(fading);
	const words = (parseEcho(result.stdout) ?? []).map((doc) => ({
		text: doc.action,
		color: doc.action_color
	}));
	// Exit 11 with an echoed document is datum's benign re-log, `EXISTS`
	// (da-9sfm, da-97w7z); exit 11 echoing nothing is a slug collision.
	const failed = result.code !== 0 && !(result.code === 11 && words.length);
	if (!failed && !words.length) return marks.set(null);
	// A fresh object every run, identical words or not, so a second command
	// restarts the fade instead of riding out the first one's.
	const next: Marks = { words, failed };
	marks.set(next);
	fading = setTimeout(
		() => marks.update((m) => (m === next ? null : m)),
		failed ? ERROR_MS : ACTION_MS
	);
}
