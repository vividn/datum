// The real `datum` CLI, one instance answering command after command
// (da-1vat, da-g8o8, da-jm4sv).
//
// Runs inside #lib/db.worker.ts, the only place OPFS sync access handles
// exist. The module is `datum-cli` built for wasm32-wasip1 as a reactor
// (scripts/build-cli-wasm.mjs): compiled once per worker and instantiated
// once (#lib/instance.ts), each command given argv, env, stdin, stdout and
// stderr exactly as a shell would give a process, and `/data` preopened over
// the database files in #lib/wasiFs.ts. What it prints is what the CLI
// prints; there is nothing here that knows what a command means. The
// instance keeps each database's connection open between commands, so one
// costs its query and not an instantiate and two opens (~10 ms, da-zv6hn);
// one that traps or grows too big ends, and the next command starts another.
//
// EVERY BURST TAKES THE FILES FOR ITSELF. A sync access handle on each
// database file is opened before a process starts, so nothing outside this
// worker is held while the page is idle and every tab runs its own commands
// — there is no owner tab to elect, forward to, or lose. The spike measured
// both this and an owner-tab design; this one was faster under two tabs
// (26.6 ms median against 101) and cannot run a command twice, where the
// owner's resend could (da-1vat).
//
// Back-to-back commands SHARE one acquisition (da-dvzx): while the queue
// still has jobs waiting, the handles and the lock are kept and the next job
// runs on them, because opening four handles costs about as much as a warm
// command and a burst paid it per command. Two bounds keep that from
// starving another tab: they are let go the moment the queue drains, and a
// burst that never pauses lets go anyway once it has held them past
// `BURST_HOLD_MS`.
//
// Tabs take turns through a Web Lock, held for as long as the handles are.
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

import { File as MemFile, OpenFile, type Fd, type PreopenDirectory } from './vendor/browser_wasi_shim/index.js';
import { Instance } from './instance.js';
import cliWasm from './wasm/datum-cli.wasm?url';
import { cached, DATABASES, dbFiles, forgetCached, KEPT, MemoryHandle, opfsDirectory, Sink, staged, unkept, type Handle } from './wasiFs.js';
import { backupArgv, mergeArgv } from './calls.js';
import { dbFlag } from './dbFlag.js';
import { OPFS_DIR } from './databases.js';
import { FROZEN_AT, type CliResult, type Stage, type StorageKind } from './dbProtocol.js';

/**
 * How long a command waits for the files before failing.
 *
 * Covers both ways of not getting them: another tab's process holding them
 * (the spike saw at most 915 ms of that, eleven tries), and a browser whose
 * `createSyncAccessHandle` simply never settles (da-mskb).
 */
const ACQUIRE_DEADLINE_MS = 5000;

/** The Web Lock this worker holds for as long as it has the files. */
const FILES_LOCK = 'datum-cli-files';

/**
 * The longest a burst may keep the files before letting another tab in.
 *
 * The queue draining is what usually gives them up; this is the bound for a
 * burst that never drains — a hand on the widget bar, a pane following a
 * cursor — where otherwise a second tab would wait out the whole of it.
 *
 * 250 ms, because the lock is first come, first served: this IS the other
 * tab's worst added wait, and it is no longer than the page takes to answer
 * a keypress. It stays well inside `ACQUIRE_DEADLINE_MS`, and at a warm
 * command of ~21 ms (the spike) it amortizes one acquisition over about ten
 * commands.
 *
 * Measured against the alternative of giving the files up the moment
 * `locks.query()` shows another tab waiting: that cut the median acquire
 * under two tabs from 88 ms to 17 ms, but the 40 writes of the spike's
 * two-tab race then took 5.3 s — what holding nothing at all takes, since a
 * contended burst never gets to share — against 4.0 s for this cap. Its
 * ~110 ms median acquire is not starvation but a fair turn taken in
 * batches: 200 ms was the worst wait seen, inside this bound.
 */
const BURST_HOLD_MS = 250;

/**
 * How long a wait for the lock is a fair turn. Past it another tab is
 * holding the files — a long process, or a tab the system froze mid-burst
 * — and the page says so rather than sit on «opening database…» (da-89kom).
 */
const BLOCKED_MS = 1000;

