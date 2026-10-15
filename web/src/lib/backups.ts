// Backup PLANS (da-9ow8m): a Tower of Hanoi rotation per database, kept
// at a destination and taken while a datum.day tab is open.
//
// vividn, 2026-09-25: «create cloud backup menu that let's the user create
// automated tower of hanoi backups … configure frequency of backups (maybe
// every hour as a default?) and max slots (set to none …)». The rotation is
// the CLI's own (`db backup --to DIR --every D [--slots N]`, da-d5cmj),
// and so is every number the widget shows: the ages (`--list`), the reach
// (`--plan`), the size of one backup, what a restore would do (`db restore
// --dry-run`). Nothing here knows which slot a period lands in.
//
// THE DESTINATION IS THE ROTATION. The CLI keeps it in a directory, and a
// process's directory is its stage (`Stage`, $lib/dbProtocol.ts), which
// lives for that one process: the destination's `index.json` goes in, the
// CLI writes the period's slot file and a new index beside it, and exactly
// the files it names as written are put back, the slot first and the index
// last — the order the CLI itself keeps, so a run that dies halfway leaves
// an index naming whole files — and the file it replaced is removed. A 2xx
// is not taken for a whole file (da-5okjx): each slot file's length at the
// destination is asked back and must be the length put, or the run fails
// there, before the index names it and before anything is removed.
//
// A destination is three calls (`Destination`). The real kinds are an
// S3-compatible bucket (da-pstzw, $lib/s3.ts) and a WebDAV folder
// (da-bmxbb, $lib/webdav.ts); `memory`, a bucket in this
// tab, is offered on the DEBUG database alone (vremya on #217): a plan
// survives a reload and a tab's bucket does not, so offered anywhere else
// it would look like protection and hold nothing. NOT ENCRYPTED in v1
// (vividn, 12:03): the widget says so.
//
// TWO WRITERS, ONE INDEX. `index.json` is read, rotated and written back,
// so two devices backing up to one bucket could each write an index that
// lacks the other's slot — naming a file the other removed. So the index
// is read AGAIN just before it is put, and this run's slot merged into
// what is there now (`merged`). That leaves the time between the re-read
// and the put, which two TABS of one device are kept out of by a Web Lock
// (`alone`, da-u050c); two devices still share it.
//
// Plans are THIS DEVICE'S settings — a plan names a destination and its
// credentials — so they are entries of field `backup_plan` in `_internal`
// (slug `backup_plan:<name>`), the whole plan as `-k plan=<json>`.

import { writable, get } from 'svelte/store';
import {
	backupListArgv,
	backupSizeArgv,
	backupToArgv,
	deletePlanArgv,
	planReachArgv,
	plansArgv,
	restorePreviewArgv,
	restoreStagedArgv,
	savePlanArgv
} from './calls.js';
import { confirm } from './confirm.js';
import type { RestorePreview } from './dbio.js';
import type { Stage } from './dbProtocol.js';
import { DEBUG_DB } from './layout/defaults.js';
import { cliError, run, type RunResult } from './run.js';
import type { StepResult } from './probe.js';
import { corsRules, DEFAULT_PREFIX, s3, testConnection } from './s3.js';
import { webdav, corsHeaders as webdavCors, testConnection as testWebdav } from './webdav.js';

/** Where a plan's files go. Names are flat: `index.json` and the slot files. */
export interface Destination {
	/** The file's bytes, or null when there is no such file. */
	get(file: string): Promise<Uint8Array | null>;
	put(file: string, bytes: Uint8Array): Promise<void>;
	remove(file: string): Promise<void>;
	/** The file's length as the destination holds it (a HEAD's `Content-Length`), or null when it does not say. */
	size(file: string): Promise<number | null>;
}

/** One setting of a destination, as the widget asks for it. */
export interface Field {
	key: string;
	label: string;
	/** The value a new plan starts with. */
	value?: string;
	placeholder?: string;
	/** Drawn as a password, with `note` beside it. */
	secret?: boolean;
	note?: string;
}

/** A kind of destination: what the widget calls it, and how a plan's config opens one. */
export interface DestinationKind {
	label: string;
	/** The config's fields, in the order the widget asks for them. */
	fields: Field[];
	/** Offered on the DEBUG database only: a fake, for trying the widget and for tests. */
	debug?: boolean;
	open(config: Record<string, string>): Destination;
	/** Write, read and delete a probe — and list it, where the kind lists — naming the step that fails. */
	test?(config: Record<string, string>): Promise<StepResult[]>;
	/** What the destination needs to allow this page: a summary, how, and the rules for `origin` to paste. */
	cors?: { summary: string; note: string; rules(origin: string): string };
}

