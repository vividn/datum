// The Sync widget's one reader of the CLI (da-x4t0a): every `datum sync`
// the widget reads or runs goes through here, built in #lib/calls.ts, so a
// renamed command (da-aam46's `members` → `chain list`) changes that file
// and no other.
//
// `syncAct` runs an act (da-bjxts). In a locked chain the CLI stops at an
// approve link, as it would at a terminal: the passkey approves the record
// the link carries, and `sync approve` writes it — the chain refusing a
// code it would not take.
//
// `sync chain list` goes first: it writes nothing, where `sync status` in a
// database in no chain would mint an endpoint key (as #lib/sync.ts says of
// its own read). So only a copy that syncs (`syncs`), which has its key, is
// asked it.
//
// `table` is the rest, and pure: the CLI's rows and the sync worker's
// broadcast reachability (#lib/sync.ts `syncStates`) into clients × databases.

import { writable } from 'svelte/store';
import { ackArgv, approveArgv, canLeaveArgv, chainListArgv, lastEntryArgv, newInviteArgv, qrArgv, expelArgv, syncStatusArgv } from './calls.js';
import { INTERNAL_DB, isDbName } from './database.js';
import type { TailListing } from './tail.js';
import { dateLabel, localTime } from './clock.js';
import { cliError, run, type RunResult } from './run.js';
import { approve, approveLink } from './passkey.js';
import { syncs, type Invite, type Reach, type SyncState } from './sync.js';
import type { TicketAbout } from './syncProtocol.js';

/** One member of a chain as `sync status --json` has it: this copy left out. */
export interface Member {
	endpoint: string;
	client?: string | null;
	label: string;
	kind?: string | null;
	revoked: boolean;
	/** Where it was last seen, which sync.ts dials. */
	addr?: string | null;
	/** Every op it signed set aside (da-ry11c): revoked too. */
	purged?: boolean;
	/** A reader (da-9yvb6): it receives, and writes nothing. */
	reader?: boolean;
	/**
	 * A filtered reader's fields, and how many the database had (da-u81u1).
	 * @shallow a reader takes a second device, which no seeded database is.
	 */
	filter?: Filter | null;
	/** `offline`: its last session never reached it, which is no fault (da-ueb4p). */
	state: 'synced' | 'pending' | 'never' | 'offline' | 'error';
	to_send: number;
	last_seen?: string | null;
	/** When a session with it last ended well; `last_seen` moves at a handshake too (da-cthk5). */
	last_synced?: string | null;
	error?: string | null;
	error_at?: string | null;
}

export interface Filter {
	/** None: every field. */
	fields?: string[];
	of?: number;
	/** How far back it reaches, in seconds (da-nflkk). */
	window?: number;
	/** Sent every spec and alias too. */
	specs?: boolean;
	/** «filtered: 5 of 23 fields», in the CLI's words. */
	said?: string;
}

/** «filtered: 5 of 23 fields», as the CLI says it. */
export const filtered = (f: Filter): string => f.said ?? `filtered: ${f.fields?.length} of ${f.of} fields`;

export interface Admin {
	key: string;
	label: string;
}

/** `sync chain list --json` and `sync admin list --json`. */
export interface Admins {
	admins: Admin[];
}

/**
 * `sync chain list --json`: `chain` null in no chain (da-y6oh6), and then
 * `joining` while a join is under way, or `dissolved` once a purge dissolved it.
 */
export interface ChainList extends Admins {
	chain: string | null;
	members: Listed[];
	joining?: { endpoint: string; label: string }[];
	dissolved?: string;
	/** A join kept until a member admits it (da-4qpwf), or refused for good: never its ticket. */
	pending_join?: PendingJoin;
	/** In no chain: what the chain its first share starts will name this device (da-l341o). */
	label?: string;
}

/** A join this copy waits on a member to admit (`pending_join`, da-4qpwf). */
export interface PendingJoin {
	/** The inviting device's label, as its link names it. */
	from?: string | null;
	chain?: string | null;
	/** When its invite expires, unix seconds. */
	expires?: number | null;
	since: string;
	/** A member's refusal no retry changes: the words, and then the join waits no more. */
	refused?: string | null;
}

/** A member as `sync chain list` has it: what admitted it, and when (da-2bvxq). */
export interface Listed {
	label: string;
	/** `genesis`, or `invite <id>`. */
	via: string;
	admitted: string;
	rejoined: string | null;
}

/**
 * A new member or admin, an unlock, or a device whose ops were refused, not
 * yet acknowledged (`sync chain ack`). `member` is every member a copy found
 * on joining, as one (da-o9xdd); `refused` goes once the device is admitted.
 */