/** How soon a tab in memory goes back for the file, and the most it waits. */
const RETRY_FIRST_MS = 2000;
const RETRY_MAX_MS = 30000;

let compiled: Promise<WebAssembly.Module> | null = null;

/** The database, while this tab holds it in memory; null while it is on OPFS. */
let memory: Map<string, Handle> | null = null;

/**
 * The databases this worker keeps files for: `datum` and `_internal`, plus
 * every name a command has since asked for (da-vk1x).
 *
 * A page on `?db=other` sets `DATUM_DB=other` at the seam (#lib/run.ts), and
 * without this the CLI would make `/data/other.db` inside the process's own
 * in-memory filesystem and lose it at exit — the entry would be stored and
 * gone. So the worker reads the name out of argv and env the way the CLI does and
 * opens a real file for it: a database is persisted because a command named
 * it, not because this file knew about it.
 *
 * It only ever grows, and a name is added at most once. A tab that moves
 * between databases without reloading (the query changes under a client-side
 * navigation) therefore keeps the file of the one it left, which is exactly
 * what another tab of the app would have open anyway.
 */
let databases: readonly string[] = DATABASES;

/**
 * The databases renamed away (`db rename`, da-u2jjc), by any tab: each leaves
 * `<name>.moved` beside the others, the time it moved, written under the
 * files' lock. For `MOVED_MS` after, no worker makes the old name again, one
 * that never named it included, and a command on it is refused. Then it is a
 * name like any, which a tab that was on it still leaves (#lib/changes.ts).
 * `datum` and `_internal` never move: a command that names none runs on the default.
 */
const gone = new Set<string>();
const MOVED_MS = 30_000;
const movedFile = (db: string) => `${db}.moved`;

/**
 * Follow the databases `moved` (`db rename`, da-rktt6): the new name is kept
 * on files of its own from now on, and the old, emptied, is forgotten and its
 * files in OPFS removed, so the logo's menu no longer offers it. The default
 * and `_internal` stay, since a command that names none runs on the default.
 */
async function follow(moved: [string, string][], handles: Map<string, Handle>): Promise<void> {
	for (const [from, to] of moved) {
		if (!databases.includes(to)) databases = [...databases, to];
		if (DATABASES.includes(from)) continue;
		databases = databases.filter((db) => db !== from);
		gone.add(from);
		if (handles === memory) continue;
		const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle(OPFS_DIR);
		for (const file of dbFiles([from])) {
			handles.get(file)?.close();
			handles.delete(file);
			await dir.removeEntry(file).catch(() => {});
		}
		const file = await dir.getFileHandle(movedFile(from), { create: true });
		const marker = await (file as unknown as { createSyncAccessHandle(): Promise<Handle> }).createSyncAccessHandle();
		marker.truncate(0);
		marker.write(new TextEncoder().encode(String(Date.now())), { at: 0 });
		marker.close();
		await dir.removeEntry(movedFile(to)).catch(() => {});
	}
}

/**
 * The database `argv` and `env` name, if they name one by NAME.
 *
 * The CLI's own resolution, in TypeScript: the `--db` flag (#lib/dbFlag.ts),
 * else `DATUM_DB`, and a value that is a name rather than a path
 * (`extract_global_db_flag`, `locate` and `looks_like_path`,
 * `crates/datum-commands/src/config.rs`). A path is left to the process's
 * own filesystem, as it always has been.
 */
function dbNamed(argv: string[], env: string[]): string | undefined {
	const flag = dbFlag(argv);
	const value = flag ? flag.db : env.findLast((v) => v.startsWith('DATUM_DB='))?.slice(9);
	if (!value || value.includes('/') || value.startsWith('.') || value.endsWith('.db')) return undefined;
	return value;
}

/**
 * Note that `argv` with `env` runs against its database, so this worker opens a file
 * for it. The files a burst is holding are the wrong set once the list has
 * grown; `withFiles` gives them back and takes the new set on the next
 * command, which is the only moment no process is running.
 */
function useDatabase(argv: string[], env: string[]): void {
	const name = dbNamed(argv, env);
	if (name === undefined || databases.includes(name)) return;
	databases = [...databases, name];
	// In memory, the new database is another pair of handles in the same map
	// — there is nothing to reacquire, and the move to OPFS carries it too.
	if (memory) for (const file of dbFiles([name])) memory.set(file, new MemoryHandle());
}

