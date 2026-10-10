// Every command datum.day sends on its own — the ones no person types —
// built here, or re-exported from the module that builds it (da-1y9gi).
//
// `tests/calls-pure.test.mjs` runs each export through the real CLI in node,
// on a seeded database, and fails on a usage error — a flag, subcommand or
// argument the Rust side renamed — naming the files that send it. `Reads`
// is the type the page reads each JSON answer as, so a key renamed in Rust
// fails there too, and not on live. A new call goes here; one that is
// parsed goes into `Reads` as well, and the test refuses a `--json` without.
//
// A `--db` is left to the caller (`syncAct`, sync.ts's `cli`, the layout
// store's `_internal`…): it is a global flag, and the database a call names
// is the caller's to choose.

import type { Backup, BackupRun, Plan, PlanRows, Reach } from './backups.js';
import type { Alias, GrammarEntry } from './commands.js';
import type { Conflicted } from './conflicts.js';
import type { RestorePreview } from './dbio.js';
import type { FieldListing, NowListing } from './fields.js';
import type { LayoutRow, LayoutVersion } from './layout/store.js';
import type { Layout } from './layout/types.js';
import type { DayviewJson, PaneSpec } from './panes.js';
import type { AllFieldsStats, FieldStats } from './stats.js';
import type { Invite } from './sync.js';
import type { Admins, PurgePreview, SyncStatus } from './syncView.js';
import type { TailListing, TailPage } from './tail.js';
import type { Spec } from './track.js';

export { bandArgv, geometryArgv, nowviewSpec } from './panes.js';
export { statsArgv } from './stats.js';
export { pressArgv } from './fields.js';
export { specArgv, trackArgv } from './track.js';
export { resolveArgv } from './conflicts.js';

// ---- the fields bar, the strip, the panes --------------------------------------

export function fieldsArgv(): string[] {
	return ['fields', '--json'];
}

export function frecencyArgv(): string[] {
	return ['fields', '--json', '--sort', 'frecency'];
}

export function grammarArgv(): string[] {
	return ['help', '--json'];
}

export function nowArgv(): string[] {
	return ['now', '--json'];
}

export function helpArgv(command?: string): string[] {
	return command && command !== 'help' ? ['help', ...command.split(' ')] : ['help'];
}

/** The oldest TIMED entry: a timeless one sorts first and has no day. */
export function firstEntryArgv(): string[] {
	return ['head', '-n', '1', '--metric', 'occur', '--json'];
}

export function lastDotsArgv(field: string): string[] {
	return ['tail', field, '-n', '50', '--json'];
}

export function aliasesArgv(): string[] {
	return ['alias', 'list', '--json'];
}

/** The field FIRST: it is the command's positional, and a flag between `tail` and it would make it the flag's value. */
export function tailArgv(page: TailPage = {}): string[] {
	return [
		'tail',
		...(page.field ? [page.field] : []),
		'--json',
		...(page.count !== undefined ? ['-n', String(page.count)] : []),
		...(page.above !== undefined ? ['--above', page.above] : [])
	];
}

/** `--show all`: the table, then the `--json` line under it — one computation for both. */
export function showAllArgv(argv: string[]): string[] {
	return [...argv, '--show', 'all'];
}

export function specShowArgv(field: string): string[] {
	return ['spec', 'show', field];
}

export function redoArgv(): string[] {
	return ['redo', '--dry-run'];
}

export function conflictsArgv(): string[] {
	return ['conflicts', '--json'];
}

export const DAYVIEW_VERTICAL: PaneSpec = {
	title: 'dayview',
	argv: ['dayview', '--orientation', 'vertical', '--days', '7']
};

// ---- sync ------------------------------------------------------------------------

export function syncStatusArgv(): string[] {
	return ['sync', 'status', '--json'];
}

export function chainListArgv(): string[] {
	return ['sync', 'chain', 'list', '--json'];
}

