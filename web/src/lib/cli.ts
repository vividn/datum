// The real `datum` CLI, one process per command (da-1vat, da-g8o8).
//
// Runs inside $lib/db.worker.ts, the only place OPFS sync access handles
// exist. The module is `datum-cli` built for wasm32-wasip1
// (scripts/build-cli-wasm.mjs): compiled once per worker, instantiated
// afresh for every command, with argv, env, stdin, stdout and stderr
// exactly as a shell would give them, and `/data` preopened over the
// database files in $lib/wasiFs.ts. What it prints is what the CLI prints;
// there is nothing here that knows what a command means.
//
// EVERY COMMAND TAKES THE FILES FOR ITSELF. It opens a sync access handle on
// each database file at process start and closes them at exit, so nothing is
// held between commands and every tab runs its own — there is no owner tab
// to elect, forward to, or lose. The spike measured both this and an
// owner-tab design; this one was faster under two tabs (26.6 ms median
// against 101) and cannot run a command twice, where the owner's resend
// could (da-1vat).
//
// Tabs take turns through a Web Lock held for the length of one process.
// The handles alone would keep two processes apart, but not fairly: a tab
// with commands queued closes its handles and reopens them before another
// tab's retry comes round, and twenty writes across two tabs starved one of
// them past its deadline. The lock is first come, first served across the
// origin, and the browser releases it when a tab dies mid-command. Without
// Web Locks, a command that finds the files held waits and tries again.
//
// A browser that will not hand the worker a sync access handle gets a
// database in memory instead, for the tab's life, and a tab in memory keeps
// going back for the file: when it gets it, what was typed meanwhile moves
// across through the CLI's own `db backup -` and `db restore -`.

import { File as MemFile, OpenFile, WASI, type Fd } from '@bjorn3/browser_wasi_shim';
import cliWasm from './wasm/datum-cli.wasm?url';
import { DATABASES, DB_FILES, MemoryHandle, opfsDirectory, Sink, type Handle } from './wasiFs.js';
import type { CliResult, StorageKind } from './dbProtocol.js';

/** Where the CLI's files live in OPFS. */
const OPFS_DIR = 'datum-cli';

/**
 * How long a command waits for the files before failing.
 *
 * Covers both ways of not getting them: another tab's process holding them
 * (the spike saw at most 915 ms of that, eleven tries), and a browser whose
 * `createSyncAccessHandle` simply never settles (da-mskb).
 */
const ACQUIRE_DEADLINE_MS = 5000;

/** The Web Lock a process holds for as long as it has the files. */
const FILES_LOCK = 'datum-cli-files';

/** How soon a tab in memory goes back for the file, and the most it waits. */
const RETRY_FIRST_MS = 2000;
const RETRY_MAX_MS = 30000;

let compiled: Promise<WebAssembly.Module> | null = null;

/** The database, while this tab holds it in memory; null while it is on OPFS. */
let memory: Map<string, Handle> | null = null;

/** The env of the latest command, for the handover's own two commands. */
let lastEnv: string[] = [];

/** Told when storage changes under the page. */
let announce: (storage: StorageKind) => void = () => {};

/** One process at a time per worker, so a command never meets its own tab. */
let queue: Promise<unknown> = Promise.resolve();

/** The newest queued job per key, told when a newer one replaces it. */
const waiting = new Map<string, () => void>();

/**
 * Queue `job`; resolves `null` if it was superseded before it started.
 *
 * A job with a `key` (a pane's identity, da-eoq3) that has not started yet
 * is dropped when a newer job with the same key is queued: only the newest
 * picture of a pane is worth a process. A job without one — every command
 * typed or pressed — is never dropped.
 */
function enqueue<T>(job: () => Promise<T>): Promise<T>;
function enqueue<T>(job: () => Promise<T>, key?: string): Promise<T | null>;
function enqueue<T>(job: () => Promise<T>, key?: string): Promise<T | null> {
	let dropped = false;
	const drop = () => (dropped = true);
	if (key) {
		waiting.get(key)?.();
		waiting.set(key, drop);
	}
	const next = queue.then(() => {
		if (key && waiting.get(key) === drop) waiting.delete(key);
		return dropped ? null : job();
	});
	queue = next.catch(() => {});
	return next;
}

function compile(): Promise<WebAssembly.Module> {
	compiled ??= WebAssembly.compileStreaming(fetch(cliWasm));
	compiled.catch(() => (compiled = null));
	return compiled;
}

/** `promise`, or a rejection once `ms` pass with nothing settled. */
function withDeadline<T>(promise: Promise<T>, ms: number): Promise<T> {
	let timer: ReturnType<typeof setTimeout>;
	const expiry = new Promise<never>((_, reject) => {
		timer = setTimeout(() => reject(new Error(`no sync access handle within ${ms}ms`)), ms);
	});
	return Promise.race([promise, expiry]).finally(() => clearTimeout(timer));
}

