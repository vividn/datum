// The Sync widget's one reader of the CLI (da-x4t0a): every `datum sync`
// the widget reads or runs goes through here, so a renamed command
// (da-aam46's `members` → `chain list`) changes this file and no other.
//
// `syncAct` runs an act (da-bjxts). In a locked chain the CLI stops at an
// approve link, as it would at a terminal: the passkey approves the record
// the link carries, and `sync approve` writes it — the chain refusing a
// code it would not take.
//
// `sync chain list` goes first: in a database in no chain it fails, writing
// nothing, where `sync status` would mint an endpoint key (as $lib/sync.ts
// says of its own read). So only a member, which has its key, is asked it.
//
// `table` is the rest, and pure: the CLI's rows and the sync worker's
// broadcast reachability ($lib/sync.ts `syncStates`) into clients × databases.

import { writable } from 'svelte/store';
import { run, type RunResult } from './run.js';
import { approve, approveLink } from './passkey.js';
import type { Reach, SyncState } from './sync.js';

/** One member of a chain as `sync status --json` has it: this copy left out. */
export interface Member {
	endpoint: string;
	client?: string | null;
	label: string;
	kind?: string | null;
	revoked: boolean;
	/** A reader (da-9yvb6): it receives, and writes nothing. */
	reader?: boolean;
	/** A filtered reader's fields, and how many the database had (da-u81u1). */
	filter?: Filter | null;
	state: 'synced' | 'pending' | 'never' | 'error';
	to_send: number;
	last_seen?: string | null;
	error?: string | null;
	error_at?: string | null;
}

export interface Filter {
	fields: string[];
	of: number;
}

/** «filtered: 5 of 23 fields», as the CLI says it. */
export const filtered = (f: Filter): string => `filtered: ${f.fields.length} of ${f.of} fields`;

export interface Admin {
	key: string;
	label: string;
}

/** A new member or admin, or an unlock, not yet acknowledged (`sync chain ack`). */
export interface Notice {
	subject: string;
	kind: 'endpoint' | 'admin' | 'unlock';
	label: string;
	text: string;
}

/** One database's chain, or `chain: null` for one that is in none. */
export interface DbSync {
	db: string;
	chain: string | null;
	this?: { endpoint: string; client: string; label: string; reader?: boolean; filter?: Filter | null };
	members: Member[];
	admins?: Admin[];
	/** When this copy's sync was paused, while it is. */
	paused?: string | null;
	notices?: Notice[];
}

/** The approve link datum.day was opened on (`/sync/approve#<record>`), until it is dealt with. */
export const approving = writable<string | null>(null);

/** Every database's chain, read through the CLI. */
export function readSync(dbs: string[]): Promise<DbSync[]> {
	return Promise.all(
		dbs.map(async (db): Promise<DbSync> => {
			const members = await run(['--db', db, 'sync', 'chain', 'list', '--json']);
			if (members.code !== 0) return { db, chain: null, members: [] };
			const status = await run(['--db', db, 'sync', 'status', '--json']);
			if (status.code !== 0) return { db, chain: null, members: [] };
			const s = JSON.parse(status.stdout);
			const { admins } = JSON.parse(members.stdout);
			return { db, chain: s.chain, this: s.this, members: s.members, admins, paused: s.paused, notices: s.notices };
		})
	);
}

export type Mark = 'synced' | 'syncing' | 'pending' | 'error' | 'none';

export interface Cell {
	mark: Mark;
	/** Last known, of a client not reachable now: drawn grey, with `at`. */
	stale: boolean;
	/** When the state was last true, ms since the epoch. */
	at?: number;
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
				row.cells[d.db] = { mark: 'none', stale: false, title: 'revoked' };
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
			const title = m.error ? `${words}: ${m.error}` : words;
			row.cells[d.db] = {
				mark,
				stale: false,
				at: mark === 'error' ? ms(m.error_at) : ms(m.last_seen),
				title: `${title}${m.reader ? ', read-only' : ''}${m.filter ? `, ${filtered(m.filter)}` : ''}`,
				error: m.error ?? undefined,
				reader: m.reader,
				filter: m.filter ?? undefined
			};
		}
		if (d.this) {
			const marks = d.members.filter((m) => !m.revoked).map((m) => rows.get(m.client ?? m.endpoint)?.cells[d.db]?.mark);
			const mark = WORST.find((w) => marks.includes(w)) ?? 'synced';
			const f = d.this.filter ?? undefined;
			const title = `this copy: ${WORDS[mark]}${d.this.reader ? ', read-only' : ''}${f ? `, ${filtered(f)}` : ''}`;
			here.cells[d.db] = { mark, stale: false, title, reader: d.this.reader, filter: f };
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

/**
 * `datum --db <db> sync <argv>`, approved with a passkey when the chain is
 * locked: the stdout of both runs, `new-invite`'s secret being the first's.
 */
export async function syncAct(db: string, argv: string[]): Promise<RunResult> {
	const r = await run(['--db', db, 'sync', ...argv], '');
	const link = r.code !== 0 && approveLink(r.stderr);
	if (!link) return r;
	const done = await run(['--db', db, 'sync', 'approve', link], await approve(link));
	return { ...done, stdout: r.stdout + done.stdout };
}

/** Write `link`'s record into `db`, signed with `code`. */
export const approveInto = (db: string, link: string, code: string): Promise<RunResult> =>
	run(['--db', db, 'sync', 'approve', link], code);

/** A new invite on `db` (starting an open chain on a db in none): its secret. */
export async function newInvite(db: string): Promise<string> {
	const r = await syncAct(db, ['new-invite']);
	if (r.code !== 0) throw new Error(r.stderr.replace(/^error: /, ''));
	return JSON.parse(r.stdout.split('\n')[0]).secret;
}

/** `text` as a QR code, SVG, drawn by the CLI. */
export async function qr(text: string): Promise<string> {
	return (await run(['sync', 'qr', text])).stdout;
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
		case 'revoke':
			return [`Revoke ${who(r.endpoint)} ${where}`];
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
			if (r.unlock) words.push('This unlocks the chain: any device can add, revoke or rename devices.');
			return words;
		}
		case 'invite': {
			const caps = r.caps as { role?: string } | undefined;
			return [`Invite a new device as a ${caps?.role === 'reader' ? 'reader' : 'member'} ${where}`];
		}
		default:
			return [`A ${String(r.type)} record ${where}`];
	}
}
