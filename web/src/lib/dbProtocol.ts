// The message protocol between db.ts (main thread) and db.worker.ts.
//
// One command goes out — argv, env, and stdin if it reads any — and one
// process's outcome comes back. Nothing else crosses: the worker runs the
// real `datum` CLI and holds no opinion about what a command means.

/**
 * Where the database lives.
 *
 * `opfs` is the file in the origin's private file system, which survives a
 * reload. `memory` is this browser refusing the worker a sync access handle:
 * the tab keeps a database of its own in memory until it can have the file,
 * and says so (see $lib/BetaBanner.svelte).
 */
export type StorageKind = 'opfs' | 'memory';

/** One CLI process's outcome, and where its time went. */
export interface CliResult {
	argv: string[];
	code: number;
	stdout: string;
	stderr: string;
	/**
	 * The databases the process committed a write to, by name (`datum`,
	 * `_internal`) — other tabs refresh what reads that one.
	 */
	changed: string[];
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
	/** Find out where the database lives, and say. */
	| { op: 'open' }
	/** Run `datum <argv>` under WASI, with `stdin` on its standard input. */
	| { op: 'cli'; argv: string[]; env: string[]; stdin?: string };

/** A command with the tag that pairs its response to it. */
export type DbRequest = DbCommand & { id: number };

/**
 * An unsolicited word from the worker: a tab that fell to memory got the
 * file after all and moved its entries across — or request `id`'s process
 * wrote `text` to stderr while it is still running (da-h9vj).
 */
export type DbNotice =
	| { notice: 'storage'; storage: StorageKind }
	| { notice: 'stderr'; id: number; text: string };

export type DbResponse =
	| { id: number; ok: true; storage: StorageKind; cli?: CliResult }
	| { id: number; ok: false; error: string };

/** Anything the worker may post: an answer, or news nobody asked for. */
export type DbMessage = DbResponse | DbNotice;
