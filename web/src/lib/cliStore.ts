// What the prompts remember: every command run, when, and what it printed,
// plus the recall history Up walks.
//
// One store for every prompt on the page (da-0dhc), and one for each
// database (da-kb18k): a page shows its own database's commands alone. The
// CLI widget shows all of it; the bottom bar shows the last command's
// receipt and none of what they printed. Both write through `record`, so however many prompts are
// on screen there is one writer of the localStorage key — two components
// persisting their own copies would each overwrite the other's commands.
//
// It survives a reload, out of `localStorage`, because a terminal you
// can't scroll back through yesterday is a terminal that forgets what you
// told it. Restoring is not re-running: what comes back is the text that
// was printed, never a second execution of the command that printed it.

import { derived, get, writable } from 'svelte/store';
import { browser } from '$app/env';
import type { RunResult } from './run.js';
import { noteRun } from './bar.js';
import { maskTicket, syncStates } from './sync.js';
import { currentDb, DEFAULT_DB } from './database.js';
import { parseEcho, type EchoDoc } from './echo.js';

/**
 * `of` is the `ranAt` of the command whose undo or redo button ran this one
 * (da-25q3k): the strip draws it as that command's mark, never as a command
 * of its own.
 */
export type Command = { line: string; ranAt: number; result: RunResult; of?: number };

export interface CliState {
	scrollback: Command[];
	history: string[];
}

/**
 * Where the scrollback is kept, and how many commands of it.
 *
 * The version is in the KEY, not in the value: a store written by an
 * older shape is then not read at all, rather than read and picked
 * apart. The cap is what stops a terminal left open for a month from
 * growing without bound — oldest dropped, the way scrollback goes. The
 * key is named for the REPL, which the CLI widget was until da-mwp4m:
 * renaming it would lose every device's scrollback.
 *
 * A database's own (da-kb18k). The default keeps the key every database
 * once shared, so what it held, all of them mixed with nothing saying
 * which, is `datum`'s, and the others start empty. The page's database is
 * read once, as the store is: changing it is a reload (Brand.svelte).
 */
const keyOf = (db: string): string => (db === DEFAULT_DB ? 'datum.repl.v1' : `datum.repl.v1:${db}`);
const STORE_KEY = keyOf(currentDb());
const KEEP = 200;

// Read once, when the module loads, so a prompt mounting later — or a
// second one — finds the scrollback simply there, first paint included.
export const cli = writable<CliState>(restore());

/** Read back what a previous session left, or start empty. */
function restore(): CliState {
	try {
		const raw = localStorage.getItem(STORE_KEY);
		const saved = raw ? JSON.parse(raw) : null;
		if (Array.isArray(saved?.scrollback) && Array.isArray(saved?.history)) {
			return {
				scrollback: saved.scrollback.filter(
					(entry: Command) =>
						typeof entry?.line === 'string' &&
						typeof entry?.ranAt === 'number' &&
						typeof entry?.result?.code === 'number'
				),
				history: saved.history.filter((line: string) => typeof line === 'string')
			};
		}
	} catch {
		// A private window, a store the browser blocks, a shape from a
		// build that is gone: none of them are worth an error in front of
		// someone trying to type a command. The session works, it just
		// starts empty.
	}
	return { scrollback: [], history: [] };
}

/**
 * A command has run: into the scrollback with its result and the instant
 * it was run, and into the recall history unless it repeats the last one.
 */
export function record(line: string, ranAt: number, result: RunResult, of?: number): void {
	// A sync ticket is one-time, but no secret belongs in a store kept on
	// disk and recalled with Up (da-l9kdr).
	line = maskTicket(line);
	// Past the last command's, so it names this one (da-25q3k): a step's
	// `of`, a row's `data-ran`. Two commands in one millisecond, or a held
	// clock, would share it; a millisecond on, the stamp reads the same.
	ranAt = Math.max(ranAt, (get(cli).scrollback.at(-1)?.ranAt ?? -Infinity) + 1);
	// Capped here, not only on the way to disk: a tab that stays open all
	// day would otherwise keep every command in memory and in the DOM while
	// the store on disk held the last KEEP (review threads on #156, #166).
	cli.update(({ scrollback, history }) => ({
		scrollback: [...scrollback, { line, ranAt, result, ...(of === undefined ? {} : { of }) }].slice(-KEEP),
		history: (history[history.length - 1] === line ? history : [...history, line]).slice(-KEEP)
	}));
	// The bottom bar's echo and receipt (da-0a5o): told here, so that
	// every prompt and every press that records is a command the bar shows.
	noteRun(line, ranAt, result);
	save();
}

/**
 * The scrollback cleared, as a terminal's screen is (da-3eczg): what the
 * commands printed goes, Up still recalls them, and the database is
 * untouched.
 */
export function clearScrollback(): void {
	cli.update(({ history }) => ({ scrollback: [], history }));
	save();
}

/**
 * The action words that changed the database (`Outcome` in the CLI's
 * render/mod.rs); EXISTS, NODIFF, ISLAST and FAILED moved nothing. A new
 * entry's word is its shape, which the document says is a write as `created`.
 *
 * A TRACK row names itself in that word (`TRACK●`), a `track`'s and a `get`'s
 * of one alike; only `action_color`, EXISTS's yellow on a `get`, parts them.
 */
const WRITES = new Set(['UPDATE', 'DELETE', 'OWRITE', 'RENAME', 'RETIME', 'UNDONE', 'REVERT', 'IGNORE', 'FINISH']);
const EXISTS_COLOR = '#e5a50a';
const wroteDoc = (d: EchoDoc): boolean =>
	d.created || WRITES.has(d.action) || (d.action.startsWith('TRACK') && d.action_color !== EXISTS_COLOR);