/** Open a sync access handle on every file SQLite will touch. */
async function open(): Promise<Map<string, Handle>> {
	const root = await navigator.storage.getDirectory();
	const dir = await root.getDirectoryHandle(OPFS_DIR, { create: true });
	const handles = new Map<string, Handle>();
	try {
		for (const name of DB_FILES) {
			const file = await dir.getFileHandle(name, { create: true });
			// Worker-only, so lib.dom does not declare it.
			const sync = file as unknown as { createSyncAccessHandle(): Promise<Handle> };
			handles.set(name, await sync.createSyncAccessHandle());
		}
	} catch (e) {
		for (const handle of handles.values()) handle.close();
		throw e;
	}
	return handles;
}

/** `open`, retried while another tab's process holds the files. */
async function acquire(): Promise<{ handles: Map<string, Handle>; tries: number }> {
	if (!('createSyncAccessHandle' in FileSystemFileHandle.prototype)) {
		throw new Error('this browser has no sync access handles');
	}
	const deadline = performance.now() + ACQUIRE_DEADLINE_MS;
	for (let tries = 1, wait = 1; ; tries++, wait = Math.min(wait * 2, 50)) {
		const opening = open();
		try {
			return { handles: await withDeadline(opening, deadline - performance.now()), tries };
		} catch (e) {
			// An open that lands after we stopped waiting would hold the files
			// for the worker's life, and every other tab's commands with them.
			opening.then(close, () => {});
			const busy = (e as { name?: string } | null)?.name === 'NoModificationAllowedError';
			if (!busy || performance.now() > deadline) throw e;
			await new Promise((resolve) => setTimeout(resolve, wait));
		}
	}
}

function close(handles: Map<string, Handle>): void {
	for (const handle of handles.values()) handle.close();
}

/** Run `job` with the files: this tab's turn at the lock, then the handles. */
async function withFiles<T>(job: (handles: Map<string, Handle>, tries: number) => Promise<T>) {
	const turn = async () => {
		const { handles, tries } = await acquire();
		try {
			return await job(handles, tries);
		} finally {
			close(handles);
		}
	};
	const locks = typeof navigator === 'undefined' ? undefined : navigator.locks;
	return locks ? (locks.request(FILES_LOCK, turn) as Promise<T>) : turn();
}

/** SQLite's file change counter (header offset 24), bumped by every write. */
function changeCounter(db: Handle): number {
	const bytes = new Uint8Array(4);
	db.read(bytes, { at: 24 });
	return new DataView(bytes.buffer).getUint32(0);
}

/** One process against `handles`. Resolves once it has exited. */
async function exec(
	module: WebAssembly.Module,
	handles: Map<string, Handle>,
	argv: string[],
	env: string[],
	stdin = '',
	onStderr?: (text: string) => void
) {
	const stdout = new Sink();
	const stderr = new Sink(onStderr);
	// Measured per database, since the change counter is per file: a layout
	// saved to `_internal` is no reason to re-run the panes on `datum`.
	// A process that creates a database is not a write of the user's: the
	// first pane to run makes the schema, and that is no reason to ask the
	// browser to keep anything, or to tell the other tabs (da-frg2).
	const dbs = DATABASES.map((name) => ({ name, db: handles.get(`${name}.db`)! }));
	const before = dbs.map(({ db }) => db.getSize() > 0 && changeCounter(db));
	const fds: Fd[] = [
		new OpenFile(new MemFile(new TextEncoder().encode(stdin))),
		stdout,
		stderr,
		opfsDirectory('/data', handles)
	];
	// HOME inside the preopen: datum makes its config dir there, in memory
	// for the length of the process. The database is DATUM_DB — and datum's
	// data dir is the preopen itself, so `--db <name>` lands on
	// `/data/<name>.db` by the CLI's own rule, and `--db _internal` is the
	// other persisted file with nothing here parsing argv for it.
	const vars = ['HOME=/data', 'DATUM_HOME=/data', 'DATUM_DB=/data/datum.db', ...env];
	const args = ['datum', ...argv];
	const wasi = new WASI(args, vars, fds, { debug: false });
	// The shim (0.4.2, the latest) sizes argv in UTF-16 units and writes it
	// as UTF-8, so a non-ASCII argument overran its buffer into the next one:
	// `spec set 日本語 --default-verb occur` arrived as `occuS`. Sized in
	// bytes here, the way it already sizes the environment.
	const argvBytes = args.reduce((n, arg) => n + new TextEncoder().encode(arg).length + 1, 0);
	wasi.wasiImport.args_sizes_get = (count: number, size: number) => {
		const view = new DataView(wasi.inst.exports.memory.buffer);
		view.setUint32(count, args.length, true);
		view.setUint32(size, argvBytes, true);
		return 0;
	};
	const instance = await WebAssembly.instantiate(module, {
		wasi_snapshot_preview1: wasi.wasiImport
	});
	const started = performance.now();
	let code: number;
	try {
		code = wasi.start(instance as Parameters<WASI['start']>[0]);
	} catch (e) {
		// A trap: `panic = "abort"` in the wasm profile, after the panic
		// message reached stderr. 134 is what a shell reports for SIGABRT.
		console.warn('datum trapped:', e);
		code = 134;
	}
	const ran = performance.now();
	for (const handle of handles.values()) handle.flush();
	return {
		code,
		// Unterminated, as every stdout on the seam is.
		stdout: stdout.text().replace(/\n+$/, ''),
		stderr: stderr.text().replace(/\n+$/, ''),
		changed: dbs
			.filter(({ db }, i) => before[i] !== false && changeCounter(db) !== before[i])
			.map(({ name }) => name),
		started,
		ran,
		flushed: performance.now()
	};
}

