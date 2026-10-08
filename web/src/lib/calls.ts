// Every command datum.day sends on its own — the ones no person types —
// built here, or re-exported from the module that builds it (da-1y9gi).
//
// `tests/calls-pure.test.mjs` runs each export through the real CLI in node,
// on a seeded database, and fails on a usage error — a flag, subcommand or
// argument the Rust side renamed — naming the files that send it. `READS`
// is the keys the page reads out of each JSON answer, so a key renamed in
// Rust fails there too, and not on live. A new call goes here; one that is
// parsed goes into `READS` as well, and the test refuses a `--json` without.
//
// A `--db` is left to the caller (`syncAct`, sync.ts's `cli`, the layout
// store's `_internal`…): it is a global flag, and the database a call names
// is the caller's to choose.

import type { Plan } from './backups.js';
import type { Layout } from './layout/types.js';
import type { PaneSpec } from './panes.js';
import type { TailPage } from './tail.js';

export { bandArgv, geometryArgv, nowviewSpec } from './panes.js';
export { statsArgv } from './stats.js';
export { pressArgv } from './fields.js';
export { specArgv, trackArgv } from './track.js';

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
		...(page.before !== undefined ? ['-t', page.before] : [])
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
 * The keys the page reads from each call's JSON, by the call's name. A path
 * is dotted; `[]` after a segment is any element of an array, and a leading
 * `[]` is the answer itself being one. Each must be found in at least one
 * sample answer the test gets (`tests/calls-pure.test.mjs`).
 */
export const READS: Record<string, string[]> = {
	// $lib/fields.ts `FieldListing`, the lane Placement.svelte reads
	fieldsArgv: ['[].field', '[].spec', '[].states', '[].color', '[].state_colors', '[].ignored', '[].y', '[].height'],
	frecencyArgv: ['[].field', '[].y', '[].frecency'],
	// $lib/commands.ts `GrammarEntry`
	grammarArgv: [
		'[].name',
		'[].aliases',
		'[].usage',
		'[].bare',
		'[].subcommands[].name',
		'[].flags[].short',
		'[].flags[].long',
		'[].flags[].takes_value'
	],
	nowArgv: ['active[].field', 'active[].state'],
	firstEntryArgv: ['rows[].time', 'rows[].dur_seconds'],
	lastDotsArgv: ['rows[].verb', 'rows[].time'],
	aliasesArgv: ['[].name', '[].expansion', '[].source', '[].field'],
	// $lib/tail.ts `TailRow`, `TailColumn`
	tailArgv: [
		'rows[].hid',
		'rows[].field',
		'rows[].verb',
		'rows[].time',
		'rows[].state_label',
		'rows[].quick_id',
		'rows[].previous_state',
		'rows[].previous_state_label',
		'rows[].previous_state_color',
		'rows[].previous_state_cell',
		'rows[].following_state',
		'rows[].following_state_label',
		'rows[].following_state_color',
		'rows[].following_state_cell',
		'rows[].state_body',
		'rows[].state_repeat',
		'columns[].key',
		'columns[].header',
		'columns[].cells'
	],
	// DayStrip.svelte `readGeometry`
	geometryArgv: ['hours[].hour', 'hours[].x', 'axis_ticks[].is_major', 'axis_ticks[].label', 'days[].date'],
	// $lib/stats.ts `AllFieldsStats`, `FieldStats`
	showAllArgv: [
		'total_count',
		'tracked_duration_seconds',
		'earliest',
		'latest',
		'fields[].field',
		'fields[].color',
		'fields[].count',
		'fields[].duration_seconds',
		'fields[].ratio',
		'fields[].frequency.per_day',
		'total_duration_seconds',
		'frequency',
		'states[].state',
		'states[].count',
		'states[].duration_seconds',
		'states[].ratio'
	],
	// $lib/track.ts `Spec`
	specShowArgv: ['field', 'verb', 'color', 'description', 'allowed_states', 'state_colors'],
	// $lib/syncView.ts `DbSync`, `Member`; $lib/sync.ts `Peer`
	syncStatusArgv: [
		'chain',
		'paused',
		'notices',
		'clock',
		'this.endpoint',
		'this.client',
		'this.label',
		'this.datum',
		'this.schema',
		'members[].endpoint',
		'members[].label',
		'members[].kind',
		'members[].revoked',
		'members[].addr',
		'members[].state',
		'members[].to_send'
	],
	chainListArgv: ['admins'],
	adminListArgv: ['admins'],
	// $lib/sync.ts `Invite`
	newInviteArgv: ['secret', 'expires'],
	// $lib/syncView.ts `purgeWords`: the preview's; the `--yes` purge answers no JSON
	purgeArgv: ['created', 'edited', 'deleted', 'defined', 'first', 'last'],
	restorePreviewArgv: ['would_delete', 'would_insert', 'would_update', 'warnings', 'fingerprint', 'dropped'],
	plansArgv: ['rows[].data.plan'],
	// $lib/backups.ts `Backup`
	backupListArgv: [
		'backups[].slot',
		'backups[].file',
		'backups[].backup_time',
		'backups[].source_db',
		'backups[].entry_count',
		'backups[].size',
		'backups[].age_secs'
	],
	backupToArgv: ['slot', 'written', 'removed'],
	planReachArgv: ['span_secs', 'files', 'reach_secs'],
	// $lib/layout/store.ts `Row`
	layoutsArgv: [
		'rows[].hid',
		'rows[].slug',
		'rows[].modify_time',
		'rows[].data.name',
		'rows[].data.version',
		'rows[].data.root',
		'rows[].data.designedFor',
		'rows[].data.kit_id'
	],
	layoutHistoryArgv: ['[].version', '[].modify_time']
};