/** Buckets of the `memory` kind, by name — gone with the tab. */
const buckets = new Map<string, Map<string, Uint8Array>>();

export const KINDS: Record<string, DestinationKind> = {
	s3: {
		label: 'S3-compatible bucket (AWS, R2, B2, Wasabi, MinIO)',
		fields: [
			{ key: 'endpoint', label: 'Endpoint URL', placeholder: 'blank for AWS; https://<account>.r2.cloudflarestorage.com' },
			{ key: 'bucket', label: 'Bucket' },
			{ key: 'region', label: 'Region', placeholder: 'us-east-1; auto for R2' },
			{ key: 'prefix', label: 'Path prefix', value: DEFAULT_PREFIX },
			{ key: 'key', label: 'Access key id' },
			{
				key: 'secret',
				label: 'Secret',
				secret: true,
				note: 'Kept in this browser, unencrypted. Use a key scoped to this one bucket, allowed only to read, write, list and delete in it.'
			}
		],
		open: s3,
		test: testConnection,
		cors: {
			summary: 'CORS: the bucket must allow this page',
			note:
				"The browser talks to the bucket directly. Paste these rules into the bucket's CORS settings (S3: Permissions → CORS; " +
				'R2: Settings → CORS policy; B2, Wasabi and MinIO take them through their S3 API). Listing is a GET on the bucket.',
			rules: corsRules
		}
	},
	webdav: {
		label: 'WebDAV folder (Nextcloud, ownCloud, many NAS boxes)',
		fields: [
			{ key: 'url', label: 'Folder URL', placeholder: 'https://cloud.example.com/remote.php/dav/files/<user>/datum-backups/' },
			{ key: 'user', label: 'User' },
			{
				key: 'password',
				label: 'App password',
				secret: true,
				note: "Kept in this browser, unencrypted. Use an app password made for datum.day, not the account's own."
			}
		],
		open: webdav,
		test: testWebdav,
		cors: {
			summary: 'CORS: the server must allow this page',
			note:
				'The browser talks to the server directly, and most WebDAV servers do not allow that until told to. Have the web ' +
				'server in front of it send these headers on every response, refusals included (nginx: add_header … always), and ' +
				'answer the OPTIONS preflight itself, without asking for a password.',
			rules: webdavCors
		}
	},
	memory: {
		label: 'memory (this tab only, for trying it out)',
		fields: [{ key: 'bucket', label: 'bucket' }],
		debug: true,
		open({ bucket = '' }) {
			const files = buckets.get(bucket) ?? buckets.set(bucket, new Map()).get(bucket)!;
			return {
				get: async (file) => files.get(file) ?? null,
				put: async (file, bytes) => void files.set(file, bytes),
				remove: async (file) => void files.delete(file),
				size: async (file) => files.get(file)?.length ?? null
			};
		}
	}
};

/** The kinds a new plan may use on database `db`. */
export const offered = (db: string): string[] =>
	Object.keys(KINDS).filter((kind) => !KINDS[kind].debug || db === DEBUG_DB);

export interface Plan {
	name: string;
	kind: string;
	config: Record<string, string>;
	/** The databases it covers, by name. */
	dbs: string[];
	/** Seconds between backups. */
	every: number;
	/** The cap on rotating slots, slot 0 not counted; null grows with time. */
	slots: number | null;
}

export const HOUR = 3600;

/** A kind's config as a new plan starts it. */
export const defaults = (kind: string): Record<string, string> =>
	Object.fromEntries((KINDS[kind]?.fields ?? []).flatMap((f) => (f.value ? [[f.key, f.value]] : [])));

/** A new plan as the widget first offers it: the first kind on offer, hourly, no cap, this page's database. */
export function draft(db: string): Plan {
	const kind = offered(db)[0] ?? '';
	return { name: '', kind, config: defaults(kind), dbs: [db], every: HOUR, slots: null };
}

/** One backup, as `db backup --list --json` reports it. */
export interface Backup {
	slot: number;
	file: string;
	backup_time: string;
	source_db: string;
	entry_count: number;
	size: number;
	age_secs: number;
}