/** Succeeds only where the copy holds nothing but its own data (da-h7pnc). */
export function canLeaveArgv(): string[] {
	return ['sync', 'chain', 'leave', '--alone', '--dry-run'];
}

export function leaveArgv(): string[] {
	return ['sync', 'chain', 'leave'];
}

export function newInviteArgv(): string[] {
	return ['sync', 'new-invite'];
}

export function endpointKeyArgv(): string[] {
	return ['sync', 'endpoint-key'];
}

export function vectorArgv(): string[] {
	return ['sync', 'pipe', 'vector'];
}

export function approveArgv(link: string): string[] {
	return ['sync', 'approve', link];
}

export function qrArgv(text: string): string[] {
	return ['sync', 'qr', text];
}

export function pausedArgv(pause: boolean): string[] {
	return ['sync', pause ? 'pause' : 'resume'];
}

/** This copy's own label, or — given its endpoint — another member's. */
export function renameArgv(label: string, endpoint?: string): string[] {
	return ['sync', 'chain', 'rename', ...(endpoint ? [endpoint] : []), label];
}

export function revokeArgv(endpoint: string): string[] {
	return ['sync', 'chain', 'revoke', endpoint];
}

/** The preview, or with `yes` the purge itself (da-ry11c). */
export function purgeArgv(endpoint: string, yes = false): string[] {
	return ['sync', 'chain', 'purge', endpoint, yes ? '--yes' : '--json'];
}

export function ackArgv(subject: string): string[] {
	return ['sync', 'chain', 'ack', subject];
}

export function adminAddArgv(key: string): string[] {
	return ['sync', 'admin', 'add', key];
}

export function adminListArgv(): string[] {
	return ['sync', 'admin', 'list', '--json'];
}

export function adminRemoveArgv(key: string, unlock = false): string[] {
	return ['sync', 'admin', 'remove', key, ...(unlock ? ['--allow-unlock'] : [])];
}

// ---- backups and the database --------------------------------------------------

export function backupArgv(): string[] {
	return ['db', 'backup', '-'];
}

export function restoreArgv(): string[] {
	return ['db', 'restore', '-'];
}

export function mergeArgv(): string[] {
	return ['db', 'restore', '-', '--merge'];
}

export function dbPurgeArgv(flags: string[] = []): string[] {
	return ['db', 'purge', '--yes', ...flags];
}

/** What a restore would do, counted on the same file (da-5fooh). */
export function restorePreviewArgv(argv: string[]): string[] {
	return [...argv, '--dry-run', '--json'];
}

export function restoreStagedArgv(file: string): string[] {
	return ['db', 'restore', `/data/stage/${file}`];
}

/** Plans live in `_internal`. */
export function plansArgv(): string[] {
	return ['tail', 'backup_plan', '--json', '-n', '100000'];
}

export function savePlanArgv(plan: Plan, exists: boolean): string[] {
	const value = `plan=${JSON.stringify(plan)}`;
	return exists
		? ['update', `backup_plan:${plan.name}`, '-k', value]
		: ['occur', 'backup_plan', '--id', '%name', '-k', `name=${JSON.stringify(plan.name)}`, '-k', value];
}

export function deletePlanArgv(name: string): string[] {
	return ['delete', `backup_plan:${name}`];
}

const everyArgs = (plan: Plan) => ['--every', `${plan.every}s`, ...(plan.slots ? ['--slots', String(plan.slots)] : [])];

export function backupListArgv(): string[] {
	return ['db', 'backup', '--list', '/data/stage', '--json'];
}

export function backupToArgv(plan: Plan): string[] {
	return ['db', 'backup', '--to', '/data/stage', ...everyArgs(plan), '--json'];
}

export function planReachArgv(plan: Plan, span: number): string[] {
	return ['db', 'backup', '--plan', ...everyArgs(plan), '--span', `${span}s`, '--json'];
}

export function backupSizeArgv(): string[] {
	return ['db', 'backup', '/data/stage/size.jsonl.gz'];
}

