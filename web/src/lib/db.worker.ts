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
//
// ONE OF THESE WORKERS OWNS THE DATABASE; THE REST BORROW IT (da-eovo).
//
// The SAH pool is single-writer — an exclusive sync access handle per file
// per origin — so a second tab that opens for itself gets none, falls to
// memory, and neither tab can see the other's rows. That is a data-loss
// warning where local-first promises the opposite: two tabs should both
// write and stay in sync.
//
// So only ONE worker opens the database. Which one is decided by
// `navigator.locks.request(OWNER_LOCK, …)` with a callback that never
// resolves: the holder keeps the lock for as long as its worker exists,
// which is as long as its tab exists. Every other worker opens nothing and
// forwards its page's statements to the owner over a `BroadcastChannel`,
// getting the same `DbResponse` back — so from `db.ts` upwards nothing
// changed, and from OPFS's side there is still exactly one writer.
//
// A SharedWorker would be the tidier home for the owner, but cannot be:
// `createSyncAccessHandle` is exposed to dedicated workers only, so the
// owner has to be one of the tabs' own workers. When that tab closes the
// lock is released, the next tab wins it and opens the pool, and requests
// that were in flight are sent again — nothing typed is ever dropped.

import { SCHEMA } from './schema.generated.js';
import type { DbMessage, DbRequest, DbResponse, Row, StorageKind } from './dbProtocol.js';

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

/** The lock whose holder owns the database, held for this worker's life. */
const OWNER_LOCK = 'datum-db-owner';

/** Where the workers of this origin talk to one another. */
const PEER_CHANNEL = 'datum-db-peers';

/** This worker's address on that channel. */
const PEER = typeof crypto === 'undefined' ? '' : crypto.randomUUID();

/**
 * What one worker says to another.
 *
 * Four words: an owner announcing itself, a statement going out, its rows
 * coming back, and the owner's storage kind when it changes underneath
 * everyone.
 */
type PeerMessage =
	| { kind: 'owner' }
	| { kind: 'req'; from: string; request: DbRequest }
	| { kind: 'res'; to: string; response: DbResponse }
	| { kind: 'storage'; storage: StorageKind };

const peers: BroadcastChannel | null =
	typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(PEER_CHANNEL);

/** Whether this worker is the one holding the database open. */
let owning = false;

/**
 * Every request from this tab's page that has not been answered yet.
 *
 * The queue IS the promise that nothing typed is lost: a request leaves it
 * only when an answer for it comes back, and every way it could go astray —
 * sent before any owner was elected, sent to an owner whose tab then closed
 * — ends in it being sent again.
 */
const inflight = new Map<number, DbRequest>();

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
 * (the API is younger than the app's floor), and for the handoff window
 * where this worker has just won the lock but the closing tab's handles are
 * not released yet. It loses everything on reload, so it says so — and it
 * distinguishes the two, since only the second heals on its own:
 * `memory-other-tab` rather than `memory`, which the banner turns into
 * "nothing to do, this tab is taking it over" instead of the generic
 * sentence (da-60ww, da-eovo). See $lib/BetaBanner.svelte.
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
	} else {
		// Said rather than left alone: a worker that took the database over
		// from a closed tab starts out believing storage is `opfs`, because
		// that is what the previous owner told it, and a silent branch here
		// would leave it claiming durability it does not have.
		storage = 'memory';
	}
	console.warn(
		storage === 'memory-other-tab'
			? 'the database is still held by a closing tab — retrying; entries are in memory until then'
			: 'no durable storage available — entries will not survive a reload'
	);
	return new sqlite3.oo1.DB(':memory:', 'c');
}

/**
 * Tell the page where the database now lives, unprompted — and, if this
 * worker is the owner, tell the other tabs, whose storage this also is.
 */
