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

import type { BackupRun, Listed, Plan, PlanRows, Reach } from './backups.js';
import type { Alias, GrammarEntry } from './commands.js';
import type { Conflicted } from './conflicts.js';
import type { RestorePreview } from './dbio.js';
import type { FieldListing, NowListing } from './fields.js';
import type { KitRow, LayoutRow, LayoutVersion } from './layout/store.js';
import type { Layout } from './layout/types.js';
import type { DayviewJson, PaneSpec } from './panes.js';
import type { AllFieldsStats, FieldStats } from './stats.js';
import { shellSplit } from './shellSplit.js';
import type { Invite } from './sync.js';
import type { Admins, ChainList, Grid, PurgePreview, SyncStatus } from './syncView.js';
import type { TailListing, TailPage } from './tail.js';
import type { Premade, Spec } from './track.js';
import type { Refs } from './selection.js';

export { bandArgv, geometryArgv, layersArgv, nowviewSpec } from './panes.js';
export { statsArgv } from './stats.js';
export { pressArgv } from './fields.js';
export { declareArgv, specArgv } from './track.js';
export { resolveArgv } from './conflicts.js';

// ---- the fields bar, the strip, the panes --------------------------------------

export function fieldsArgv(): string[] {
	return ['fields', '--json'];
}

/** The one field's row, one the database knows nothing of too: where the pictures will draw it (da-gq232). */
export function fieldArgv(field: string): string[] {
	return ['fields', field, '--json'];
}

/** `argv`, a log line, as what it would write and does not (da-axrdv): asked under `DATUM_SHOW=json`, a DRYRUN document. */
export function dryRunArgv(argv: string[]): string[] {
	return [...argv, '--dry-run'];
}