export interface Notice {
	subject: string;
	kind: 'endpoint' | 'member' | 'rejoin' | 'admin' | 'unlock' | 'wipe' | 'refused' | 'unwritten';
	label: string;
	/** What `sync chain ack` takes for it. */
	who: string;
	/** The device it is about, which an expel names. */
	endpoint?: string | null;
	/** When, RFC 3339: a wipe's, the order's instant (da-pvoai). */
	at: string;
	/** The CLI's own words: who, by label, in local time (da-o9xdd). */
	text: string;
}

/** `n`, a notice of `db`'s, as the root bar's dialog says it: the CLI's words. */
export const noticeWords = (db: string, n: Notice): string => `«${db}»: ${n.text.charAt(0).toUpperCase()}${n.text.slice(1)}`;

/** One database's chain, or `chain: null` for one that is in none. */
export interface DbSync {
	db: string;
	chain: string | null;
	this?: {
		endpoint: string;
		client: string;
		label: string;
		/** The build, schema and sync floor this copy runs (da-ryhmi), which sync.ts says to every peer (da-3zqyv). */
		datum: string;
		schema: number;
		floor: number;
		reader?: boolean;
		/** `member`, `reader` or `writer` (da-o5o6m), in a chain. */
		role?: 'member' | 'reader' | 'writer' | null;
		/** @shallow as a member's. */
		filter?: Filter | null;
		revoked?: string | null;
	};
	members: Member[];
	admins?: Admin[];
	/** `sync chain list`'s members, revoked ones too. */
	listed?: Listed[];
	/** When this copy's sync was paused, while it is. */
	paused?: string | null;
	notices?: Notice[];
	/**
	 * Devices whose changes wait for this copy's clock to catch up with theirs (da-ns8dq).
	 * @shallow it takes a second device, ahead of this one.
	 */
	clock?: { endpoint: string; text: string }[];
	/** A join this copy waits on, in no chain yet (da-4qpwf). */
	pending?: PendingJoin;
	/** In no chain: what its chain will name this device, `ChainList`'s `label`. */
	label?: string;
}

/**
 * `sync status --json`: a `DbSync` but for the database's name and the admins, which `sync chain list` has, and its
 * pending join, which it names `pending_join` (da-4qpwf).
 * `this` is always there: only a `DbSync` made for a database in no chain lacks it.
 */
export type SyncStatus = Omit<DbSync, 'db' | 'admins' | 'listed' | 'pending' | 'label'> & Required<Pick<DbSync, 'this'>> & { pending_join?: PendingJoin | null };

/**
 * A link's `#` held in this tab's `sessionStorage` as well as in memory
 * (da-amhrv): a first visit reloads once for isolation (+layout.svelte)
 * after the route has already dropped the `#`, and the card must survive
 * it. Never a query or a request — the tab's own storage. `drop` forgets
 * the stored copy of a link that has been used, leaving the card shown.
 */
function heldInTab(key: string) {
	let start: string | null = null;
	try {
		start = sessionStorage.getItem(key);
	} catch {}
	const store = writable(start);
	const drop = () => {
		try {
			sessionStorage.removeItem(key);
		} catch {}
	};
	store.subscribe((v) => {
		try {
			if (v === null) drop();
			else sessionStorage.setItem(key, v);
		} catch {}
	});
	return { ...store, drop };
}

/** The approve link datum.day was opened on (`/sync/approve#<record>`), until it is dealt with. */
export const approving = heldInTab('datum-sync-approving');

/** The ticket of the join link datum.day was opened on (`/sync/join#<ticket>`, da-l9kdr), until it is dealt with. */
export const joining = heldInTab('datum-sync-joining');

/** The `datum sync login` link datum.day was opened on (`/sync/login#<login>`, da-mzqs4), until it is dealt with. */
export const loggingIn = heldInTab('datum-sync-login');

/**
 * The code a `datum sync login` link shows (da-mzqs4), which the CLI prints
 * beside it: the first 16 hex characters of the device key it names, read
 * from the link's base32 JSON, in groups of 4 (`8a88 e3dd 7409 f195`, da-ntp5c).
 * The CLI's `chain::login_code` must say the same. Empty for a link that does not read.
 */