const wrote = new WeakMap<Command, string[]>();

/** The entries a command wrote, by hid, out of its echo (da-25q3k). */
export function wroteBy(entry: Command): string[] {
	let hids = wrote.get(entry);
	if (!hids) {
		hids = [...new Set((parseEcho(entry.result.stdout) ?? []).filter(wroteDoc).map((d) => d.hid))];
		wrote.set(entry, hids);
	}
	return hids;
}

/** A button's undo or redo that moved something: drawn as the mark on the command it stepped. */
export const stepped = (entry: Command): boolean => entry.of !== undefined && wroteBy(entry).length > 0;

/**
 * Where the undo button of a command that wrote stands (da-25q3k): `undone`
 * by the step that took it back, whose answer it then shows (da-hbpkw), and
 * `buried` by the newest later
 * command that wrote one of its entries — `revert <hid>` steps an entry back
 * its LAST change, which is then that one's, so the button is refused.
 */
export interface Mark {
	undone?: Command;
	buried?: Command;
}

/** Every command that wrote, and its mark; read from the scrollback, so a reload keeps them. */
export const marks = derived(cli, ({ scrollback }) => {
	const out = new Map<Command, Mark>();
	scrollback.forEach((entry, i) => {
		const hids = wroteBy(entry);
		if (entry.of !== undefined || !hids.length) return;
		const mark: Mark = {};
		for (const later of scrollback.slice(i + 1)) {
			if (later.of === entry.ranAt) {
				// A button's step is always a `revert`, which each press turns back the other way.
				if (stepped(later)) mark.undone = mark.undone ? undefined : later;
			} else if (wroteBy(later).some((hid) => hids.includes(hid))) mark.buried = later;
		}
		out.set(entry, mark);
	});
	return out;
});

let pending: ReturnType<typeof setTimeout> | undefined;

/** Queue a write; commands arrive in bursts and the store is not fast. */
function save(): void {
	clearTimeout(pending);
	pending = setTimeout(persist, 100);
}

function persist(): void {
	clearTimeout(pending);
	const { scrollback, history } = get(cli);
	let keep = scrollback.slice(-KEEP);
	for (;;) {
		try {
			localStorage.setItem(
				STORE_KEY,
				JSON.stringify({ scrollback: keep, history: history.slice(-KEEP) })
			);
			return;
		} catch {
			// Out of quota — a drawn view's stdout is a whole SVG — or a
			// store that refuses every write. Shed the oldest half and try
			// again; when there is nothing left to shed it was the store.
			if (keep.length === 0) return;
			keep = keep.slice(Math.ceil(keep.length / 2));
		}
	}
}

/**
 * Every command `db` ran and what it printed, gone (da-3ni5m): a purge
 * leaves none of the purged database on this device, its scrollback
 * included, and no other database's (da-kb18k).
 */
export function forget(db: string): void {
	if (keyOf(db) === STORE_KEY) {
		clearTimeout(pending);
		cli.set({ scrollback: [], history: [] });
	}
	try {
		localStorage.removeItem(keyOf(db));
	} catch {
		// Refused: nothing was kept to forget.
	}
}

/** The purges ordered elsewhere that this database's scrollback was cleared for, by notice subject. */
const PURGED_KEY = `${STORE_KEY}.purged`;

/**
 * This copy purged by another member's order (da-pvoai), `n` the notice the
 * copy keeps of it: the commands run before it go with the data they printed,
 * as a purge run here takes them (`forget`), and so does what was typed, the
 * values in it the purged data's too. In their place the CLI's notice, once,
 * which names where this copy's backup is. A command run after the order's
 * instant stays, as the entries it wrote do. Every tab on the database does
 * it: the first writes it to the store and marks it done, the others read it
 * back from there.
 */
function purged(n: { subject: string; at: string; text: string }): void {
	let done: string[] = [];
	try {
		done = JSON.parse(localStorage.getItem(PURGED_KEY) ?? '[]');
	} catch {
		// Unread: cleared again, which changes nothing it has not already.
	}
	if (done.includes(n.subject)) return;
	const at = Date.parse(n.at) || Date.now();
	const notice: Command = { line: 'sync chain purge', ranAt: at, result: { argv: [], code: 0, stdout: n.text, stderr: '' } };
	cli.update(({ scrollback }) => ({ scrollback: [notice, ...scrollback.filter((c) => c.ranAt > at)], history: [] }));
	persist();
	try {
		localStorage.setItem(PURGED_KEY, JSON.stringify([...done, n.subject].slice(-KEEP)));
	} catch {
		// Not marked: the next state heard clears it again, to the same.
	}
}

if (browser) {
	// A reload is the one moment a debounced write would lose: flush.
	addEventListener('pagehide', persist);
	addEventListener('storage', (e) => {
		// Another tab forgot, so this one does, or its next command writes it all back.
		if (e.key === STORE_KEY && e.newValue === null) cli.set({ scrollback: [], history: [] });
		// Another tab cleared a purge, and wrote what it left first.
		if (e.key === PURGED_KEY) {
			clearTimeout(pending);
			cli.set(restore());
		}
	});
	// A purge reaches this copy as a sync applies it, said by the notice its sync leader carries to every tab.
	syncStates.subscribe((all) => {
		for (const n of all[currentDb()]?.notices ?? []) if (n.kind === 'wipe' && n.subject.startsWith('purge:')) purged(n);
	});
}
