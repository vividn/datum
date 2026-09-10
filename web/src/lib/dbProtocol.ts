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
 * database"; `memory-other-tab` is "another page of this origin still holds
 * it" — the OPFS pool is single-writer.
 *
 * Since da-eovo only ONE worker per origin opens the database and the rest
 * borrow it, so `memory-other-tab` no longer means a second tab: it is the
 * handoff window, where the tab that just won the lock asks for the pool
 * before the closing tab's sync access handles are released. It heals by
 * itself within a retry, which is why the banner tells the reader to wait
 * rather than to close anything (da-60ww, da-eovo).
 */
export type StorageKind = 'opfs' | 'memory' | 'memory-other-tab';

/**
 * How the wasi CLI holds its database files (da-1vat, see $lib/cli.ts):
 * for the owner worker's life, or for one command.
 */
export type CliLock = 'owner' | 'command';

/** One CLI process's outcome, plus what the spike measures about it. */
export interface CliResult {
	argv: string[];
	code: number;
	stdout: string;
	stderr: string;
	/** Whether the process committed a write — other tabs refresh on it. */
	changed: boolean;
	storage: StorageKind;
	/** Where the time went, in ms; `tries` is how many opens the handles took. */
	ms: {
		compile: number;
		acquire: number;
		instantiate: number;
		run: number;
		flush: number;
		tries: number;
	};
}

export type DbCommand =
	/** Open the database if it isn't open yet, and report where it landed. */
	| { op: 'open' }
	| { op: 'exec'; sql: string; bind?: Row }
	/** Run `datum <argv>` under WASI, the CLI's own database (da-1vat). */
	| { op: 'cli'; argv: string[]; env: string[]; lock: CliLock };

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
	| { id: number; ok: true; storage: StorageKind; rows: Row[]; cli?: CliResult }
	| { id: number; ok: false; error: string };

/** Anything the worker may post: an answer, or news nobody asked for. */
export type DbMessage = DbResponse | DbNotice;
