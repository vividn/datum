// The dedicated worker that owns SQLite.
//
// SQLite runs here rather than on the main thread because that is the only
// place OPFS works: every sqlite-wasm OPFS VFS needs
// `FileSystemFileHandle.createSyncAccessHandle`, which the platform exposes
// to dedicated workers only. On the main thread the SAH-pool install fails
// with "Missing required OPFS APIs" and `oo1.OpfsDb` with "requires
// Atomics.wait()", so a page-hosted database could never be anything but
// in-memory and every entry died on reload (da-5iqw).
//
// The protocol is deliberately thin — see $lib/dbProtocol.ts. Requests are
// answered in order: `exec` is synchronous once the database is open, and a
// worker runs one turn at a time, so no statement can interleave with
// another.

import { SCHEMA } from './schema.generated.js';
import type { DbMessage, DbRequest, Row, StorageKind } from './dbProtocol.js';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SqliteDb = any;

/**
 * The worker global, typed for the two calls this file makes: TypeScript
 * resolves `self` against lib.dom here, where it is a `Window` whose
 * `postMessage` takes a target origin.
 */
const ctx = self as unknown as {
	postMessage(message: DbMessage): void;
	addEventListener(type: 'message', listener: (event: MessageEvent<DbRequest>) => void): void;
};

let storage: StorageKind = 'memory';
let opening: Promise<SqliteDb> | null = null;

/**
 * The live database. Mutable because a memory database is not the end of the
 * story: `keepTrying()` reopens durable storage when it becomes available and
 * moves what was typed in the meantime across, and every later statement has
 * to land on the new one.
 */
let db: SqliteDb = null;

/** Open the database once; every later caller shares the first promise. */
function openOnce(): Promise<SqliteDb> {
	opening ??= (async () => {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		const sqlite3InitModule = ((await import('@sqlite.org/sqlite-wasm')) as any).default;
		const sqlite3 = await sqlite3InitModule({
			print: console.log,
			printErr: console.error
		});

		const opened = await openPersistent(sqlite3);
		opened.exec(SCHEMA);
		db = opened;
		// Degraded is a state to recover FROM, not just to report. A tab that
		// opened while another held the pool keeps its entries in memory, and
		// they are lost on reload unless something goes back for them.
		if (storage !== 'opfs') keepTrying(sqlite3);
		return opened;
	})();
	return opening;
}

/**
 * How long the durable open gets before the fallback takes over.
 *
 * Generous on purpose: a successful install is sub-second, so anything
 * near this bound is a browser that is not going to answer at all.
 */
const OPEN_DEADLINE_MS = 3000;

/**
 * `promise`, but rejecting if `ms` pass with nothing settled.
 *
 * An install that resolves after expiry is abandoned rather than closed:
 * there is no handle to close until it settles, and by then the memory
 * database is the one in use.
 */
function withDeadline<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
	const expiry = new Promise<never>((_, reject) => {
		setTimeout(() => reject(new Error(message)), ms);
	});
	return Promise.race([promise, expiry]);
}