// ---- layouts: the person's in `_internal`, a kit's in the data database ---------

const layoutSlug = (name: string) => `layout:${name}`;

/** The tree as `-k` data. Every value JSON, so a name like `123` stays a string; an empty `designedFor=` removes one. */
const layoutKeys = (layout: Layout, kit?: string) => [
	'-k',
	`version=${layout.version}`,
	'-k',
	`root=${JSON.stringify(layout.root)}`,
	'-k',
	`designedFor=${layout.designedFor ? JSON.stringify(layout.designedFor) : ''}`,
	...(kit ? ['-k', `kit_id=${JSON.stringify(kit)}`] : [])
];

export function layoutsArgv(): string[] {
	return ['tail', 'layout', '--json', '-n', '100000'];
}

export function newLayoutArgv(layout: Layout, kit?: string): string[] {
	return ['occur', 'layout', '--id', '%name', '-k', `name=${JSON.stringify(layout.name)}`, ...layoutKeys(layout, kit)];
}

export function updateLayoutArgv(layout: Layout, kit?: string): string[] {
	return ['update', layoutSlug(layout.name), ...layoutKeys(layout, kit)];
}

export function layoutHistoryArgv(name: string): string[] {
	return ['history', layoutSlug(name), '--json'];
}

export function layoutUndoArgv(name: string, version?: number): string[] {
	return ['undo', layoutSlug(name), ...(version === undefined ? [] : ['--to-version', String(version)])];
}

/** By hid, since `del` splits its argument at commas and a name may have one. */
export function deleteLayoutArgv(hid: string): string[] {
	return ['del', hid, '--force'];
}

// ---- the sample week -------------------------------------------------------------

export function sampleTaggedArgv(): string[] {
	return ['grep', '"sample":true', '-o', '%full_hid%'];
}

export function deleteSampleArgv(hids: string[]): string[] {
	return ['delete', hids.join(','), '--show', 'none'];
}

export function kitInstallArgv(): string[] {
	return ['kit', 'install', '-'];
}

export function kitUninstallArgv(): string[] {
	return ['kit', 'uninstall', 'sample'];
}

// ---- what the page reads -----------------------------------------------------------

/**
 * The type the page reads each call's JSON as, by the call's name — the
 * types its call sites parse the answer into, and nothing kept beside them
 * (da-qy4w0). `tests/calls-pure.test.mjs` walks each with the TypeScript
 * checker into every key it names, nested, and fails on one that no sample
 * answer carries: a key a call site starts reading is checked once its type
 * has it. Where call sites read several, `|` lists them. The walk checks a
 * property tagged `@shallow` and nothing under it: what the page wrote
 * itself, or what no seeded database can hold.
 */
export interface Reads {
	/** `frecency` comes with `--sort frecency` alone. */
	fieldsArgv: Omit<FieldListing, 'frecency'>[];
	frecencyArgv: FieldListing[];
	grammarArgv: GrammarEntry[];
	nowArgv: NowListing;
	firstEntryArgv: TailListing;
	lastDotsArgv: TailListing;
	aliasesArgv: Alias[];
	conflictsArgv: Conflicted[];
	tailArgv: TailListing;
	geometryArgv: DayviewJson;
	showAllArgv: AllFieldsStats | FieldStats;
	specShowArgv: Spec;
	syncStatusArgv: SyncStatus;
	chainListArgv: Admins;
	adminListArgv: Admins;
	newInviteArgv: Invite;
	/** The preview's: the `--yes` purge answers no JSON. */
	purgeArgv: PurgePreview;
	restorePreviewArgv: RestorePreview;
	plansArgv: PlanRows;
	backupListArgv: { backups: Backup[] };
	backupToArgv: BackupRun;
	planReachArgv: Reach;
	layoutsArgv: { rows: LayoutRow[] };
	layoutHistoryArgv: LayoutVersion[];
}
