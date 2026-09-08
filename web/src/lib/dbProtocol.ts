// The message protocol between db.ts (main thread) and db.worker.ts.
//
// One statement plus its bindings goes out, rows come back — that is all
// the database layer ever needs to say across the wire. Rows arrive as
// plain objects and `rowToEntry` in db.ts maps them, so the worker stays a
// database and nothing more.

/** A result row, exactly as sqlite's `rowMode: 'object'` produces it. */
export type Row = Record<string, unknown>;

/**
 * Where the open database actually lives.
 *
 * The two memory kinds are the same storage and a different diagnosis.
 * `memory` is "this browser gave the worker no durable place to put the
 * database"; `memory-other-tab` is "another page of this origin already
 * holds it" — the OPFS pool is single-writer — which is the one the reader
 * can do something about (da-60ww).
 */
export type StorageKind = 'opfs' | 'memory' | 'memory-other-tab';

export type DbCommand =
	/** Open the database if it isn't open yet, and report where it landed. */
	| { op: 'open' }
	| { op: 'exec'; sql: string; bind?: Row };

/** A command with the tag that pairs its response to it. */
export type DbRequest = DbCommand & { id: number };

/**
 * An unsolicited word from the worker, paired to no request.
 *
 * Storage can change after the answer that reported it: a tab that opened in
 * memory because another held the pool takes the pool over when that tab
 * closes. The page has to hear about that to stop warning about data it is no
 * longer at risk of losing, and nothing it asked for would carry the news.
 */
export type DbNotice = { notice: 'storage'; storage: StorageKind };

export type DbResponse =
	| { id: number; ok: true; storage: StorageKind; rows: Row[] }
	| { id: number; ok: false; error: string };

/** Anything the worker may post: an answer, or news nobody asked for. */
export type DbMessage = DbResponse | DbNotice;
