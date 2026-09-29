// The Sync widget's one reader of the CLI (da-x4t0a): every `datum sync`
// the widget reads goes through here, so a renamed command (da-aam46's
// `members` → `chain list`) changes this file and no other.
//
// `sync chain list` goes first: in a database in no chain it fails, writing
// nothing, where `sync status` would mint an endpoint key (as $lib/sync.ts
// says of its own read). So only a member, which has its key, is asked it.
//
// `table` is the rest, and pure: the CLI's rows and the sync worker's
// broadcast reachability ($lib/sync.ts `syncStates`) into devices × databases.

import { run } from './run.js';
import type { Reach, SyncState } from './sync.js';

/** One member of a chain as `sync status --json` has it: this copy left out. */
export interface Member {
	endpoint: string;
	device?: string | null;
	label: string;
	client?: string | null;
	revoked: boolean;
	state: 'synced' | 'pending' | 'never' | 'error';
	to_send: number;
	last_seen?: string | null;
	error?: string | null;
	error_at?: string | null;
}

/** One database's chain, or `chain: null` for one that is in none. */
export interface DbSync {
	db: string;
	chain: string | null;
	this?: { endpoint: string; device: string; label: string };
	members: Member[];
}

/** Every database's chain, read through the CLI. */
export function readSync(dbs: string[]): Promise<DbSync[]> {
	return Promise.all(
		dbs.map(async (db): Promise<DbSync> => {
			const members = await run(['--db', db, 'sync', 'chain', 'list', '--json']);
			if (members.code !== 0) return { db, chain: null, members: [] };
			const status = await run(['--db', db, 'sync', 'status', '--json']);
			if (status.code !== 0) return { db, chain: null, members: [] };
			const s = JSON.parse(status.stdout);
			return { db, chain: s.chain, this: s.this, members: s.members };
		})
	);
}

export type Mark = 'synced' | 'syncing' | 'pending' | 'error' | 'none';

export interface Cell {
	mark: Mark;
	/** Last known, of a device not reachable now: drawn grey, with `at`. */
	stale: boolean;
	/** When the state was last true, ms since the epoch. */
	at?: number;
	/** What the tooltip says: the state in words, `to_send`, the error verbatim. */
	title: string;
	error?: string;
}

export interface Row {
	key: string;
	label: string;
	client?: string;
	/** This device: first, and always online. */
	here: boolean;
	reach: Reach;
	lastSeen?: number;
	cells: Record<string, Cell>;
}

const RANK: Record<Reach, number> = { offline: 0, reachable: 1, online: 2 };
const WORST: Mark[] = ['error', 'syncing', 'pending', 'synced'];
const CLIENT: Record<string, string> = { cli: 'CLI', web: 'datum.day' };
const WORDS: Record<Mark, string> = {
	synced: 'synced',
	syncing: 'syncing now',
	pending: 'pending',
	error: 'error',
	none: 'not in this chain'
};

const ms = (t?: string | null): number | undefined => (t ? Date.parse(t) : undefined);
const later = (a?: number, b?: number) => (a === undefined ? b : b === undefined ? a : Math.max(a, b));

/** Devices × databases: this device first, a device's copies grouped by its `device`. */
export function table(dbs: DbSync[], states: Record<string, SyncState>): Row[] {
	const here: Row = { key: 'this', label: 'this device', client: 'datum.day', here: true, reach: 'online', cells: {} };
	const rows = new Map<string, Row>();
	const mine = new Set(dbs.flatMap((d) => (d.this ? [d.this.device] : [])));
	for (const d of dbs) {
		if (d.this && here.label === 'this device') here.label = d.this.label;
		for (const m of d.members) {
			if (m.revoked || (m.device && mine.has(m.device))) continue;
			const key = m.device ?? m.endpoint;
			const row = rows.get(key) ?? { key, label: m.label, here: false, reach: 'offline' as Reach, cells: {} };
			rows.set(key, row);
			row.client ??= CLIENT[m.client ?? ''];
			const peer = states[d.db]?.peers[m.endpoint];
			if (peer && RANK[peer.reach] > RANK[row.reach]) row.reach = peer.reach;
			row.lastSeen = later(row.lastSeen, later(peer?.lastSeen, ms(m.last_seen)));
			const mark: Mark =
				peer?.reach === 'online' ? 'syncing' : m.state === 'never' ? 'pending' : m.state;
			const words =
				mark !== 'pending' ? WORDS[mark] : m.state === 'never' ? 'never synced' : `pending, ${m.to_send} to send`;
			row.cells[d.db] = {
				mark,
				stale: false,
				at: mark === 'error' ? ms(m.error_at) : ms(m.last_seen),
				title: m.error ? `${words}: ${m.error}` : words,
				error: m.error ?? undefined
			};
		}
		if (d.this) {
			const marks = d.members.filter((m) => !m.revoked).map((m) => rows.get(m.device ?? m.endpoint)?.cells[d.db]?.mark);
			const mark = WORST.find((w) => marks.includes(w)) ?? 'synced';
			here.cells[d.db] = { mark, stale: false, title: `this copy: ${WORDS[mark]}` };
		}
	}
	const all = [here, ...rows.values()];
	for (const row of all)
		for (const d of dbs) {
			const cell = row.cells[d.db];
			if (!cell) row.cells[d.db] = { mark: 'none', stale: false, title: WORDS.none };
			else if (row.reach === 'offline') {
				cell.stale = true;
				cell.title += ' (last known)';
			}
		}
	return all;
}