/** The env of the latest command, for the handover's own two commands. */
let lastEnv: string[] = [];

/** Told when storage changes under the page. */
let announce: (storage: StorageKind) => void = () => {};

/** Told how long another tab has kept this one waiting for the files, and `null` when it lets go. */
let blocked: (waited: number | null) => void = () => {};

/** A queued job, and what tells its caller it was dropped unrun. */
interface Job {
	/** A pane's identity: a newer job with the same key drops this one. */
	key?: string;
	/** A pane's refresh: behind every other job, cheapest first. */
	pane: boolean;
	/** A sync session's run: behind the panes too, and ahead of them while the page is hidden. */
	session: boolean;
	/** The command word, what the job's cost is measured under. */
	word: string;
	/** Never rejects: a failed job's rejection is handed to its caller. */
	start: () => Promise<void>;
	drop: () => void;
}

/**
 * The jobs not yet started. One process at a time per worker, so a command
 * never meets its own tab; while any job waits, the files are kept.
 */
const jobs: Job[] = [];

/** Whether a job is running, so the next is picked when it finishes. */
let busy = false;

/** How long each pane's command word last took, in ms. */
const cost = new Map<string, number>();

/**
 * Where `job` stands in the line (da-icmo1): everything but a pane's refresh
 * — a line typed, a button pressed, the bar's own reads, a save — first, in
 * the order sent, so a read still follows the write before it; then the
 * panes, cheapest first by their command's last run. A command never run
 * counts as cheap. First in, first out had nowview's half second answer
 * 12.9 s after a write, behind `stats`, and a typed `occur` 13 s after its
 * Enter. vividn: «This whole app must feel very snappy».
 *
 * A sync session's run comes last (da-xzty6): its next round was always in
 * line before the panes its last round set off, so they waited out the whole
 * sync — the tail under its dimmed copy from the last visit, a nowview a tap
 * asked for dimmed until it ended. So each round lets the panes draw what it
 * brought, and what the reader asks goes before the next.
 */
const rank = (job: Job) => (job.session ? (hidden ? -0.5 : Infinity) : job.pane ? (cost.get(job.word) ?? 0) : -1);

/**
 * Whether the page is hidden (da-tavhb). A phone's reader taps and leaves
 * the app at once (vividn: «It would be amazing if that data was already
 * broadcast before the interface is closed again»), and the browser may stop
 * a hidden page's workers at any moment. So while hidden a pane's refresh
 * waits for the page to show again, as nobody sees a redraw, and a sync
 * session's run goes next after the commands typed or pressed: not ahead of
 * them, since the tap's own write, queued before the page hid, is what the
 * push is for (vremya on #1598).
 */
let hidden = false;

/** Whether `job` may run now: a hidden page's panes wait. */
const ready = (job: Job) => !(hidden && job.pane);

/** Say whether the page is hidden; shown again, the panes held run. */
export function setHidden(now: boolean): void {
	hidden = now;
	if (!now) void pump();
}

/**
 * `argv`'s command word: the first that is neither a flag nor `--db`'s value.
 * Only a pane's is ranked, and a pane's argv starts with its command; `--db`
 * is the one flag a caller puts before it. A batch of days' data layers
 * (da-0e06p, da-m6i2a) is not a band's picture: a cost of its own.
 */
const wordOf = (argv: string[]) =>
	(argv.find((word, i) => !word.startsWith('-') && argv[i - 1] !== '--db') ?? '') +
	(argv.includes('--data-layer') ? ' --data-layer' : '');

/**
 * Queue `job`; resolves `null` if it was superseded before it started.
 *
 * A job with a `key` (a pane's identity, da-eoq3) that has not started yet
 * is dropped when a newer job with the same key is queued: only the newest
 * picture of a pane is worth a process. A job without one — every command
 * typed or pressed — is never dropped.
 */
function enqueue<T>(job: () => Promise<T>): Promise<T>;
function enqueue<T>(job: () => Promise<T>, argv: string[], pane?: boolean, key?: string, session?: boolean): Promise<T | null>;
function enqueue<T>(job: () => Promise<T>, argv: string[] = [], pane = false, key?: string, session = false): Promise<T | null> {
	return new Promise<T | null>((resolve, reject) => {
		const stale = key === undefined ? -1 : jobs.findIndex((waiting) => waiting.key === key);
		if (stale >= 0) jobs.splice(stale, 1)[0].drop();
		jobs.push({ key, pane, session, word: wordOf(argv), drop: () => resolve(null), start: () => job().then(resolve, reject) });
		void pump();
	});
}