/** What the widget knows about a plan: its backups, newest first, when they were listed, and what it is doing. */
export interface PlanStatus {
	backups: Backup[];
	/** When `backups` was listed (epoch ms): their ages are as of then. */
	at: number;
	running: boolean;
	error: string;
}

const idle = (): PlanStatus => ({ backups: [], at: 0, running: false, error: '' });

export const status = writable<Record<string, PlanStatus>>({});

/** The plans, as `_internal` holds them. */
export const plans = writable<Plan[]>([]);

const destination = (plan: Plan): Destination => {
	const kind = KINDS[plan.kind];
	if (!kind) throw new Error(`no destination kind ${plan.kind}`);
	return kind.open(plan.config);
};

/** `argv`'s stdout, parsed; a rejection with its stderr when it failed. */
async function json<T>(argv: string[], stage?: Stage): Promise<T & { stage?: Stage }> {
	const result = await run(argv, undefined, undefined, undefined, stage);
	if (result.code !== 0) throw cliError(result.stderr, `datum ${argv.join(' ')} exited ${result.code}`);
	return { ...JSON.parse(result.stdout), stage: result.stage };
}

// ---- plans in `_internal` ---------------------------------------------------

async function internal(...argv: string[]): Promise<RunResult> {
	const result = await run(['--db', '_internal', ...argv]);
	if (result.code !== 0) throw cliError(result.stderr, `datum ${argv[0]} exited ${result.code}`);
	return result;
}

/** Re-read the plans. */
export async function loadPlans(): Promise<Plan[]> {
	const { stdout } = await internal(...plansArgv());
	const { rows } = JSON.parse(stdout) as PlanRows;
	const found = rows.flatMap((r) => (r.data?.plan ? [r.data.plan] : [])).sort((a, b) => a.name.localeCompare(b.name));
	plans.set(found);
	return found;
}

/** `tail backup_plan --json`. A field's first log also writes a TRACK row with no data (da-c8mh). */
export interface PlanRows {
	rows: { data: { plan?: Plan } | null }[];
}

/** Save `plan` under its name, new or not. */
export async function savePlan(plan: Plan): Promise<void> {
	const exists = get(plans).some((p) => p.name === plan.name);
	await internal(...savePlanArgv(plan, exists));
	await loadPlans();
}

export async function deletePlan(name: string): Promise<void> {
	await internal(...deletePlanArgv(name));
	await loadPlans();
}

// ---- what is at a destination ----------------------------------------------

/**
 * The backups at `plan`'s destination, newest first, for any database —
 * `db backup --list` on its index. Empty when nothing is there yet.
 */
export async function listed(plan: Plan): Promise<Backup[]> {
	const index = await destination(plan).get('index.json');
	if (!index) return [];
	return (await json<{ backups: Backup[] }>(backupListArgv(), { 'index.json': index })).backups;
}

/** Re-read `plan`'s backups into `status`. */
export async function refresh(plan: Plan): Promise<PlanStatus> {
	const next = { ...idle(), ...get(status)[plan.name], backups: [] as Backup[], at: Date.now(), error: '' };
	try {
		next.backups = await listed(plan);
	} catch (e) {
		next.error = e instanceof Error ? e.message : String(e);
	}
	status.update((s) => ({ ...s, [plan.name]: next }));
	return next;
}

/** The newest backup of `db` in `status`, if any. */
export const newest = (s: PlanStatus, db: string): Backup | undefined => s.backups.find((b) => b.source_db === db);

/** How old `backup` is now, in seconds: its age when listed, plus the time since. */
export const ageOf = (s: PlanStatus, backup: Backup): number => backup.age_secs + Math.max(0, Date.now() - s.at) / 1000;

/** Whether `db`'s newest backup is a period or more old — or it has none. */
export function overdue(plan: Plan, s: PlanStatus, db: string): boolean {
	const last = newest(s, db);
	return !last || ageOf(s, last) >= plan.every;
}

// ---- taking one --------------------------------------------------------------

/** `index.json` as the CLI writes it: per db, the rotation and its backups. */
interface Index {
	version?: number;
	dbs: Record<string, { every_secs: number; slots: number | null; backups: { slot: number; file: string }[] }>;
}

/**
 * `now`, the index at the destination as it is now, with `ours` — the
 * CLI's — merged in for `db`'s `slot` alone; and the files that merge
 * replaced. Every other db's rotation and every other slot is left as
 * `now` has it, so a writer that raced this run keeps what it wrote.
 */