/**
 * Open the on-device database, preferring durable storage.
 *
 * The SAH-pool VFS, not `oo1.OpfsDb`: the plain OPFS VFS needs
 * `Atomics.wait()` on a `SharedArrayBuffer`, so it also needs the document
 * to be cross-origin-isolated. The pool needs only the sync access handle
 * this worker already has, which makes persistence independent of whether
 * the deployment manages to send COOP/COEP.
 *
 * The in-memory fallback is for a browser with no sync access handle at all
 * (the API is younger than the app's floor) and for a second tab, which
 * cannot have one because the pool is single-writer. It loses everything on
 * reload, so it says so — and it distinguishes the two, since only the
 * second tab has a remedy: `memory-other-tab` rather than `memory`, which
 * the banner turns into "use that tab, or close it and reload this one"
 * instead of the generic sentence (da-60ww). See $lib/BetaBanner.svelte.
 *
 * The deadline is what makes that fallback reachable. A browser that will
 * not hand this worker a sync access handle does not always say so: the
 * install can simply never settle, and a `catch` cannot fire on a promise
 * that never rejects, so the page sits at "opening database…" forever
 * (da-mskb). Expiry is treated exactly as a rejection is — memory, and a
 * banner that says memory — which turns an unbounded wait into a state the
 * UI already knows how to show.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function openPersistent(sqlite3: any): Promise<SqliteDb> {
	if (typeof sqlite3.installOpfsSAHPoolVfs === 'function') {
		try {
			const pool: SqliteDb = await withDeadline(
				sqlite3.installOpfsSAHPoolVfs({}),
				OPEN_DEADLINE_MS,
				`no sync access handle within ${OPEN_DEADLINE_MS}ms`
			);
			storage = 'opfs';
			return new pool.OpfsSAHPoolDb('/datum.db');
		} catch (e) {
			console.warn('OPFS SAH pool unavailable:', e);
			storage = heldByAnotherTab(e) ? 'memory-other-tab' : 'memory';
		}
	}
	console.warn(
		storage === 'memory-other-tab'
			? 'another datum.day tab holds the database — entries here will not survive a reload'
			: 'no durable storage available — entries will not survive a reload'
	);
	return new sqlite3.oo1.DB(':memory:', 'c');
}

/** Tell the page where the database now lives, unprompted. */
function announceStorage(): void {
	ctx.postMessage({ notice: 'storage', storage });
}

/** How long to wait before the first retry, and the ceiling it backs off to. */
const RETRY_FIRST_MS = 2000;
const RETRY_MAX_MS = 30000;

/**
 * Go back for durable storage until we get it.
 *
 * The common degraded case is a second tab: the SAH pool is exclusive per
 * origin, so tab two opens in memory and stays there for as long as tab one
 * is open — which is usually the moment someone types the entry they care
 * about. Closing the first tab should heal the second, and without this it
 * does not, because the open is attempted exactly once.
 *
 * Backs off to a half-minute and then keeps going. It is a `setTimeout` with
 * no work in it when storage is already durable, so the cost of waiting a
 * long time is nil, and the alternative is a tab that has silently given up
 * on the user's data.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function keepTrying(sqlite3: any, delay = RETRY_FIRST_MS): void {
	setTimeout(() => {
		void (async () => {
			let durable: SqliteDb = null;
			try {
				if (typeof sqlite3.installOpfsSAHPoolVfs !== 'function') return;
				// `forceReinitIfPreviouslyFailed` is the entire reason a retry can
				// work: sqlite-wasm CACHES the install promise per VFS name, so
				// without it every later call returns the first rejection and
				// this loop would spin on a stale answer forever. Measured: 45s
				// of retries, none of which reached OPFS, before this line.
				const pool: SqliteDb = await withDeadline(
					sqlite3.installOpfsSAHPoolVfs({ forceReinitIfPreviouslyFailed: true }),
					OPEN_DEADLINE_MS,
					`no sync access handle within ${OPEN_DEADLINE_MS}ms`
				);
				durable = new pool.OpfsSAHPoolDb('/datum.db');
			} catch {
				// Still held, or still unsupported. Either way, try again.
				keepTrying(sqlite3, Math.min(delay * 2, RETRY_MAX_MS));
				return;
			}
			try {
				// EVERYTHING FROM HERE IS SYNCHRONOUS, AND THAT IS THE WHOLE
				// SAFETY ARGUMENT. A worker runs one turn at a time, so with no
				// `await` between opening the durable database and swapping it
				// in, no `exec` request can be served against a half-migrated
				// pair — the move is atomic with respect to the message loop.
				durable.exec(SCHEMA);
				const moved = copyAcross(db, durable);
				const previous = db;
				db = durable;
				storage = 'opfs';
				previous.close();
				announceStorage();
				console.info(`durable storage recovered; moved ${moved} rows across`);
			} catch (e) {
				// The migration failed, so the memory database is still the one
				// holding the user's entries and must stay live.
				console.warn('could not migrate into durable storage:', e);
				try {
					durable.close();
				} catch {
					/* nothing further to do */
				}
				keepTrying(sqlite3, Math.min(delay * 2, RETRY_MAX_MS));
			}
		})();
	}, delay);
}