function announceStorage(): void {
	ctx.postMessage({ notice: 'storage', storage });
	if (owning) peers?.postMessage({ kind: 'storage', storage } satisfies PeerMessage);
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
 * rethrows whatever the platform hands back. A handle already held — by the
 * tab that owned the database until a moment ago, since one origin is one
 * OPFS — comes back as `NoModificationAllowedError`, which is a different
 * fact from "this browser has no OPFS" and the only one that heals by
 * itself. Anything else keeps the generic diagnosis (da-60ww).
 */
function heldByAnotherTab(e: unknown): boolean {
	return (e as { name?: unknown } | null)?.name === 'NoModificationAllowedError';
}

function errorText(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

/** Run one statement against the database this worker owns. */
async function serve(request: DbRequest): Promise<DbResponse> {
	try {
		await openOnce();
		const rows: Row[] = [];
		// `db`, not what `openOnce()` resolved to: a migration may have
		// swapped the database under us since this request's open resolved.
		if (request.op === 'exec') {
			db.exec({
				sql: request.sql,
				bind: request.bind,
				rowMode: 'object',
				callback: (row: Row) => rows.push(row)
			});
		}
		return { id: request.id, ok: true, storage, rows };
	} catch (e) {
		return { id: request.id, ok: false, error: errorText(e) };
	}
}

/** Hand one answer back to this tab's page, and retire the request. */
function answerPage(response: DbResponse): void {
	// The first answer wins. A resent request can be answered twice — by the
	// owner that was about to go away and by the one that replaced it — and
	// the page asked only once.
	if (!inflight.delete(response.id)) return;
	ctx.postMessage(response);
}

/**
 * Get one request answered, from wherever the database is.
 *
 * Owner: run it. Anyone else: send it to the owner and wait. A send that
 * lands in the gap between one owner and the next is not lost, only
 * unanswered — `scheduleResend` and the next owner's announcement both send
 * it again.
 */
function pump(request: DbRequest): void {
	if (owning) {
		void serve(request).then(answerPage);
		return;
	}
	peers?.postMessage({ kind: 'req', from: PEER, request } satisfies PeerMessage);
	scheduleResend();
}

/**
 * How long an unanswered request waits before being sent again.
 *
 * Long enough that a healthy owner has answered well within it, so the
 * common case never sends twice. A statement that does arrive twice is
 * harmless anyway: a read repeats, and an insert collides with the row its
 * first run stored, which is the CLI's own exit-11 duplicate — never a lost
 * row. The first answer to come back is the one the page sees.
 */
const RESEND_MS = 3000;
let resending = false;

function scheduleResend(): void {
	if (resending || inflight.size === 0) return;
	resending = true;
	setTimeout(() => {
		resending = false;
		for (const request of inflight.values()) pump(request);
	}, RESEND_MS);
}

/** Take the database: open it, say so, and answer what has been waiting. */
function becomeOwner(): void {
	owning = true;
	peers?.postMessage({ kind: 'owner' } satisfies PeerMessage);
	// Eagerly, rather than on this tab's first statement: the other tabs
	// have nowhere to send theirs until this one is open.
	void openOnce().then(announceStorage, (e) => console.warn('database unavailable:', e));
	for (const request of inflight.values()) pump(request);
}

/**
 * Stand in line for the database.
 *
 * The callback never resolves, so the lock is held until this worker is
 * gone — which is when its tab is gone, and is exactly the moment the next
 * tab should take over.
 *
 * With no channel to forward over or no lock to elect with, every worker
 * opens for itself: that is where this started, and it is degraded (the
 * second tab falls to memory and says so) rather than lossy.
 */
function claimOwnership(): void {
	const locks = typeof navigator === 'undefined' ? undefined : navigator.locks;
	if (!peers || !locks) {
		becomeOwner();
		return;
	}
	void locks.request(OWNER_LOCK, () => {
		becomeOwner();
		return new Promise<never>(() => {});
	});
}

peers?.addEventListener('message', (event: MessageEvent<PeerMessage>) => {
	const message = event.data;
	switch (message.kind) {
		case 'owner':
			// A new owner. Anything sent into the gap it just closed needs
			// sending again — nothing else knows those requests exist.
			if (!owning) for (const request of inflight.values()) pump(request);
			return;
		case 'req':
			if (!owning) return;
			void serve(message.request).then((response) =>
				peers?.postMessage({ kind: 'res', to: message.from, response } satisfies PeerMessage)
			);
			return;
		case 'res':
			if (message.to === PEER) answerPage(message.response);
			return;
		case 'storage':
			// The owner's storage is this tab's storage.
			if (!owning) {
				storage = message.storage;
				announceStorage();
			}
			return;
	}
});

ctx.addEventListener('message', (event: MessageEvent<DbRequest>) => {
	const request = event.data;
	inflight.set(request.id, request);
	pump(request);
});

claimOwnership();