export function merged(now: Index | null, ours: Index, db: string, slot: number): { index: Index; replaced: string[] } {
	const mine = ours.dbs[db];
	const entry = mine.backups.find((b) => b.slot === slot)!;
	const index: Index = now ?? { version: ours.version, dbs: {} };
	const theirs = index.dbs[db]?.backups ?? [];
	index.dbs[db] = {
		...mine,
		backups: [...theirs.filter((b) => b.slot !== slot), entry].sort((a, b) => a.slot - b.slot)
	};
	return { index, replaced: theirs.filter((b) => b.slot === slot && b.file !== entry.file).map((b) => b.file) };
}

/** `db backup --to --json`: the slot written, or null when none was due, and the files it wrote and removed. */
export interface BackupRun {
	slot: number | null;
	written: string[];
	removed: string[];
}

/** One run of the rotation for `db`: the CLI writes, and the destination is given what it wrote. */
async function backupOne(plan: Plan, db: string): Promise<void> {
	const dest = destination(plan);
	const index = await dest.get('index.json');
	const out = await json<BackupRun>(
		['--db', db, ...backupToArgv(plan)],
		index ? { 'index.json': index } : {}
	);
	if (out.slot === null) return;
	// The order the CLI keeps: the slot file, then the index, then what it replaced.
	for (const file of out.written) {
		if (file === 'index.json') continue;
		const want = out.stage![file].byteLength;
		await dest.put(file, out.stage![file]);
		const got = await dest.size(file);
		if (got !== want)
			throw new Error(
				`${file} reached the destination ${got === null ? 'with no length to check' : `as ${got} of ${want} bytes`}; the backup it would replace is kept`
			);
	}
	const now = await dest.get('index.json');
	const ours = JSON.parse(new TextDecoder().decode(out.stage!['index.json']));
	const next = merged(now && JSON.parse(new TextDecoder().decode(now)), ours, db, out.slot);
	await dest.put('index.json', new TextEncoder().encode(JSON.stringify(next.index, null, 2)));
	for (const file of new Set([...out.removed, ...next.replaced])) await dest.remove(file);
}

/** One run at a time, across every plan: two runs on one index would lose a slot. */
let queue: Promise<unknown> = Promise.resolve();

/** What «Back up now» says while another tab holds the device's one run. */
const BUSY = 'Another datum.day tab is backing up. Try again when it is done.';

/**
 * `job`, as this DEVICE's one run (da-u050c): `queue` is a tab's, and so is
 * the schedule, and `merged` cannot see a tab that re-read the index before
 * this one put its own — the later put names a file the other removed. The
 * lock is taken only when free and never waited for: a bucket request hung
 * in the tab holding it would hang this one too. False when another tab has
 * it. A browser without Web Locks runs as it did.
 */
async function alone(job: () => Promise<void>): Promise<boolean> {
	const locks = typeof navigator === 'undefined' ? undefined : navigator.locks;
	if (!locks) return job().then(() => true);
	return locks.request('datum-backups', { ifAvailable: true }, async (lock) => {
		if (lock) await job();
		return !!lock;
	});
}

/** Back up `plan`'s databases now — every one of them, or only the `due` ones. */
export function backUp(plan: Plan, due = false): Promise<void> {
	const job = queue.then(async () => {
		const ran = await alone(async () => {
			status.update((s) => ({ ...s, [plan.name]: { ...idle(), ...s[plan.name], running: true } }));
			let error = '';
			try {
				const before = await refresh(plan);
				for (const db of plan.dbs) if (!due || overdue(plan, before, db)) await backupOne(plan, db);
			} catch (e) {
				error = e instanceof Error ? e.message : String(e);
			}
			const after = await refresh(plan);
			status.update((s) => ({ ...s, [plan.name]: { ...after, running: false, error: error || after.error } }));
		});
		// The schedule says nothing: its next check runs, and lists what that tab took.
		if (!ran && !due) status.update((s) => ({ ...s, [plan.name]: { ...idle(), ...s[plan.name], error: BUSY } }));
	});
	queue = job;
	return job;
}

// ---- restoring one -----------------------------------------------------------

/**
 * Restore `db` from `backup`, once the reader has seen what it does: the
 * CLI's `--dry-run` counts on the same file, in a confirm (da-5fooh — a
 * restore REPLACES, so the deletions are the number that matters).
 */
