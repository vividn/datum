// The main thread's handle on the database worker.
//
// The CLI itself runs in $lib/db.worker.ts, because OPFS is reachable only
// from a dedicated worker (da-5iqw). This file is the client side of that:
// one command out, one process's outcome back, and the storage it ran on.

import type { CliResult, DbCommand, DbMessage, DbResponse, Stage, StorageKind } from './dbProtocol.js';
import { announceChange } from './changes.js';
import { answered, blocked, upgrading } from './waiting.js';
import { DAMAGED, damaged } from './damaged.js';
import { DEFAULT_DB } from './database.js';
import { dbOf } from './run.js';

export type { StorageKind };

let worker: Worker | null = null;
let nextId = 0;
const pending = new Map<number, (response: DbResponse) => void>();
/** Who hears a running request's stderr, by request id. */
const listeners = new Map<number, (text: string) => void>();
let storage: StorageKind | null = null;
let opened: Promise<void> | null = null;

const watchers = new Set<(kind: StorageKind) => void>();
const durableWatchers = new Set<() => void>();

/**
 * Called when storage changes underneath the page — which happens when a tab
 * that opened in memory gets the file and moves its entries there. Returns
 * its own unsubscribe.
 */
export function onStorageChange(watcher: (kind: StorageKind) => void): () => void {
	watchers.add(watcher);
	return () => watchers.delete(watcher);
}

/**
 * Called after each write that lands in durable storage — never for one held
 * in memory, where there is nothing a browser could be asked to keep
 * (da-frg2). Returns its own unsubscribe.
 */
export function onDurableWrite(watcher: () => void): () => void {
	durableWatchers.add(watcher);
	return () => durableWatchers.delete(watcher);
}

/**
 * Whether entries survive a reload. Null before the database is open.
 *
 * The UI states this rather than assuming it — see $lib/BetaBanner.svelte.
 */
export function storageKind(): StorageKind | null {
	return storage;
}

function setStorage(kind: StorageKind): void {
	if (kind === storage) return;
	storage = kind;
	for (const watcher of watchers) watcher(kind);
}

/**
 * Start the worker, lazily.
 *
 * Lazily because this module is imported during prerendering, where there
 * is no `Worker` — every caller reaches the database from `onMount` or a
 * later event, so the worker is only ever constructed in the browser.
 */
function ensureWorker(): Worker {
	if (worker) return worker;
	const started = new Worker(new URL('./db.worker.ts', import.meta.url), { type: 'module' });
	started.onmessage = (event: MessageEvent<DbMessage>) => {
		const message = event.data;
		if ('notice' in message) {
			if (message.notice === 'stderr') return listeners.get(message.id)?.(message.text);
			// From the first: one process may snapshot more than one database.
			if (message.notice === 'upgrading') return upgrading.update((since) => since ?? performance.now());
			// Dated from when the wait BEGAN, so the line changes its words in
			// place and keeps counting, rather than blink back to 0 s (kimi-k3 on #493).
			if (message.notice === 'blocked')
				return blocked.set(message.waited === null ? null : performance.now() - message.waited);
			// A tab that was in memory just moved its rows onto the file,
			// which is news for every other tab.
			setStorage(message.storage);
			return announceChange();
		}
		const settle = pending.get(message.id);
		pending.delete(message.id);
		settle?.(message);
	};
	// A worker that fails to load answers nothing, so its requests would
	// hang forever. Fail them loudly and drop the worker, so the next call
	// gets a fresh one instead of talking to a corpse.
	started.onerror = (event) => {
		for (const [id, settle] of [...pending]) {
			pending.delete(id);
			settle({ id, ok: false, error: `database worker failed: ${event.message}` });
		}
		if (worker === started) worker = null;
		// Its wait for the files, if it was in one, is not this page's any more.
		blocked.set(null);
	};
	worker = started;
	return started;
}

/** Whether nothing is out: every request sent to the worker has answered. */
export const dry = (): boolean => pending.size === 0;

/** Send one request to the worker and resolve with its answer. */
function request(
	command: DbCommand,
	onStderr?: (text: string) => void
): Promise<Extract<DbResponse, { ok: true }>> {
	const id = ++nextId;
	const started = ensureWorker();
	if (onStderr) listeners.set(id, onStderr);
	return new Promise((resolve, reject) => {
		pending.set(id, (response) => {
			listeners.delete(id);
			// One process at a time: an answer is the end of any upgrade it ran.
			if (command.op === 'cli') answered(pending.size === 0);
			if (!response.ok) return reject(new Error(response.error));
			setStorage(response.storage);
			resolve(response);
		});
		started.postMessage({ ...command, id });
	});
}

/** Find out where the database lives (idempotent). */
export function initDb(): Promise<void> {
	opened ??= request({ op: 'open' }).then(
		() => {},
		(e) => {
			opened = null;
			throw e;
		}
	);
	return opened;
}

/**
 * Run `datum <argv>` in the worker ($lib/cli.ts).
 *
 * A process that wrote is news: for every other tab, which re-runs its panes
 * on it, and — when the write is durable — for the persistence request.
 * `null` when a newer command with the same `key` superseded it unrun.
 * `session`: a sync session's own run, announced as one (`announceChange`)
 * and queued behind the panes ($lib/cli.ts).
 */
export async function runCli(
	argv: string[],
	env: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string,
	stage?: Stage,
	session?: boolean,
	pane?: boolean
): Promise<CliResult | null> {
	const cli = (await request({ op: 'cli', argv, env, stdin, key, stage, pane, session }, onStderr)).cli;
	if (!cli) return null;
	// A file SQLite cannot read fails every command on it, `db purge` too:
	// the page offers the way out that does not go through SQLite (da-0v940).
	if (cli.code !== 0 && DAMAGED.test(cli.stderr)) damaged.set(dbOf(argv, DEFAULT_DB));
	if (cli.changed.length) {
		announceChange(cli.changed, session);
		if (cli.storage === 'opfs') {
			// Each watcher on its own: the command has already run, and a hook
			// that throws (the persistence prompt, or any later one) must not
			// turn a written row into a rejected promise (review thread on #160).
			for (const watcher of durableWatchers) {
				try {
					watcher();
				} catch (error) {
					console.error('durable-write watcher failed', error);
				}
			}
		}
	}
	return cli;
}