/**
 * Copy every row of every table from one database into another, returning how
 * many rows moved.
 *
 * Reads the table list out of `sqlite_master` rather than naming tables here,
 * so a schema change cannot leave this silently copying a subset — which is
 * the failure that would lose exactly the data this function exists to save.
 *
 * `INSERT OR IGNORE` because the destination may not be empty: another tab may
 * have written to it while this one was in memory. On a key collision the
 * durable row wins, since it is the one that survived a reload; ours is at
 * worst a duplicate of it.
 */
function copyAcross(from: SqliteDb, to: SqliteDb): number {
	const tables: string[] = [];
	from.exec({
		sql: "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
		rowMode: 'object',
		callback: (row: Row) => tables.push(String(row.name))
	});

	// All or nothing on the destination. A copy that dies halfway must not
	// leave the durable database holding some tables and not others, since
	// the caller keeps the memory database live on failure and will try the
	// whole copy again.
	to.exec('BEGIN');
	let moved = 0;
	try {
		for (const table of tables) {
			// `table_xinfo`, not `SELECT *`: the schema has generated columns,
			// and SQLite refuses an INSERT that names one ("cannot INSERT into
			// generated column"). `hidden` is 0 for an ordinary column, 1 for
			// a hidden virtual-table column, 2 or 3 for generated. Measured:
			// the first cut of this copied nothing at all, four retries running.
			const columns: string[] = [];
			from.exec({
				sql: `PRAGMA table_xinfo("${table}")`,
				rowMode: 'object',
				callback: (c: Row) => {
					if (c.hidden === 0) columns.push(String(c.name));
				}
			});
			if (columns.length === 0) continue;
			const quoted = columns.map((c) => `"${c}"`).join(', ');
			const holes = columns.map(() => '?').join(', ');
			const rows: Row[] = [];
			from.exec({
				sql: `SELECT ${quoted} FROM "${table}"`,
				rowMode: 'object',
				callback: (r: Row) => rows.push(r)
			});
			for (const row of rows) {
				to.exec({
					sql: `INSERT OR IGNORE INTO "${table}" (${quoted}) VALUES (${holes})`,
					bind: columns.map((c) => row[c])
				});
				moved += 1;
			}
		}
		to.exec('COMMIT');
	} catch (e) {
		to.exec('ROLLBACK');
		throw e;
	}
	return moved;
}

/**
 * Whether the pool failed because another page of this origin holds it.
 *
 * The pool asks each of its files for a `createSyncAccessHandle` and
 * rethrows whatever the platform hands back. A handle already held —
 * by a second tab, since one origin is one OPFS — comes back as
 * `NoModificationAllowedError`, which is a different fact from "this
 * browser has no OPFS" and the only one with a remedy the reader can act
 * on. Anything else keeps the generic diagnosis (da-60ww).
 */
function heldByAnotherTab(e: unknown): boolean {
	return (e as { name?: unknown } | null)?.name === 'NoModificationAllowedError';
}

ctx.addEventListener('message', (event: MessageEvent<DbRequest>) => {
	const request = event.data;
	openOnce().then(
		() => {
			try {
				const rows: Row[] = [];
				// `db`, not `opened`: a migration may have swapped the database
				// under us since this request's `openOnce()` resolved.
				if (request.op === 'exec') {
					db.exec({
						sql: request.sql,
						bind: request.bind,
						rowMode: 'object',
						callback: (row: Row) => rows.push(row)
					});
				}
				ctx.postMessage({ id: request.id, ok: true, storage, rows });
			} catch (e) {
				ctx.postMessage({ id: request.id, ok: false, error: errorText(e) });
			}
		},
		(e) => ctx.postMessage({ id: request.id, ok: false, error: errorText(e) })
	);
});

function errorText(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}