/** A task boundary: a request that arrived meanwhile is queued before the next pick. */
const nextTask = () =>
	new Promise<void>((resolve) => {
		const { port1, port2 } = new MessageChannel();
		port1.onmessage = () => resolve();
		port2.postMessage(null);
	});

/**
 * Run jobs, the best ranked first and the earliest among equals, until none
 * that may run wait. A job can end in a microtask and the next start in the same one,
 * so the queue crosses a task between jobs: a line typed while one ran is
 * otherwise not queued until a burst of them yields.
 */
async function pump(): Promise<void> {
	if (busy) return;
	busy = true;
	for (;;) {
		// -1 until a job that may run is seen; then the best ranked, the earliest among equals.
		const at = jobs.reduce((best, job, i) => (ready(job) && (best < 0 || rank(job) < rank(jobs[best])) ? i : best), -1);
		if (at < 0) break;
		const [job] = jobs.splice(at, 1);
		const t0 = performance.now();
		await job.start();
		if (job.pane) cost.set(job.word, performance.now() - t0);
		// Drained: nothing that may run is behind this job, so the files go
		// back rather than be held across an idle page, or a hidden one.
		if (!jobs.some(ready)) release();
		else await nextTask();
	}
	// The page may have been hidden across that task, leaving only held panes.
	release();
	busy = false;
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
	gone.clear();
	for (const db of databases.filter((db) => !DATABASES.includes(db))) {
		const here = await dir.getFileHandle(`${db}.db`).then(
			() => true,
			(e: { name?: string }) => {
				if (e?.name !== 'NotFoundError') throw e;
				return false;
			}
		);
		if (here) continue;
		const marker = await dir.getFileHandle(movedFile(db)).catch(() => null);
		if (!marker) continue;
		if (Date.now() - Number(await (await marker.getFile()).text()) < MOVED_MS) gone.add(db);
		else await dir.removeEntry(movedFile(db)).catch(() => {});
	}
	databases = databases.filter((db) => !gone.has(db));
	const opening = databases;
	try {
		for (const name of dbFiles(opening)) {
			const file = await dir.getFileHandle(name, { create: true });
			// Worker-only, so lib.dom does not declare it.
			const sync = file as unknown as { createSyncAccessHandle(): Promise<Handle> };
			const handle = await sync.createSyncAccessHandle();
			// The journal is read only to roll it back: there is nothing to keep.
			handles.set(name, name.endsWith('.db') ? cached(name, handle) : handle);
		}
		// The kept backups (`KEPT`), so `db restore` reads one and the CLI
		// sees the pre-migration ones to keep one per state and prune the
		// oldest; one it pruned was truncated, and
		// goes now (da-hse93) — once the listing is over, which a removal
		// could otherwise cut short.
		const pruned: string[] = [];
		for await (const name of (dir as unknown as { keys(): AsyncIterable<string> }).keys()) {
			if (!KEPT.test(name)) continue;
			const file = await dir.getFileHandle(name);
			const handle = await (file as unknown as { createSyncAccessHandle(): Promise<Handle> }).createSyncAccessHandle();
			if (handle.getSize() > 0) {
				handles.set(name, handle);
				continue;
			}
			handle.close();
			pruned.push(name);
		}
		for (const name of pruned) await dir.removeEntry(name);
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

/** The files this worker is holding, and what gives them back. */
interface Files {
	handles: Map<string, Handle>;
	/** The opens the acquisition took — for the command that paid for it. */
	tries: number;
	/** When they were taken, for `BURST_HOLD_MS`. */
	since: number;
	release: () => void;
}

/** The files, while a burst holds them; null while this worker has none. */
let files: Files | null = null;

/**
 * Take this tab's turn at the lock, then the handles, and keep both until
 * the returned `release`.
 *
 * The lock is held by staying inside its callback — the shape the Web Locks
 * API gives for a lock outliving one await — so the handles are closed and
 * the lock freed in that order, with no window where another tab holds the
 * lock and ours still has the files.
 */
async function hold(): Promise<Files> {
	const locks = typeof navigator === 'undefined' ? undefined : navigator.locks;
	if (!locks) {
		const { handles, tries } = await acquire();
		return { handles, tries, since: performance.now(), release: () => close(handles) };
	}
	// Named apart from `release` below, which is this module's own: this one
	// is the handle back INTO the lock's callback.
	let give!: () => void;
	const given = new Promise<void>((resolve) => (give = resolve));
	let said = false;
	const asked = performance.now();
	const waiting = setTimeout(() => {
		said = true;
		blocked(performance.now() - asked);
	}, BLOCKED_MS);
	const over = () => {
		clearTimeout(waiting);
		if (said) blocked(null);
		said = false;
	};
	return new Promise<Files>((resolve, reject) => {
		locks
			.request(FILES_LOCK, async () => {
				over();
				let got: { handles: Map<string, Handle>; tries: number };
				try {
					got = await acquire();
				} catch (e) {
					reject(e);
					return;
				}
				resolve({ ...got, since: performance.now(), release: give });
				try {
					await given;
				} finally {
					close(got.handles);
				}
			})
			.catch((e) => {
				over();
				reject(e);
			});
	});
}

/** Give the files back, if this worker is holding any. */
function release(): void {
	files?.release();
	files = null;
}

/** Run `job` with the files — a burst's, if one is holding them. */
async function withFiles<T>(job: (handles: Map<string, Handle>, tries: number) => Promise<T>) {
	// Past the cap, let go before asking again: the lock is first come, first
	// served, so the tab behind us is served here rather than at the end of a
	// burst that never pauses.
	//
	// And let go of a set that does not cover every database a command has
	// named, so the next acquisition opens the new file. Asked of the HANDLES
	// and not of a list captured when the hold resolved (vremya on #22): the
	// captured list was a 1–30 ms hole. `open()` builds `dbFiles(databases)`
	// once and then awaits per file, so a first-time `--db <name>` arriving
	// between those awaits grew `databases` AFTER the files were chosen — and
	// the grown reference is what `hold()` then recorded, so this test saw a
	// set that matched and the command ran with no `<name>.db` handle: the CLI
	// made the database in the process's own memory filesystem, exited 0, and
	// the row evaporated. The handles cannot lie about what they are.
	const covers = (held: Files) => databases.every((db) => held.handles.has(`${db}.db`));
	if (files && (performance.now() - files.since > BURST_HOLD_MS || !covers(files))) release();
	const held = (files ??= await hold());
	// The opens are the acquiring command's; every command after it on the
	// same handles opened nothing, and says so.
	const tries = held.tries;
	held.tries = 0;
	return job(held.handles, tries);
}

/** SQLite's file change counter (header offset 24), bumped by every write. */
function changeCounter(db: Handle): number {
	const bytes = new Uint8Array(4);
	db.read(bytes, { at: 24 });
	return new DataView(bytes.buffer).getUint32(0);
}

/**
 * The instance that runs this worker's commands (#lib/instance.ts, da-jm4sv),
 * once one has: started at the first, and again after one that ended it.
 */
let live: Instance | null = null;

/**
 * The most memory an instance is kept with after a command. wasm memory
 * never shrinks, so one command's peak would be the instance's for its life:
 * past this the next command starts another. Twice SQLite's page cache
 * (`cache_size`, 64 MiB, `Db::configure_connection`), so a cache that has
 * filled is kept, and one command that held far more than that is not.
 */
const KEEP_MEMORY = 128 << 20;

/**
 * One command against `handles`, on the worker's instance; `fresh`, on an
 * instance of its own, as a process of its own was. Resolves once it has
 * run.
 */
async function exec(
	module: WebAssembly.Module,
	handles: Map<string, Handle>,
	argv: string[],
	env: string[],
	stdin = '',
	onStderr?: (text: string) => void,
	stage?: Stage,
	at?: number,
	fresh = false
) {
	const stdout = new Sink();
	const stderr = new Sink(onStderr);
	// Measured per database, since the change counter is per file: a layout
	// saved to `_internal` is no reason to re-run the panes on `datum`.
	// A process that creates a database is not a write of the user's: the
	// first pane to run makes the schema, and that is no reason to ask the
	// browser to keep anything, or to tell the other tabs (da-frg2).
	const dbs = databases
		.map((name) => ({ name, db: handles.get(`${name}.db`) }))
		.filter((pair): pair is { name: string; db: Handle } => pair.db !== undefined);
	const before = dbs.map(({ db }) => db.getSize() > 0 && changeCounter(db));
	const cli = (!fresh && live) || (await Instance.start(module));
	const data = opfsDirectory('/data', handles, stage, cli.inodes);
	const fds: [Fd, Fd, Fd, Fd] = [new OpenFile(new MemFile(new TextEncoder().encode(stdin))), stdout, stderr, data];
	// HOME inside the preopen: datum makes its config dir there, in memory
	// for the length of the command. The database is DATUM_DB, unless `env`
	// names one (the page's, #lib/run.ts `dbEnv`) — and datum's
	// data dir is the preopen itself, so `--db <name>` lands on
	// `/data/<name>.db` by the CLI's own rule, and `--db _internal` is the
	// other persisted file with nothing here parsing argv for it. Those
	// files' connections are kept for the next command (`DATUM_KEEP_OPEN`):
	// a path the CLI is given is a file of this command's alone.
	const fallback = env.some((v) => v.startsWith('DATUM_DB=')) ? [] : ['DATUM_DB=/data/datum.db'];
	const kept = fresh ? [] : [`DATUM_KEEP_OPEN=${dbs.map(({ name }) => `/data/${name}.db`).join(',')}`];
	const vars = ['HOME=/data', 'DATUM_HOME=/data', ...fallback, ...env, ...kept];
	const started = performance.now();
	const { code, dead, trapped } = await cli.run(['datum', ...argv], vars, fds, at);
	if (trapped) forgetCached();
	// What a command moved or let go of in `/data` is no file a connection
	// kept open may still point into: the next starts afresh.
	if (!fresh) live = dead || cli.memory > KEEP_MEMORY || data.left.size > 0 || data.moved.length > 0 ? null : cli;
	const ran = performance.now();
	for (const handle of handles.values()) handle.flush();
	// In memory, what OPFS would keep is kept in the map.
	if (handles === memory) for (const [file, handle] of unkept(data)) handles.set(file, handle);
	const unsaved = handles === memory ? [] : await keep(handles, data);
	// A rename is done once what it moved is all kept: then what it left is
	// emptied. Otherwise the copies are, and it is no rename (da-rktt6).
	const undone = data.left.size > 0 && unsaved.length > 0;
	if (undone) for (const [, to] of data.moved) handles.get(`${to}.db`)?.truncate(0);
	else for (const handle of data.left) handle.truncate(0);
	const moved = undone ? [] : data.moved;
	return {
		code: undone ? code || 1 : code,
		// Unterminated, as every stdout on the seam is.
		stdout: stdout.text().replace(/\n+$/, ''),
		stderr: [stderr.text().replace(/\n+$/, ''), ...unsaved].filter(Boolean).join('\n'),
		stage: stage && staged(data),
		// A database renamed away is gone, not changed: a reader of it would make it again.
		changed: dbs
			.filter(({ db }, i) => before[i] !== false && changeCounter(db) !== before[i])
			.map(({ name }) => name)
			.filter((name) => !moved.some(([from]) => from === name)),
		moved,
		started,
		ran,
		flushed: performance.now()
	};
}

/**
 * Write every backup the process left in memory (`KEPT`) to OPFS,
 * beside the database, and hold it with the burst's other files (da-hse93).
 * The CLI has already said it was saved, so one that cannot be is told on
 * stderr, as the process's own warning would be.
 */
async function keep(handles: Map<string, Handle>, data: PreopenDirectory): Promise<string[]> {
	const said: string[] = [];
	const backups = unkept(data);
	if (!backups.length) return said;
	const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle(OPFS_DIR);
	for (const [name, backup] of backups) {
		let handle: Handle | undefined;
		try {
			const file = await dir.getFileHandle(name, { create: true });
			handle = await (file as unknown as { createSyncAccessHandle(): Promise<Handle> }).createSyncAccessHandle();
			const bytes = new Uint8Array(backup.getSize());
			backup.read(bytes, { at: 0 });
			if (handle.write(bytes, { at: 0 }) !== bytes.length) throw new Error('short write');
			handle.flush();
			handles.set(name, handle);
		} catch (e) {
			handle?.close();
			await dir.removeEntry(name).catch(() => {});
			said.push(
				name.endsWith('.db')
					? `error: the rename to ${name.slice(0, -3)} was not kept, and nothing moved: ${e}`
					: `warning: the backup ${name} could not be kept: ${e}`
			);
		}
	}
	return said;
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
	live = null;
	memory = new Map(dbFiles(databases).map((name) => [name, new MemoryHandle()]));
	keepTrying(RETRY_FIRST_MS);
}

/**
 * Go back for the files until we have them, then move what memory holds
 * onto them.
 *
 * The move is two commands of the CLI's own per database — `db backup -` out
 * of memory, `db restore - --merge` into the file — queued like any other,
 * so no command can run against a half-moved pair. A merge adds and deletes
 * nothing, and is content-hash idempotent, so rows another tab already has
 * are skipped rather than doubled and rows only the file has stay. (It moves the live rows: a layout's older versions, kept in the
 * database's history, stay behind in memory.)
 */
function keepTrying(delay: number): void {
	setTimeout(() => {
		void enqueue(async () => {
			const held = memory;
			if (!held) return;
			try {
				const module = await compile();
				// Each on an instance of its own: these are two sets of files under one name.
				const alone = (handles: Map<string, Handle>, argv: string[], stdin = '') =>
					exec(module, handles, argv, lastEnv, stdin, undefined, undefined, undefined, true);
				// Memory stays the database until every one has moved: half
				// the data durable is worse than all of it in memory, because
				// nobody can see which half. A retry after a half-done move
				// restores the first again, which idempotence makes harmless.
				const restored = await withFiles(async (handles) => {
					const said: string[] = [];
					for (const db of databases) {
						const backup = await alone(held, ['--db', db, ...backupArgv()]);
						if (backup.code !== 0) throw new Error(backup.stderr);
						const restore = await alone(handles, ['--db', db, ...mergeArgv()], backup.stdout);
						if (restore.code !== 0) throw new Error(restore.stderr);
						said.push(`${db}: ${restore.stdout}`);
					}
					return said.join('; ');
				});
				memory = null;
				live = null;
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

/** Be told when another tab keeps this one waiting for the files past `BLOCKED_MS`, and when it lets go. */
export function onBlocked(listener: (waited: number | null) => void): void {
	blocked = listener;
}

/**
 * Run `datum <argv>` with `env`; resolves once it has run, or
 * `null` if a newer command with the same `key` superseded it first.
 * `pane`: a pane's refresh, which waits behind every other command;
 * `session`: a sync session's run, which waits behind the panes too, and
 * ahead of them while the page is hidden (`setHidden`).
 */
export function runCli(
	argv: string[],
	env: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string,
	stage?: Stage,
	pane = false,
	session = false
): Promise<CliResult | null> {
	// The gesture's instant is this command's alone, not the handover's.
	const frozen = env.find((v) => v.startsWith(FROZEN_AT));
	const at = frozen ? Number(frozen.slice(FROZEN_AT.length)) : undefined;
	env = env.filter((v) => v !== frozen);
	lastEnv = env;
	useDatabase(argv, env);
	return enqueue(async () => {
		const t0 = performance.now();
		const module = await compile();
		const t1 = performance.now();
		const runIn = async (handles: Map<string, Handle>, tries: number): Promise<CliResult> => {
			const t2 = performance.now();
			const named = dbNamed(argv, env);
			if (named && gone.has(named)) {
				const ms = { compile: t1 - t0, acquire: t2 - t1, instantiate: 0, run: 0, flush: 0, tries };
				const stderr = `error: the database ${named} was renamed by another tab: nothing ran`;
				return { argv, code: 1, stdout: '', stderr, changed: [], storage: handles === memory ? 'memory' : 'opfs', ms };
			}
			const run = await exec(module, handles, argv, env, stdin, onStderr, stage, at);
			if (run.moved.length) {
				await follow(run.moved, handles);
				// The next command opens the files of the databases as they are now.
				if (handles !== memory) release();
			}
			return {
				argv,
				code: run.code,
				stdout: run.stdout,
				stderr: run.stderr,
				changed: run.changed,
				...(run.moved.length && { moved: run.moved }),
				...(run.stage && { stage: run.stage }),
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
	}, argv, pane, key, session);
}