export function loginCode(link: string): string {
	const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
	const bits = [...(link.split('#').pop() ?? '').toUpperCase()].map((c) => A.indexOf(c).toString(2).padStart(5, '0')).join('');
	try {
		const bytes = Uint8Array.from(bits.match(/.{8}/g) ?? [], (b) => parseInt(b, 2));
		return String(JSON.parse(new TextDecoder().decode(bytes)).addr.id)
			.slice(0, 16)
			.replace(/(.{4})(?=.)/g, '$1 ');
	} catch {
		return '';
	}
}

/** What joining said, kept across the load that opens the database it joined (da-bedyz). */
export const joinedInto = heldInTab('datum-sync-joined');

/**
 * What joining with a ticket that says `a` of itself does, in words, for
 * `db`, the database chosen, whose chain `d` reads (da-l9kdr), or a new one
 * (`fresh`, da-bedyz) — and, when it cannot, why not: the link expired, or
 * the database is in a chain already (a copy belongs to one) that it cannot
 * leave. One that holds nothing but its own data can (`alone`), and joining
 * replaces its chain (da-h7pnc). One in the link's own chain joins it
 * again: a revoked device rejoins so (da-j8c4l). One in another chain it
 * cannot leave is `other`: no refusal, the card offering a new database to
 * join with instead (da-48kc3).
 */