/**
 * Where the database lives, found by asking for the files once.
 *
 * Also starts compiling the module, so the first command does not pay for
 * it — without holding up the answer, which is what enables the prompt.
 */
export function probe(): Promise<StorageKind> {
	void compile().catch((e) => console.warn('datum-cli.wasm did not compile:', e));
	return enqueue(async () => {
		if (memory) return 'memory';
		try {
			await withFiles(async () => {});
			return 'opfs';
		} catch (e) {
			console.warn('no durable storage — entries are held in memory until there is:', e);
			fallToMemory();
			return 'memory';
		}
	});
}

function fallToMemory(): void {
	memory = new Map(DB_FILES.map((name) => [name, new MemoryHandle()]));
	keepTrying(RETRY_FIRST_MS);
}

/**
 * Go back for the files until we have them, then move what memory holds
 * onto them.
 *
 * The move is two commands of the CLI's own per database — `db backup -` out
 * of memory, `db restore -` into the file — queued like any other, so no
 * command can run against a half-moved pair. Restore is content-hash
 * idempotent, so rows another tab already has are skipped rather than
 * doubled. (It moves the live rows: a layout's older versions, kept in the
 * database's history, stay behind in memory.)
 */
function keepTrying(delay: number): void {
	setTimeout(() => {
		void enqueue(async () => {
			const held = memory;
			if (!held) return;
			try {
				const module = await compile();
				// Memory stays the database until every one has moved: half
				// the data durable is worse than all of it in memory, because
				// nobody can see which half. A retry after a half-done move
				// restores the first again, which idempotence makes harmless.
				const restored = await withFiles(async (handles) => {
					const said: string[] = [];
					for (const db of DATABASES) {
						const backup = await exec(module, held, ['--db', db, 'db', 'backup', '-'], lastEnv);
						if (backup.code !== 0) throw new Error(backup.stderr);
						const restore = await exec(
							module,
							handles,
							['--db', db, 'db', 'restore', '-'],
							lastEnv,
							backup.stdout
						);
						if (restore.code !== 0) throw new Error(restore.stderr);
						said.push(`${db}: ${restore.stdout}`);
					}
					return said.join('; ');
				});
				memory = null;
				console.info(`durable storage recovered: ${restored}`);
				announce('opfs');
			} catch (e) {
				// Still refused, or the file is not ours after all: memory is
				// still the database. Try again later.
				console.warn('durable storage still unavailable:', e);
				keepTrying(Math.min(delay * 2, RETRY_MAX_MS));
			}
		});
	}, delay);
}

/** Where the database lives now, without queueing anything to ask. */
export function storageNow(): StorageKind {
	return memory ? 'memory' : 'opfs';
}

/** Be told when storage changes under the page. */
export function onStorage(listener: (storage: StorageKind) => void): void {
	announce = listener;
}

/**
 * Run `datum <argv>` with `env`; resolves once the process has exited, or
 * `null` if a newer command with the same `key` superseded it first.
 */
export function runCli(
	argv: string[],
	env: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string
): Promise<CliResult | null> {
	lastEnv = env;
	return enqueue(async () => {
		const t0 = performance.now();
		const module = await compile();
		const t1 = performance.now();
		const runIn = async (handles: Map<string, Handle>, tries: number): Promise<CliResult> => {
			const t2 = performance.now();
			const run = await exec(module, handles, argv, env, stdin, onStderr);
			return {
				argv,
				code: run.code,
				stdout: run.stdout,
				stderr: run.stderr,
				changed: run.changed,
				storage: handles === memory ? 'memory' : 'opfs',
				ms: {
					compile: t1 - t0,
					acquire: t2 - t1,
					instantiate: run.started - t2,
					run: run.ran - run.started,
					flush: run.flushed - run.ran,
					tries
				}
			};
		};
		return memory ? runIn(memory, 0) : withFiles(runIn);
	}, key);
}