export async function restoreFrom(plan: Plan, db: string, backup: Backup): Promise<RunResult | null> {
	const bytes = await destination(plan).get(backup.file);
	if (!bytes) throw new Error(`${backup.file} is not at the destination any more`);
	const stage = { [backup.file]: bytes };
	const argv = ['--db', db, ...restoreStagedArgv(backup.file)];
	const seen = await json<RestorePreview>(restorePreviewArgv(argv), stage);
	// Its warnings too (da-vceux): a header that disagrees with the lines is known before the yes.
	const text =
		`Restore ${db} to the backup of ${new Date(backup.backup_time).toLocaleString()} (${backup.entry_count} entries): ` +
		`${seen.would_delete} deleted, ${seen.would_insert} added, ${seen.would_update} changed. ` +
		'Undo reverts the whole restore.' +
		seen.warnings.map((w) => ` ⚠ ${w}.`).join('');
	if (!(await confirm(text, 'restore'))) return null;
	return run([...argv, '--yes'], undefined, undefined, undefined, stage);
}

// ---- the explanation ---------------------------------------------------------

/** `db backup --plan`: the rotation after `span` seconds of `plan`. */
export interface Reach {
	span_secs: number;
	files: number;
	reach_secs: number | null;
}

/** The rotation `plan` holds after a day, a month and a year. */
export const reaches = (plan: Plan): Promise<Reach[]> =>
	Promise.all(
		[86_400, 30 * 86_400, 365 * 86_400].map((span) =>
			json<Reach>(planReachArgv(plan, span))
		)
	);

/** The size of one backup of `db`, gzipped, as the CLI writes it. */
export async function backupSize(db: string): Promise<number> {
	const result = await run(['--db', db, ...backupSizeArgv()], undefined, undefined, undefined, {});
	if (result.code !== 0) throw new Error(result.stderr);
	return result.stage?.['size.jsonl.gz']?.length ?? 0;
}

// ---- the schedule ------------------------------------------------------------

/**
 * Back up whatever is overdue: every plan's databases whose newest backup
 * is a period old. At start — «plus one on open when overdue» — and each
 * minute after while the tab is open. A closed browser takes no backups.
 *
 * The minute's check runs no process unless something is due: a plan's
 * backups are listed once, and their ages counted on from then.
 */
export async function check(): Promise<void> {
	for (const plan of get(plans)) {
		const known = get(status)[plan.name];
		const s = known?.at ? known : await refresh(plan);
		if (!s.running && plan.dbs.some((db) => overdue(plan, s, db))) await backUp(plan, true);
	}
}

let timer: ReturnType<typeof setInterval> | undefined;

/** Start the schedule, once per tab: the plans read once, and checked now and each minute. */
export function schedule(): void {
	if (timer) return;
	timer = setInterval(() => void check(), 60_000);
	void loadPlans().then(check, () => {});
}

// ---- words -------------------------------------------------------------------

/** `3 h ago`-style: the largest unit, whole. */
export function ago(secs: number): string {
	const units: [number, string][] = [
		[365 * 86_400, 'y'],
		[86_400, 'd'],
		[3600, 'h'],
		[60, 'min']
	];
	const [size, unit] = units.find(([size]) => secs >= size) ?? [1, 's'];
	return `${Math.floor(secs / size)} ${unit}`;
}

/** Bytes, the way a person reads a size. */
export function bytes(n: number): string {
	const units = ['B', 'KB', 'MB', 'GB'];
	let i = 0;
	while (n >= 1024 && i < units.length - 1) (n /= 1024), i++;
	return `${i ? n.toFixed(1) : n} ${units[i]}`;
}

// ---- the place ---------------------------------------------------------------

/** Asked by `db restore`: the Backups widget scrolls to the backups to restore from, and clears it. */
export const toRestore = writable(false);

/**
 * Open «Backups» ($lib/layout/BackupsWidget.svelte), a PLACE (da-ea5q2):
 * a modal on a desktop, a tab on a phone, by `openPlace` — `restore` at the backups
 * to restore from. The shell is imported on use: through the registry, it
 * imports this module.
 */
export async function openBackups(view: 'plans' | 'restore' = 'plans'): Promise<void> {
	toRestore.set(view === 'restore');
	const { openPlace } = await import('./layout/shell.js');
	await openPlace('backups');
}