export function joinWords(
	a: TicketAbout,
	db: string,
	d: DbSync | undefined,
	now: number,
	alone = false,
	fresh = false
): { words: string[]; refused?: string; other?: boolean } {
	const from = a.from ? `«${a.from}»` : 'another device';
	const chain = a.chain ? ` (chain ${a.chain.slice(0, 10)})` : '';
	const words = [
		fresh
			? `Join the sync chain of ${from}${chain} with a new database «${db}» in this browser: it will hold what the chain holds.`
			: `Join the sync chain of ${from}${chain} with this browser’s database «${db}»: what each holds, both will.`
	];
	const until = a.expires && new Date(a.expires * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
	if (a.expires && a.expires * 1000 <= now) return { words, refused: `This link expired at ${until}: ask ${from} for a new one.` };
	if (d?.chain && d.chain === a.chain)
		words.push(
			d.this?.revoked
				? `«${db}» was expelled from this chain: joining again admits it back, and what it logged since goes to the others.`
				: `«${db}» is in this chain already: joining again admits it back, as after an expel.`
		);
	else if (d?.chain && alone) words.push(`«${db}»’s own sync chain, just this browser, is replaced by this one.`);
	// Merging two chains is to come (vividn, 2026-11-10): meanwhile the card offers a new database.
	else if (d?.chain) return { words: [...words, `«${db}» is in another sync chain, and a database belongs to one: join with a new database instead.`], other: true };
	if (a.role === 'reader') words.push(readOnly(a.fields));
	if (a.role === 'writer') words.push(writeOnly(a.fields));
	if (until) words.push(`The link works once, until ${until}.`);
	return { words };
}

/** What a reader's invite admits, in the person's words (da-wm9wb): `fields`, a filtered one's. */
const readOnly = (fields?: string[]): string =>
	fields?.length
		? `Read only, of ${fields.map((f) => `«${f}»`).join(', ')}: you’ll receive what they log of those; nothing you log in it goes to them.`
		: 'Read only: you’ll receive what they log here; nothing you log in it goes to them.';

/**
 * What a writer's invite admits (da-o5o6m): it writes as a member does, and passes on nothing of anyone else's;
 * it logs only its fields, where it names any, and changes only its own entries (da-mogvf).
 */
const writeOnly = (fields?: string[]): string =>
	(!fields
		? 'As a writer: what you log goes to them, and you’ll receive what they log, passing none of it on.'
		: fields.length
			? `As a writer, of ${fields.map((f) => `«${f}»`).join(', ')}: what you log of those goes to them, and you’ll receive what they log of those.`
			: 'As a writer: what you log goes to them, and you’ll receive nothing of theirs.') +
	' You change only the entries you log.';

/**
 * The ticket in what Enter code was given: a bare code, a join link, or the
 * CLI command that carries one. None for a link with no ticket in its
 * fragment: a ticket holds no «/».
 */
export function ticketOf(typed: string): string {
	const word = (typed.trim().split(/\s+/).pop() ?? '').replace(/^['"]|['"]$/g, '');
	const at = word.indexOf('#');
	return at >= 0 ? word.slice(at + 1) : word.includes('/') ? '' : word;
}

/** Why `name` cannot be a new database's, where `taken` are this browser's: none when it can (da-bedyz). */
export function newDbRefusal(name: string, taken: string[]): string | undefined {
	if (!name) return 'Name the new database.';
	if (name === INTERNAL_DB || !isDbName(name)) return `«${name}» cannot be a database’s name: no «/», no leading «.», no «.db» at its end.`;
	if (taken.includes(name)) return `A database «${name}» is here already: choose it from the list instead.`;
}

/** A new database's name from the inviter's database's name or label (`from`), as a name none of `taken` has (da-bedyz, da-wm9wb). */
export function newDbName(from: string | undefined, taken: string[]): string {
	const base =
		(from ?? '')
			.toLowerCase()
			.replace(/[^a-z0-9_-]+/g, '-')
			.replace(/^-+|-+$/g, '') || 'shared';
	let name = base;
	for (let n = 2; taken.includes(name) || newDbRefusal(name, taken); n++) name = `${base}-${n}`;
	return name;
}

/** Whether `db` holds an entry, meta left out: joining it would mix its own with the chain's (da-bedyz). */
export async function holdsEntries(db: string): Promise<boolean> {
	const r = await run(['--db', db, ...lastEntryArgv()]);
	return r.code === 0 && ((JSON.parse(r.stdout) as Partial<TailListing>).rows?.length ?? 0) > 0;
}

/** The device that redeemed invite `id` of `d`'s, and when it joined: none while the invite is unused. */
export function redeemer(d: DbSync | undefined, id: string): { label: string; at: string } | undefined {
	const m = d?.listed?.find((m) => m.via === `invite ${id}`);
	return m && { label: m.label, at: m.rejoined ?? m.admitted };
}

/** Whether `db` may leave its chain to join another: it holds nothing but its own data (da-h7pnc). */
export async function canLeave(db: string): Promise<boolean> {
	return (await run(['--db', db, ...canLeaveArgv()])).code === 0;
}

/** Every database's chain, read through the CLI. */
export function readSync(dbs: string[]): Promise<DbSync[]> {
	return Promise.all(
		dbs.map(async (db): Promise<DbSync> => {
			const members = await run(['--db', db, ...chainListArgv()]);
			const list = members.code === 0 ? (JSON.parse(members.stdout) as ChainList) : undefined;
			const pending = list?.pending_join ?? undefined;
			if (!syncs(members)) return { db, chain: null, members: [], pending, label: list?.label };
			const status = await run(['--db', db, ...syncStatusArgv()]);
			if (status.code !== 0) return { db, chain: null, members: [] };
			const s = JSON.parse(status.stdout) as SyncStatus;
			const { admins, members: listed } = JSON.parse(members.stdout) as ChainList;
			return { db, chain: s.chain, this: s.this, members: s.members, admins, listed, paused: s.paused, notices: s.notices, clock: s.clock, pending };
		})
	);
}

export type Mark = 'synced' | 'syncing' | 'pending' | 'offline' | 'error' | 'none';

export interface Cell {
	mark: Mark;
	/** Last known, of a client not reachable now: drawn grey, with `at`. */
	stale: boolean;
	/** When the state was last true, ms since the epoch. */
	at?: number;
	/** When a session with it last ended well (`last_synced`); this copy's, the latest of its members'. */
	synced?: number;
	/** What the tooltip says: the state in words, `to_send`, the error verbatim. */
	title: string;
	error?: string;
	/** That copy is a reader: drawn with the lock. */
	reader?: boolean;
	/** A filtered reader: drawn with the filter, so an empty field reads as «not shared». */
	filter?: Filter;
}

export interface Row {
	key: string;
	label: string;
	/** `cli` or `web` in words: what the client is. */
	kind?: string;
	/** This client: first, and always online. */
	here: boolean;
	/** Its copy's endpoint in each database's chain, while not revoked. */
	endpoints: Record<string, string>;
	/** Every copy of it is revoked: the row stays, and says so. */
	revoked: boolean;
	reach: Reach;
	lastSeen?: number;
	cells: Record<string, Cell>;
}

const RANK: Record<Reach, number> = { offline: 0, reachable: 1, online: 2 };
const WORST: Mark[] = ['error', 'syncing', 'pending', 'synced'];
const KIND: Record<string, string> = { cli: 'CLI', web: 'datum.day' };
const WORDS: Record<Mark, string> = {
	synced: 'synced',
	syncing: 'syncing now',
	pending: 'pending',
	offline: 'offline',
	error: 'error',
	none: 'not in this chain'
};

const ms = (t?: string | null): number | undefined => (t ? Date.parse(t) : undefined);
const later = (a?: number, b?: number) => (a === undefined ? b : b === undefined ? a : Math.max(a, b));

/** Clients × databases: this client first, a client's copies grouped by its `client`. */
export function table(dbs: DbSync[], states: Record<string, SyncState>): Row[] {
	const here: Row = { key: 'this', label: 'this client', kind: 'datum.day', here: true, reach: 'online', endpoints: {}, revoked: false, cells: {} };
	const rows = new Map<string, Row>();
	const mine = new Set(dbs.flatMap((d) => (d.this ? [d.this.client] : [])));
	for (const d of dbs) {
		if (d.this && here.label === 'this client') here.label = d.this.label;
		if (d.this) here.endpoints[d.db] = d.this.endpoint;
		for (const m of d.members) {
			if (m.client && mine.has(m.client)) continue;
			const key = m.client ?? m.endpoint;
			const row = rows.get(key) ?? { key, label: m.label, here: false, reach: 'offline' as Reach, endpoints: {}, revoked: true, cells: {} };
			rows.set(key, row);
			if (m.revoked) {
				row.cells[d.db] = { mark: 'none', stale: false, title: m.purged ? 'unjoined' : 'expelled' };
				continue;
			}
			row.revoked = false;
			row.endpoints[d.db] = m.endpoint;
			row.kind ??= KIND[m.kind ?? ''];
			const peer = states[d.db]?.peers[m.endpoint];
			if (peer && RANK[peer.reach] > RANK[row.reach]) row.reach = peer.reach;
			row.lastSeen = later(row.lastSeen, later(peer?.lastSeen, ms(m.last_seen)));
			const mark: Mark =
				peer?.reach === 'online' ? 'syncing' : m.state === 'never' ? 'pending' : m.state;
			const words =
				mark !== 'pending' ? WORDS[mark] : m.state === 'never' ? 'never synced' : `pending, ${m.to_send} to send`;
			// Offline, why is the same every time: the words stay in the tooltip,
			// and the reason under the line is a fault's (da-ueb4p).
			const title = m.error ? `${words}: ${m.error}` : words;
			const error = m.state === 'offline' ? undefined : (m.error ?? undefined);
			row.cells[d.db] = {
				mark,
				stale: false,
				at: mark === 'error' ? ms(m.error_at) : ms(m.last_seen),
				synced: ms(m.last_synced),
				title: `${title}${m.reader ? ', read-only' : ''}${m.filter ? `, ${filtered(m.filter)}` : ''}`,
				error,
				reader: m.reader,
				filter: m.filter ?? undefined
			};
		}
		if (d.this) {
			// A member that is down may not have this copy's changes yet: pending, as a whole.
			const marks: (Mark | undefined)[] = d.members
				.filter((m) => !m.revoked)
				.map((m) => rows.get(m.client ?? m.endpoint)?.cells[d.db]?.mark)
				.map((m) => (m === 'offline' ? 'pending' : m));
			const mark = WORST.find((w) => marks.includes(w)) ?? 'synced';
			const f = d.this.filter ?? undefined;
			const title = `this copy: ${WORDS[mark]}${d.this.reader ? ', read-only' : ''}${f ? `, ${filtered(f)}` : ''}`;
			const synced = d.members.filter((m) => !m.revoked).reduce<number | undefined>((t, m) => later(t, ms(m.last_synced)), undefined);
			here.cells[d.db] = { mark, stale: false, title, synced, reader: d.this.reader, filter: f };
		}
	}
	const all = [here, ...rows.values()];
	for (const row of all)
		for (const d of dbs) {
			const cell = row.cells[d.db];
			if (!cell) row.cells[d.db] = { mark: 'none', stale: false, title: WORDS.none };
			else if (row.reach === 'offline' && !row.revoked) {
				cell.stale = true;
				cell.title += ' (last known)';
			}
		}
	return all;
}

/** How long before `now` `at` was, in its largest whole unit: «now», «2m», «3h», «4d» (da-dxng1). */
export function ago(at: number, now: number): string {
	const s = Math.max(0, now - at) / 1000;
	if (s < 60) return 'now';
	const [n, unit] = s < 3600 ? [s / 60, 'm'] : s < 86400 ? [s / 3600, 'h'] : [s / 86400, 'd'];
	return `${Math.floor(n)}${unit}`;
}

/** A device's standing in one chain, as `sync grid --json` has it (da-n0uj1.3). */
export type Standing = 'member' | 'reader' | 'writer' | 'expelled' | 'unjoined' | 'wiping' | 'wiped';

/** A grant not yet taken, as this install can read it (da-pofki): `word` its cell's, `text` the CLI's sentence. */
export interface GridGrant {
	state: 'waiting' | 'expired' | 'sealed';
	word: string;
	text: string;
}

/** A device's standing in one chain, as `sync grid --json` has it. */
export interface GridCell {
	state: Standing;
	endpoint: string;
	/**
	 * It and the column's database cannot sync: one runs a schema below the
	 * other's sync floor, and `updates` says which must update (da-52vdj).
	 * Membership crosses a floor; entries wait.
	 * @shallow a floor takes a second copy on another schema, which no seeded database is.
	 */
	apart: { updates: 'this' | 'it'; schema: number; this_schema: number; floor: number; words: string } | null;
}

/** `sync grid --json`: devices × the chains this install holds, each column by chain id; `held: false`, a chain granted to this device and held nowhere here. */
export interface Grid {
	device: string | null;
	chains: { chain: string; db: string; held: boolean }[];
	devices: {
		/** Its device key; null for a copy bound to none, which is its `endpoint` alone. */
		device: string | null;
		endpoint: string | null;
		label: string;
		this: boolean;
		cells: Record<string, GridCell | null>;
		/**
		 * By chain, the grant its column shows.
		 * @shallow a grant takes a second device, which no seeded database is.
		 */
		grants?: Record<string, GridGrant>;
		/**
		 * Grants to it this install holds and cannot open: of what, it does not know.
		 * @shallow as `grants`.
		 */
		sealed?: GridGrant[];
	}[];
}

/**
 * Why this device may not change who syncs `d` (da-dxng1), empty where it may:
 * a reader writes nothing, a writer no chain record (da-o5o6m), and a chain locked by admins is changed only by
 * one — a passkey (`es256:`, `eddsa:`), which the act asks for, or `account`,
 * the key this tab is signed in to. A key file (`ed25519:`) signs at a terminal.
 */
export function barred(d: Pick<DbSync, 'db' | 'this' | 'admins'> | undefined, account?: string): string {
	if (!d) return '';
	if (d.this?.reader) return `This device only reads ${d.db}: a device that writes it can change who syncs it.`;
	if (d.this?.role === 'writer') return `This device only logs to ${d.db}: a member can change who syncs it.`;
	const admins = d.admins ?? [];
	if (admins.length && !admins.some((a) => !a.key.startsWith('ed25519:') || a.key === account))
		return `${d.db}’s chain is locked: only its admin (${admins.map((a) => a.label).join(', ')}) can change who syncs it, at a terminal.`;
	return '';
}

/** What a cell offers a device standing so: a role (an invite, or a role change), or a destructive act. */
export function cellActs(s: Standing | undefined, bound: boolean): string[] {
	const live = s === 'member' || s === 'reader' || s === 'writer';
	const roles = live || bound ? (['member', 'reader'] as const).filter((r) => r !== s) : [];
	const acts = live ? ['expel', 'unjoin', 'wipe'] : s === 'expelled' ? ['unjoin', 'wipe'] : s === 'unjoined' ? ['wipe'] : [];
	return [...roles, ...acts];
}

/** A device's acts from its menu (da-jlyyr): the databases each reaches, and why it reaches none or not all. */
export interface DeviceActs {
	/** Rename and remove: each database here it is in, by its endpoint there, that this device may change. */
	change: { db: string; endpoint: string }[];
	changeWhy: string;
	/** Sync all: each database here it is not in, nor invited to, that this device may invite it to. */
	add: { db: string; chain: string | null }[];
	addWhy: string;
}

/**
 * What a device's menu offers (da-jlyyr). vividn, 2026-11-10: «you shouldn't be able to for example rename if you
 * only have read access--some of these obviously require admin access». A database barred (`barred`) is left out
 * and its reason said; an invite is sealed to the device's key and carried by a chain it shares with this device,
 * which a reader writes nothing in.
 */
export function deviceActs(row: Grid['devices'][number], chains: Grid['chains'], dbs: DbSync[], account?: string): DeviceActs {
	const why = (ds: (DbSync | undefined)[]) => [...new Set(ds.map((d) => barred(d, account)).filter(Boolean))].join(' ');
	const ins = chains.flatMap((c) => {
		const cell = row.cells[c.chain];
		return c.held && cell && ['member', 'reader', 'writer'].includes(cell.state) ? [{ db: c.db, endpoint: cell.endpoint, d: dbs.find((d) => d.db === c.db) }] : [];
	});
	const change = ins.filter((x) => !barred(x.d, account)).map(({ db, endpoint }) => ({ db, endpoint }));
	const changeWhy = ins.length ? why(ins.filter((x) => barred(x.d, account)).map((x) => x.d)) : `${row.label} is in no database here yet.`;
	const left = dbs.filter((d) => !ins.some((x) => x.db === d.db) && !(d.chain && row.grants?.[d.chain]?.state === 'waiting'));
	const carried = !!row.device && ins.some((x) => x.d && !x.d.this?.reader && x.d.this?.role !== 'writer');
	const add = carried ? left.filter((d) => !barred(d, account)).map(({ db, chain }) => ({ db, chain })) : [];
	const addWhy = !row.device
		? `${row.label} has no device key here, which an invite is sealed to.`
		: !ins.length
			? `${row.label} is in no database here yet, so nothing carries an invite to it.`
			: !carried
			? `This device only reads what it shares with ${row.label}: an invite reaches it through a database this device writes.`
			: !left.length
				? `${row.label} already syncs every database here.`
				: why(left.filter((d) => barred(d, account)));
	return { change, changeWhy, add, addWhy };
}

/** The confirm of removing `label` from `dbs`: what happens to whose data (da-fstz5: a lost device is unjoined). */
export const expelWords = (label: string, dbs: string[]): string => {
	const list = dbs.length > 1 ? `${dbs.slice(0, -1).join(', ')} and ${dbs.at(-1)}` : dbs[0];
	return `Expel ${label} from ${list}? It can no longer sync ${dbs.length > 1 ? 'them' : list} until a new invite admits it again. It keeps its own copy, and what it wrote stays on every device. If it is lost or someone else may hold it, Unjoin it instead, which sets aside everything it wrote: a device not yet told of the removal still takes its writes dated before it.`;
};

/** The wipe's confirm: what happens to whose data, and its limit said plainly (vividn: «a popup dialog should explain what this means»). */
export const wipeWords = (label: string, db: string): string =>
	`Wipe ${label}’s copy of ${db}? It is expelled now, and asked to delete its own copy of ${db} when it next reaches a device of this chain. A device that never comes back online cannot be made to delete anything: until it obeys, this is only an expel, and it keeps what it holds. What it synced stays on every other device; what it wrote since it last synced goes with its copy.`;

/** What `sync chain expel <who> --unjoin --json` says an unjoin would withdraw (da-ry11c). */
export interface PurgePreview {
	created: number;
	edited: number;
	deleted: number;
	/** The specs and other definitions it signed, which go too (da-sivjb). */
	defined?: number;
	first?: string | null;
	last?: string | null;
}

/** The unjoin's confirm, naming the device and what goes, before anything is done. */
export function unjoinWords(label: string, db: string, p: PurgePreview): string {
	const day = (t?: string | null) => (t ? dateLabel(localTime(new Date(t))) : '');
	const many = (n: number, one: string, more: string) => `${n} ${n === 1 ? one : more}`;
	// As the CLI's unjoin preview: a zero count says nothing, and the noun rides the first count.
	const did = ([[p.created, 'created'], [p.edited, 'edited'], [p.deleted, 'deleted']] as const)
		.filter(([n]) => n > 0)
		.map(([n, verb], i) => `${i ? n : many(n, 'entry', 'entries')} it ${verb}`);
	if (p.defined) did.push(`${many(p.defined, 'definition', 'definitions')} it signed`);
	const list = did.length > 1 ? `${did.slice(0, -1).join(', ')} and ${did.at(-1)}` : did[0];
	const what = p.first
		? `Every change it made goes, on every device: ${list}, ${day(p.first)} to ${day(p.last)}.`
		: p.defined
			? `It made no change to an entry, only ${many(p.defined, 'definition', 'definitions')}: this expels it and sets ${p.defined === 1 ? 'that' : 'them'} aside.`
			: 'It made no change to an entry: this only expels it.';
	return `Unjoin ${label} from ${db}? ${what} It is expelled too: it syncs no more, though it keeps its own copy, until a fresh invite brings it and its changes back.`;
}

/**
 * `datum --db <db> <argv>`, a `sync` act, approved with a passkey when the
 * chain is locked: the stdout of both runs, `new-invite`'s secret being the
 * first's.
 */
export async function syncAct(db: string, argv: string[]): Promise<RunResult> {
	const r = await run(['--db', db, ...argv], '');
	const link = r.code !== 0 && approveLink(r.stderr);
	if (!link) return r;
	const done = await run(['--db', db, ...approveArgv(link)], await approve(link));
	return { ...done, stdout: [r.stdout.trimEnd(), done.stdout].filter(Boolean).join('\n') };
}

/** Acknowledge `n` in `db`: every unlock by the word, anything else by its subject. */
export const ackNotice = (db: string, n: Notice): Promise<RunResult> =>
	syncAct(db, ackArgv(n.who));

/** Expel `endpoint`, the device a notice of `db`'s is about: «not yours?» (da-o9xdd). */
export const expelDevice = (db: string, endpoint: string): Promise<RunResult> => syncAct(db, expelArgv(endpoint));

/** Write `link`'s record into `db`, signed with `code`. */
export const approveInto = (db: string, link: string, code: string): Promise<RunResult> =>
	run(['--db', db, ...approveArgv(link)], code);

/** A new invite on `db` (starting an open chain on a db in none), a reader's if `reader`: its secret, and when it expires. */
export async function newInvite(db: string, reader = false): Promise<Invite> {
	const r = await syncAct(db, newInviteArgv(reader));
	if (r.code !== 0) throw cliError(r.stderr);
	return JSON.parse(r.stdout.split('\n')[0]);
}

/** `text` as a QR code, SVG, drawn by the CLI. */
export async function qr(text: string): Promise<string> {
	return (await run(qrArgv(text))).stdout;
}

const OWN_KEYS = 'datum.syncPasskeys';

/** The admin keys of the passkeys made in this browser (`add admin`). */
export function ownKeys(): Set<string> {
	try {
		return new Set(JSON.parse(localStorage.getItem(OWN_KEYS) ?? '[]'));
	} catch {
		return new Set();
	}
}

/** Remember `keys` as passkeys made in this browser. */
export function markOwn(keys: string[]): void {
	try {
		localStorage.setItem(OWN_KEYS, JSON.stringify([...ownKeys(), ...keys]));
	} catch {
		// Not remembered: its approval card says it is not this browser's.
	}
}

/**
 * What approving `r` does, in words, resolved against `d`, the held copy
 * of its chain, if any (da-bjxts): a link is anyone's to send, so the card
 * names the device, the database and the key before the passkey signs.
 */
export function approvalWords(r: Record<string, unknown>, d: DbSync | undefined, own: Set<string>): string[] {
	const where = d ? `in ${d.db}` : 'in a chain this browser holds no copy of';
	const short = (id: unknown) => String(id ?? '').slice(0, 10);
	const who = (ep: unknown) => {
		const self = d?.this;
		if (self && self.endpoint === ep) return `this browser, ${self.label} (${short(ep)})`;
		const m = d?.members.find((m) => m.endpoint === ep);
		return m ? `${m.label} (${short(ep)})` : `an unknown device (${short(ep)})`;
	};
	const key = String(r.key ?? '');
	const [kind, rest] = key.includes(':') ? key.split(':', 2) : ['key', key];
	const print = `${kind} ${rest.slice(0, 12)}…`;
	switch (r.type) {
		// The stored record types keep their names: `revoke` is an expel, `purge` an unjoin (da-n0uj1.1).
		case 'revoke':
			return [`Expel ${who(r.endpoint)} ${where}: it syncs no more, keeping its copy; what it wrote stays`];
		case 'purge':
			return [`Unjoin ${who(r.endpoint)} ${where}: every change it made goes, on every device`];
		// No longer written (da-q4rau), but an older CLI's link may ask for one.
		case 'unpurge':
			return [`Unpurge ${who(r.endpoint)} ${where}: its changes come back`];
		case 'role':
			return [`Make ${who(r.endpoint)} a ${r.role} ${where}`];
		case 'describe':
			return [`Rename ${who(r.endpoint)} to «${r.label}» ${where}`];
		case 'add_admin':
			return [
				`Add admin «${r.label}» (${print}) ${where}`,
				own.has(key) ? 'A passkey made in this browser.' : 'Not a passkey made in this browser: approve only if you know whose it is.'
			];
		case 'remove_admin': {
			const admin = d?.admins?.find((a) => a.key === key);
			const words = [`Remove admin ${admin ? `«${admin.label}»` : 'an unknown key'} (${print}) ${where}`];
			if (r.unlock) words.push('This unlocks the chain: any device can add, expel or rename devices.');
			return words;
		}
		case 'genesis': {
			const admins = (r.admins as { label?: string }[] | undefined) ?? [];
			return [`Start sync chain ${String(r.chain ?? '').slice(0, 10)}, its admins ${admins.map((a) => `«${a.label}»`).join(', ') || 'none'}`];
		}
		case 'invite': {
			const caps = r.caps as { role?: string } | undefined;
			return [`Invite a new device as a ${caps?.role === 'reader' ? 'reader' : 'member'} ${where}`];
		}
		default:
			return [`A ${String(r.type)} record ${where}`];
	}
}
