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
import type { DbRequest, DbResponse, Row, StorageKind } from './dbProtocol.js';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SqliteDb = any;

/**
 * The worker global, typed for the two calls this file makes: TypeScript
 * resolves `self` against lib.dom here, where it is a `Window` whose
 * `postMessage` takes a target origin.
 */
const ctx = self as unknown as {
	postMessage(message: DbResponse): void;
	addEventListener(type: 'message', listener: (event: MessageEvent<DbRequest>) => void): void;
};

let storage: StorageKind = 'memory';
let opening: Promise<SqliteDb> | null = null;

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
		return opened;
	})();
	return opening;
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
 * (the API is younger than the app's floor). It loses everything on reload,
 * so it says so and reports `memory` as the storage kind rather than let
 * the UI imply durability that isn't there — see $lib/BetaBanner.svelte.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function openPersistent(sqlite3: any): Promise<SqliteDb> {
	if (typeof sqlite3.installOpfsSAHPoolVfs === 'function') {
		try {
			const pool = await sqlite3.installOpfsSAHPoolVfs({});
			storage = 'opfs';
			return new pool.OpfsSAHPoolDb('/datum.db');
		} catch (e) {
			console.warn('OPFS SAH pool unavailable:', e);
		}
	}
	console.warn('no durable storage available — entries will not survive a reload');
	storage = 'memory';
	return new sqlite3.oo1.DB(':memory:', 'c');
}

ctx.addEventListener('message', (event: MessageEvent<DbRequest>) => {
	const request = event.data;
	openOnce().then(
		(opened) => {
			try {
				const rows: Row[] = [];
				if (request.op === 'exec') {
					opened.exec({
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