export function recencyArgv(): string[] {
	return ['fields', '--json', '--sort', 'recency'];
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

/** The oldest TIMED entry: a timeless one has no day. */
export function firstEntryArgv(): string[] {
	return ['head', '-n', '1', '--metric', 'occur', '--json'];
}

/** The last entry, meta left out: whether a database holds entries (da-bedyz). */
export function lastEntryArgv(): string[] {
	return ['tail', '-n', '1', '--json'];
}

export function aliasesArgv(): string[] {
	return ['alias', 'list', '--json'];
}

/** The field before any flag: it is the command's positional, and a flag between `tail` and it would make it the flag's value. */
export function tailArgv(page: TailPage = {}): string[] {
	return [
		page.head ? 'head' : 'tail',
		...(page.field ? [page.field] : []),
		'--json',
		...(page.count !== undefined ? ['-n', String(page.count)] : []),
		...(page.above !== undefined ? ['--above', page.above] : []),
		...(page.below !== undefined ? ['--below', page.below] : []),
		...(page.since !== undefined ? ['--since', page.since] : []),
		...(page.until !== undefined ? ['-t', page.until] : []),
		...(page.state !== undefined ? ['--state', page.state] : [])
	];
}

/** `--show all`: the table, then the `--json` line under it — one computation for both. */
export function showAllArgv(argv: string[]): string[] {
	return [...argv, '--show', 'all'];
}

export function specShowArgv(field: string): string[] {
	return ['setup', field, '--json'];
}

/** The last rows written, specs included, oldest first: the setup widget's recent fields (da-asgtm). */
export function touchedArgv(): string[] {
	return ['tail', '-m', 'modify', '--show-meta', '-n', '40', '--json'];
}

/** premade.toml, compiled into the CLI (da-rky8h): the setup widget's catalog (da-asgtm). */
export function premadeArgv(): string[] {
	return ['setup', '--premade', '--json'];
}

/** The spec entry's history (da-hda4j): every version of it, newest first. */
export function specHistoryArgv(field: string): string[] {
	return ['history', `spec:${field}`, '--json'];
}

/** One entry put back as version `version` of its history. */
export function revertToVersionArgv(hid: string, version: number): string[] {
	return ['revert', hid, '--to-version', String(version)];
}

/** The field renamed, or with `dry` what it would move (da-qimwi); with `merge`, into a field that exists (da-icna7). */
export function renameFieldArgv(old: string, to: string, dry = false, merge = false): string[] {
	return ['rename', old, to, ...(merge ? ['--merge'] : []), ...(dry ? ['--dry-run'] : [])];
}

/** A command's entries stepped back by its button, or forward again by a second press (da-25q3k, da-k5vg1). */
export function stepArgv(hids: string[]): string[] {
	return ['revert', hids.join(',')];
}

/**
 * A time as `-t` reads it, and a length as `dur=` does, read by the CLI with no
 * database: the nowview's empty overlay, whose `--json` says the instant
 * (`at_ms`) and the window's length (`history_minutes`). Collect's time
 * section types them (da-us1n1).
 */
export function timeReadArgv(time: string): string[] {
	return ['nowview', '--overlay', '--json', '-t', time];
}
export function lengthReadArgv(length: string): string[] {
	return ['nowview', '--overlay', '--json', '--context', length];
}

/** A field's states, its spec's in its order then every other it has used, each with when it was last used: a field opened in collect (da-r48y9). */
export function statesArgv(field: string): string[] {
	return ['states', field, '--show', 'json'];
}

/** One row of `statesArgv`'s answer. */
export interface StateUse {
	state: unknown;
	declared: boolean;
	count: number;
	last_used: string | null;
}

/** Entries, `a,b,…`, every column, one document each, as collect's entry cards read them (da-xdz73, da-ug56t). */
export function entryArgv(hids: string): string[] {
	return ['get', hids, '--pretty'];
}

/** An entry's time and its length, a line each (none where it has none): what the nowview follows an `hid` pill to (da-2v3mr). */
export function entrySpanArgv(hid: string): string[] {
	return ['get', `${hid}::at,${hid}::dur`];
}

/** The entries' trails, newest version first: one hid's alone, or each under its own hid. */
export function entriesHistoryArgv(hids: string[]): string[] {
	return ['history', hids.join(','), '--json'];
}

export function redoArgv(): string[] {
	return ['redo', '--dry-run'];
}

export function conflictsArgv(): string[] {
	return ['conflicts', '--json'];
}

/** Each `_n` and the hid it names, which label the hid pill (da-kjikt). */
export function refsArgv(): string[] {
	return ['ids', '--ref', '--json'];
}

/** As many days as the pane's width holds: `paneArgv` names them (da-nopcd). */
export const DAYVIEW_VERTICAL: PaneSpec = {
	title: 'dayview',
	argv: ['dayview', '--orientation', 'vertical']
};

// ---- sync ------------------------------------------------------------------------

export function syncStatusArgv(): string[] {
	return ['sync', 'status', '--json', '--no-grants'];
}

export function chainListArgv(): string[] {
	return ['sync', 'chain', 'list', '--json'];
}

/** Succeeds only where the copy holds nothing but its own data (da-h7pnc). */
export function canLeaveArgv(): string[] {
	return ['sync', 'chain', 'leave', '--alone', '--dry-run'];
}

export function leaveArgv(force = false): string[] {
	return ['sync', 'chain', 'leave', ...(force ? ['--force'] : [])];
}

export function newInviteArgv(reader = false): string[] {
	return ['sync', 'new-invite', ...(reader ? ['--reader'] : [])];
}

/** Every device this install's databases know of, against every chain they are in (da-n0uj1.3). */
export function gridArgv(): string[] {
	return ['sync', 'grid', '--json'];
}

/** `device` invited to this chain, the invite's `ticket` (the sync worker's) sealed to it in a chain both share (da-n0uj1.7). */
export function grantArgv(device: string, role: 'member' | 'reader', ticket: string, invite: string, name?: string): string[] {
	return ['sync', 'chain', 'invite', device, ...(role === 'reader' ? ['--reader'] : []), '--ticket', ticket, '--invite', invite, ...(name ? ['--as', name] : [])];
}

/** A member's role changed. */
export function roleArgv(endpoint: string, role: 'member' | 'reader'): string[] {
	return ['sync', 'chain', 'role', endpoint, role];
}

/** The device expelled, and asked to delete its copy (da-n0uj1.6). */
export function wipeArgv(endpoint: string): string[] {
	return ['sync', 'chain', 'expel', endpoint, '--wipe', '--yes'];
}

/** The signed-in account's key, and with `chain` an invite to it the account signs (da-nf1p4). */
export function accountArgv(chain?: string): string[] {
	return ['sync', 'account', ...(chain ? ['--invite', chain] : [])];
}

/**
 * The account record to publish (da-rnxp4), hex: the account's chains here and those `held` (the relay's) lists,
 * reached by `relay` when not n0's.
 */
export function accountRecordArgv(relay?: string, held?: string): string[] {
	return ['sync', 'account', '--record', ...(relay ? ['--relay', relay] : []), ...(held ? ['--with', held] : [])];
}

/** The chains record `body` lists that this install holds no copy of, each with the database to join it into. */
export function accountReadArgv(body: string): string[] {
	return ['sync', 'account', '--read', body];
}

/** What a waiting `datum sync login` takes (da-mzqs4): the account sealed to the device key `link` names, with its nonce. */
export function accountSealArgv(link: string): string[] {
	return ['sync', 'account', '--seal', link];
}

/** The account key's approval code of what an approve link carries (da-k04lr): run with the passkey's PRF output. */
export function accountApproveArgv(link: string): string[] {
	return ['sync', 'account', '--approve', link];
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

/** No label for this copy: every member shows the one assembled from what it runs on (da-xijlw). */
export function unsetRenameArgv(): string[] {
	return ['sync', 'chain', 'rename', '--unset'];
}

/** This copy described by `user`, its label kept (da-xza37): the username is this browser's. */
export function userArgv(user: string): string[] {
	return ['sync', 'chain', 'rename', '--user', user];
}

/** Whether `label` is one a member may carry, asked before a join: it writes nothing. */
export function labelCheckArgv(label: string): string[] {
	return ['sync', 'chain', 'rename', '--check', label];
}

/** The device stops syncing; it keeps its copy, and what it wrote stays (da-n0uj1.1). */
export function expelArgv(endpoint: string): string[] {
	return ['sync', 'chain', 'expel', endpoint];
}

/** The preview, or with `yes` the unjoin itself: everything it wrote withdrawn (da-ry11c). */
export function unjoinArgv(endpoint: string, yes = false): string[] {
	return ['sync', 'chain', 'expel', endpoint, '--unjoin', yes ? '--yes' : '--json'];
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

/**
 * `db purge` without `--yes` (da-0hay9): it makes every refusal the CLI
 * makes before its name prompt, then goes unanswered, so it purges nothing.
 */
export function dbPurgeProbeArgv(flags: string[] = []): string[] {
	return ['db', 'purge', ...flags.filter((f) => f !== '--yes' && f !== '-y')];
}

/**
 * `db purge --chain`, every copy in the chain (da-2bb7z): without
 * `yes` it names each copy and when it purges, and changes nothing.
 */
export function dbPurgeChainArgv(yes = false): string[] {
	return ['db', 'purge', '--chain', ...(yes ? ['--yes'] : [])];
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
		: ['moment', 'backup_plan', '--id', '%name', '-k', `name=${JSON.stringify(plan.name)}`, '-k', value];
}

export function deletePlanArgv(name: string): string[] {
	return ['delete', `backup_plan:${name}`];
}

const everyArgs = (plan: Plan) => ['--every', `${plan.every}s`, ...(plan.slots ? ['--slots', String(plan.slots)] : [])];

export function backupListArgv(): string[] {
	return ['db', 'backup', '--list', '/data/stage', '--json'];
}

/** The rotation's step: its period and cap are the index's, which `backupOne` writes from the plan (da-tsna7). */
export function backupToArgv(): string[] {
	return ['db', 'backup', '--to', '/data/stage', '--json'];
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
	// A new layout: a name already taken is refused, never updated (da-wudrc).
	return ['moment', 'layout', '--id', '%name', '--conflict', 'refuse', '-k', `name=${JSON.stringify(layout.name)}`, ...layoutKeys(layout, kit)];
}

export function updateLayoutArgv(layout: Layout, kit?: string): string[] {
	return ['update', layoutSlug(layout.name), ...layoutKeys(layout, kit)];
}

export function layoutHistoryArgv(name: string): string[] {
	return ['history', layoutSlug(name), '--json'];
}

export function layoutRevertArgv(name: string, version?: number): string[] {
	return ['revert', layoutSlug(name), ...(version === undefined ? [] : ['--to-version', String(version)])];
}

/** By hid, since `del` splits its argument at commas and a name may have one. */
export function deleteLayoutArgv(hid: string): string[] {
	return ['del', hid];
}

/** Keep the image on stdin, a `data:` URL of it, by its hash (da-wgf4a); it prints `attachment:<hash>`. */
export function keepAttachmentArgv(): string[] {
	return ['db', 'attachment'];
}

/** The attachment `hash`, printed as a `data:` URL. */
export function attachmentArgv(hash: string): string[] {
	return ['db', 'attachment', hash];
}

// ---- a kit's widgets (da-gh37k) -----------------------------------------------------

/** Every installed kit's `[kit]` table, its widgets in it. */
export function kitsArgv(): string[] {
	return ['tail', 'kit', '--json', '-n', '100000'];
}

/** A kit widget's `command`, asked for its rows. */
export function kitWidgetArgv(command: string): string[] {
	return [...shellSplit(command), '--json'];
}

// ---- the sample week -------------------------------------------------------------

export function sampleTaggedArgv(): string[] {
	return ['grep', '"sample":true', '-o', '%full_hid%'];
}

export function deleteHidsArgv(hids: string[]): string[] {
	return ['delete', hids.join(','), '--show', 'none'];
}

export function kitInstallArgv(): string[] {
	return ['kit', 'install', '-'];
}

export function kitUninstallArgv(): string[] {
	return ['kit', 'uninstall', 'sample'];
}

// ---- the tutorial (da-6cvrd) ---------------------------------------------------------

/** Its kit, from stdin, over whatever it installed before: the database is the kit's own. */
export function tutorialKitArgv(): string[] {
	return ['kit', 'install', '-', '--force', '--yes'];
}

/** `word`, the tutorial's one setup command (da-uzyih.4937), made: it reads the kit from stdin, tabs included. */
export function tutorialAliasArgv(word: string): string[] {
	return ['alias', 'set', word, tutorialKitArgv().join(' ')];
}

/**
 * `field`'s state ended: an on/off field off, a change field false (a game's Reset, da-uzyih.4864),
 * and nothing written unless it is still in `state`, as `now --json` read it (`-L`, da-v2d29).
 */
export function endArgv(field: string, state: unknown): string[] {
	return ['off', field, '-L', String(state)];
}

/** `fields` TRACKED, as a game begins (da-uzyih.4882), in one command (da-klzka); on a field already tracked, nothing is written. */
export function trackArgv(fields: string[]): string[] {
	return ['track', fields.join(',')];
}

/** `fields` IGNORED, their states ended, as a game ends (da-uzyih.4882). */
export function ignoreArgv(fields: string[]): string[] {
	return ['ignore', fields.join(',')];
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
/**
 * One row of a kit command's `--json` (da-gh37k): the shown columns, which
 * a `row_action`'s `{column}` reads, the cells as the table prints them,
 * and the colours its rule paints the row, where one does.
 */
export type KitWidgetRow = Record<string, unknown> & { row_cells: string[]; row_color?: string; row_background?: string; row_bold?: boolean };

/** One version of an entry's trail: the line that wrote it, where one did. */
export interface EntryVersion {
	raw: string | null;
}

/** One version of a spec's history (da-hda4j): the entry, and the number `revert --to-version` takes. */
export interface SpecVersion {
	hid: string;
	version: number;
}

export interface Reads {
	/** `last_used` comes with `--sort recency` alone. */
	fieldsArgv: Omit<FieldListing, 'last_used'>[];
	fieldArgv: Omit<FieldListing, 'last_used'>[];
	recencyArgv: FieldListing[];
	grammarArgv: GrammarEntry[];
	nowArgv: NowListing;
	firstEntryArgv: TailListing;
	lastEntryArgv: TailListing;
	aliasesArgv: Alias[];
	conflictsArgv: Conflicted[];
	refsArgv: Refs;
	tailArgv: TailListing;
	touchedArgv: TailListing;
	geometryArgv: DayviewJson;
	/** Each day's data layer, or its digest, by days back. */
	layersArgv: { pictures?: Record<string, string>; digests?: Record<string, string> };
	showAllArgv: AllFieldsStats | FieldStats;
	specShowArgv: Spec;
	premadeArgv: Premade;
	specHistoryArgv: SpecVersion[];
	entriesHistoryArgv: EntryVersion[] | { versions: EntryVersion[] }[];
	syncStatusArgv: SyncStatus;
	/**
	 * `joining` takes a join cut short, `dissolved` a chain-wide purge, and `label` (da-l341o) a copy in no chain:
	 * no seeded database holds any of them. `sync_chain::labels` holds `label` natively.
	 */
	chainListArgv: Omit<ChainList, 'joining' | 'dissolved' | 'label'>;
	adminListArgv: Admins;
	newInviteArgv: Invite;
	gridArgv: Grid;
	/** The preview's: the `--yes` unjoin answers no JSON. */
	unjoinArgv: PurgePreview;
	restorePreviewArgv: RestorePreview;
	plansArgv: PlanRows;
	backupListArgv: { backups: Listed[] };
	backupToArgv: BackupRun;
	planReachArgv: Reach;
	layoutsArgv: { rows: LayoutRow[] };
	kitsArgv: { rows: KitRow[] };
	kitWidgetArgv: KitWidgetRow[];
	layoutHistoryArgv: LayoutVersion[];
	/** The instant `-t` names, and the length a duration reads as (collect's time section, da-us1n1). */
	timeReadArgv: { at_ms: number };
	statesArgv: { field: string; states: StateUse[] };
	lengthReadArgv: { history_minutes: number };
}
