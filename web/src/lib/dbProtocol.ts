// The message protocol between db.ts (main thread) and db.worker.ts.
//
// One statement plus its bindings goes out, rows come back — that is all
// the database layer ever needs to say across the wire. Rows arrive as
// plain objects and `rowToEntry` in db.ts maps them, so the worker stays a
// database and nothing more.

/** A result row, exactly as sqlite's `rowMode: 'object'` produces it. */
export type Row = Record<string, unknown>;

/** Where the open database actually lives. */
export type StorageKind = 'opfs' | 'memory';

export type DbCommand =
	/** Open the database if it isn't open yet, and report where it landed. */
	| { op: 'open' }
	| { op: 'exec'; sql: string; bind?: Row };

/** A command with the tag that pairs its response to it. */
export type DbRequest = DbCommand & { id: number };

export type DbResponse =
	| { id: number; ok: true; storage: StorageKind; rows: Row[] }
	| { id: number; ok: false; error: string };
