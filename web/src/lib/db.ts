// The main thread's handle on the database worker.
//
// The CLI itself runs in $lib/db.worker.ts, because OPFS is reachable only
// from a dedicated worker (da-5iqw). This file is the client side of that:
// one command out, one process's outcome back, and the storage it ran on.

import type { CliResult, DbCommand, DbMessage, DbResponse, StorageKind } from './dbProtocol.js';
import { announceChange } from './changes.js';

export type { StorageKind };

let worker: Worker | null = null;
let nextId = 0;
const pending = new Map<number, (response: DbResponse) => void>();
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
	};
	worker = started;
	return started;
}

/** Send one request to the worker and resolve with its answer. */
function request(command: DbCommand): Promise<Extract<DbResponse, { ok: true }>> {
	const id = ++nextId;
	const started = ensureWorker();
	return new Promise((resolve, reject) => {
		pending.set(id, (response) => {
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
 */
export async function runCli(argv: string[], env: string[], stdin?: string): Promise<CliResult> {
	const cli = (await request({ op: 'cli', argv, env, stdin })).cli!;
	if (cli.changed.length) {
		announceChange(cli.changed);
		if (cli.storage === 'opfs') for (const watcher of durableWatchers) watcher();
	}
	return cli;
}
