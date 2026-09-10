// The real `datum` CLI, one process per command (da-1vat, route C).
//
// Runs inside $lib/db.worker.ts, the only place OPFS sync access handles
// exist. The module is `datum-cli` built for wasm32-wasip1
// (scripts/build-cli-wasm.mjs): compiled once per worker, instantiated
// afresh for every command, with argv, env, stdout and stderr exactly as a
// shell would give them, and `/data` preopened over the OPFS files in
// $lib/wasiFs.ts. What it prints is what the CLI prints; there is nothing
// here that knows what a command means.
//
// Two ways to hold the database files, because which the swap should use is
// what this spike measures:
//
//   owner    the worker that owns the database (da-eovo's election) opens
//            the handles once and keeps them; other tabs forward to it
//   command  every command opens the handles and closes them at exit, so
//            no lock outlives a command and any tab's worker can run one;
//            a command that finds them held waits and tries again

import { File as MemFile, OpenFile, WASI, type Fd } from '@bjorn3/browser_wasi_shim';
import cliWasm from './wasm/datum-cli.wasm?url';
import { DB_FILES, MemoryHandle, opfsDirectory, type Handle } from './wasiFs.js';
import type { CliLock, CliResult, StorageKind } from './dbProtocol.js';

/** Where the CLI's files live in OPFS: beside the SAH pool, not inside it. */
const OPFS_DIR = 'datum-cli';

/** How long a command waits for handles another tab holds before failing. */
const ACQUIRE_DEADLINE_MS = 5000;

let compiled: Promise<WebAssembly.Module> | null = null;
let held: Promise<Map<string, Handle>> | null = null;
let storage: StorageKind = 'opfs';

/** One command at a time per worker: a process owns the handles while it runs. */
let queue: Promise<unknown> = Promise.resolve();

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

/**
 * `open`, retried while another tab holds the files — the command design's
 * contention, and the owner design's handoff from a closing tab. Counts the
 * tries so the measurement can see the contention.
 */
async function acquire(): Promise<{ handles: Map<string, Handle>; tries: number }> {
	const opfs =
		typeof FileSystemFileHandle !== 'undefined' &&
		'createSyncAccessHandle' in FileSystemFileHandle.prototype;
	if (!opfs) {
		storage = 'memory';
		return { handles: new Map(DB_FILES.map((name) => [name, new MemoryHandle()])), tries: 1 };
	}
	const deadline = performance.now() + ACQUIRE_DEADLINE_MS;
	for (let tries = 1, wait = 1; ; tries++, wait = Math.min(wait * 2, 50)) {
		try {
			return { handles: await open(), tries };
		} catch (e) {
			const busy = (e as { name?: string } | null)?.name === 'NoModificationAllowedError';
			if (!busy || performance.now() > deadline) throw e;
			await new Promise((resolve) => setTimeout(resolve, wait));
		}
	}
}

/** SQLite's file change counter (header offset 24), bumped by every write. */
function changeCounter(db: Handle): number {
	const bytes = new Uint8Array(4);
	db.read(bytes, { at: 24 });
	return new DataView(bytes.buffer).getUint32(0);
}

async function runOnce(argv: string[], env: string[], lock: CliLock): Promise<CliResult> {
	const t0 = performance.now();
	compiled ??= WebAssembly.compileStreaming(fetch(cliWasm));
	const module = await compiled;
	const t1 = performance.now();

	let handles: Map<string, Handle>;
	let tries = 1;
	if (lock === 'owner') {
		held ??= acquire().then(
			(acquired) => acquired.handles,
			(e) => {
				held = null;
				throw e;
			}
		);
		handles = await held;
	} else {
		({ handles, tries } = await acquire());
	}
	const t2 = performance.now();

	const decoder = new TextDecoder();
	const stdout = new MemFile([]);
	const stderr = new MemFile([]);
	try {
		const db = handles.get(DB_FILES[0])!;
		const before = changeCounter(db);
		const fds: Fd[] = [
			new OpenFile(new MemFile([])),
			new OpenFile(stdout),
			new OpenFile(stderr),
			opfsDirectory('/data', handles)
		];
		// HOME inside the preopen: datum makes its data and config dirs there,
		// in memory for the length of the process. The database is DATUM_DB.
		const vars = ['HOME=/data', 'DATUM_DB=/data/datum.db', ...env];
		const wasi = new WASI(['datum', ...argv], vars, fds, { debug: false });
		const instance = await WebAssembly.instantiate(module, {
			wasi_snapshot_preview1: wasi.wasiImport
		});
		const t3 = performance.now();
		let code: number;
		try {
			code = wasi.start(instance as Parameters<WASI['start']>[0]);
		} catch (e) {
			// A trap: `panic = "abort"` in the wasm profile, after the panic
			// message reached stderr. 134 is what a shell reports for SIGABRT.
			console.warn('datum trapped:', e);
			code = 134;
		}
		const t4 = performance.now();
		for (const handle of handles.values()) handle.flush();
		const t5 = performance.now();
		return {
			argv,
			code,
			// Unterminated, as every stdout on the seam is (see runners/local.ts).
			stdout: decoder.decode(stdout.data).replace(/\n+$/, ''),
			stderr: decoder.decode(stderr.data).replace(/\n+$/, ''),
			changed: changeCounter(db) !== before,
			storage,
			ms: {
				compile: t1 - t0,
				acquire: t2 - t1,
				instantiate: t3 - t2,
				run: t4 - t3,
				flush: t5 - t4,
				tries
			}
		};
	} finally {
		if (lock === 'command') for (const handle of handles.values()) handle.close();
	}
}

/** Run `datum <argv>` with `env`; resolves once the process has exited. */
export function runCli(argv: string[], env: string[], lock: CliLock): Promise<CliResult> {
	const next = queue.then(() => runOnce(argv, env, lock));
	queue = next.catch(() => {});
	return next;
}
