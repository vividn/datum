// datum.day syncs (da-6kpj5): the page's half of Easy Sync.
//
// ONE ENDPOINT PER DATABASE, WHATEVER THE TABS. Two tabs running one key
// would fight at the relay — the «same endpoint key» two copies are refused
// for (#321) — so a Web Lock per database (`datum-sync:<db>`) elects the tab
// that runs it. The browser releases the lock when that tab closes, and the
// next tab in the lock's queue takes over by itself.
//
// The elected tab — the LEADER — starts $lib/sync.worker.ts, and binds an
// endpoint there if the database syncs: it is in a sync chain. Its store is
// the CLI: the worker asks for `datum sync …` commands
// and they run here, in the database worker, like every other command.
//
// - PUSH on every write to the database, this tab's or any other's
//   (`onAnyChange`): a member whose vector is behind ours is dialed, one
//   session per member at a time, a write during a session making one more.
// - PULL from every member every `PULL_MS`, which is also what keeps
//   reachability fresh.
// - Ops that ARRIVE are announced to this tab (`announceHere`); the other
//   tabs heard already, from the process that applied them.
// - REACHABILITY is held in memory, never stored: `online` while a session
//   with the peer is open, `reachable` if it answered within `REACHABLE_MS`,
//   else `offline` with when it was last seen. The leader posts it, with the
//   endpoint's id, on the `datum-sync` channel, so every tab shows the same.
// - A tab that is not the leader forwards `offer` and `connect` to it.
//   Pairing is the chain's (da-6177s): `offer` writes a chain invite —
//   starting an open chain when there is none — and `connect` redeems one.
//
// A relay other than n0's is named in `localStorage` (`datum.syncRelay`),
// as `DATUM_SYNC_RELAY` names one for the CLI: the tests' local relay.

import { writable, type Readable } from 'svelte/store';
import { announceHere, onAnyChange } from './changes.js';
import type { SyncEvent, SyncReport, SyncRequest } from './syncProtocol.js';
import type { RunResult } from './run.js';

const PULL_MS = 30_000;
const REACHABLE_MS = 60_000;
const CHANNEL = 'datum-sync';

export type Reach = 'online' | 'reachable' | 'offline';

export interface PeerState {
	label: string;
	reach: Reach;
	/** When it last answered, ms since the epoch. */
	lastSeen?: number;
	/** How the last session with it failed, if it did. */
	error?: string;
}

/** One database's sync, as its leader sees it. */
export interface SyncState {
	db: string;
	/** This copy's endpoint id, while an endpoint runs. */
	endpoint?: string;
	peers: Record<string, PeerState>;
}

/** A member, from `sync status --json`: `addr` is where it was last seen. */
interface Peer {
	endpoint: string;
	label: string;
	addr: string;
	revoked: boolean;
}

type Call = { op: 'offer' } | { op: 'connect'; ticket: string };

const states = writable<Record<string, SyncState>>({});
/** Every database's sync state this tab has heard, from its leader. */
export const syncStates: Readable<Record<string, SyncState>> = states;

let channel: BroadcastChannel | null = null;
/** The databases this tab leads, and what leading one holds. */
const led = new Map<string, Leader>();
const started = new Set<string>();
let worker: Worker | null = null;
let nextId = 0;
const pending = new Map<number, (e: SyncEvent) => void>();

/** `datum <argv>` (with `--db` in it) in the database worker, bypassing the seam's spies. */
async function cli(argv: string[], stdin?: string): Promise<RunResult> {
	const { wasiRun } = await import('./runners/wasi.js');
	return wasiRun(argv, stdin);
}

async function must(argv: string[], stdin?: string): Promise<string> {
	const r = await cli(argv, stdin);
	if (r.code !== 0) throw new Error(r.stderr.replace(/^error: /, ''));
	return r.stdout;
}

function syncWorker(): Worker {
	if (worker) return worker;
	worker = new Worker(new URL('./sync.worker.ts', import.meta.url), { type: 'module' });
	worker.onmessage = ({ data }: MessageEvent<SyncEvent>) => {
		if ('run' in data) {
			void cli(data.argv, data.stdin).then((r) =>
				worker?.postMessage({ ran: data.run, result: { code: r.code, stdout: r.stdout, stderr: r.stderr } })
			);
		} else if ('served' in data) {
			led.get(data.served)?.served(data);
		} else {
			pending.get(data.id)?.(data);
			pending.delete(data.id);
		}
	};
	return worker;
}

/** A request to the worker, before it is addressed. */
type Op = SyncRequest extends infer R ? (R extends { op: string } ? Omit<R, 'id' | 'db'> : never) : never;

function ask<T>(db: string, op: Op): Promise<T> {
	const id = ++nextId;
	return new Promise((resolve, reject) => {
		pending.set(id, (e) => ('ok' in e && e.ok ? resolve(e.value as T) : reject(new Error((e as { error: string }).error))));
		syncWorker().postMessage({ ...op, id, db });
	});
}

function relay(): string | undefined {
	try {
		return localStorage.getItem('datum.syncRelay') ?? undefined;
	} catch {
		return undefined;
	}
}

/** What the leading tab of one database does. */
class Leader {
	endpoint?: string;
	peers: Peer[] = [];
	chain = false;
	label = 'datum.day';
	/** Per peer: its vector when last in step, a session in flight, one more asked for. */
	inStep = new Map<string, string>();
	busy = new Set<string>();
	again = new Set<string>();
	seen = new Map<string, number>();
	errors = new Map<string, string>();
	/** A push reading the vector, and a write since it began. */
	pushing = false;
	pushAgain = false;

	constructor(readonly db: string) {}

	/**
	 * Read whether the database syncs, and start an endpoint if it does.
	 * `sync chain list` fails, writing nothing, in a database in no chain;
	 * `sync status` would mint an endpoint key there, on every fresh
	 * device's first boot, so only a member is asked it.
	 */
	async refresh(): Promise<void> {
		const members = await cli(['--db', this.db, 'sync', 'chain', 'list', '--json']);
		this.chain = members.code === 0;
		this.peers = [];
		if (this.chain) {
			const status = JSON.parse(await must(['--db', this.db, 'sync', 'status', '--json']));
			this.label = status.this.label;
			this.peers = (status.members as Peer[]).filter((p) => !p.revoked && p.addr);
			await this.start();
		}
		this.post();
	}

	async start(): Promise<void> {
		if (this.endpoint) return;
		const key = (await must(['--db', this.db, 'sync', 'endpoint-key'])).trim();
		this.endpoint = await ask<string>(this.db, { op: 'start', key, relay: relay(), label: this.label });
		this.post();
	}

	/**
	 * Dial every peer whose vector is not ours (`all`: every peer). A burst
	 * of writes reads the vector once more after the one it interrupted,
	 * not once per write.
	 */
	async push(all = false): Promise<void> {
		if (!this.endpoint || !this.peers.length) return;
		if (this.pushing) return void (this.pushAgain = true);
		this.pushing = true;
		try {
			do {
				this.pushAgain = false;
				const vector = all ? '' : (await must(['--db', this.db, 'sync', 'pipe', 'vector'])).trim();
				for (const p of this.peers) if (all || this.inStep.get(p.endpoint) !== vector) void this.dial(p);
				all = false;
			} while (this.pushAgain);
		} finally {
			this.pushing = false;
		}
	}

	async dial(p: Peer): Promise<void> {
		if (this.busy.has(p.endpoint)) return void this.again.add(p.endpoint);
		this.busy.add(p.endpoint);
		this.post();
		try {
			this.done(p.endpoint, await ask<SyncReport>(this.db, { op: 'dial', addr: p.addr }));
		} catch (e) {
			this.errors.set(p.endpoint, (e as Error).message);
		} finally {
			this.busy.delete(p.endpoint);
			this.post();
			if (this.again.delete(p.endpoint)) void this.push();
		}
	}

	/** A session that ended well, dialed or served. */
	done(id: string, report: SyncReport): void {
		this.seen.set(id, Date.now());
		this.errors.delete(id);
		this.inStep.set(id, JSON.stringify(report.vector));
		if (report.got) announceHere([this.db]);
		if (report.added) void this.refresh();
	}

	served(e: Extract<SyncEvent, { served: string }>): void {
		if ('report' in e) this.done(e.report.id, e.report);
		this.post();
	}

	async call(c: Call): Promise<unknown> {
		if (c.op === 'connect') {
			await this.start();
			const report = await ask<SyncReport>(this.db, { op: 'connect', ticket: c.ticket });
			this.done(report.id, report);
			// Admitted to a chain, it says what it is (`datum sync chain connect`).
			await this.refresh();
			if (this.chain) await cli(['--db', this.db, 'sync', 'chain', 'rename', this.label]);
			return report;
		}
		const { secret } = JSON.parse(await must(['--db', this.db, 'sync', 'new-invite']));
		// The first invite starts the chain, and with it the endpoint.
		await this.refresh();
		// Any member can admit the redeemer.
		const also = JSON.stringify(this.peers.map((p) => JSON.parse(p.addr)));
		return ask<string>(this.db, { op: 'ticket', secret, also });
	}

	state(): SyncState {
		const now = Date.now();
		const peers: Record<string, PeerState> = {};
		for (const p of this.peers) {
			const lastSeen = this.seen.get(p.endpoint);
			const reach: Reach = this.busy.has(p.endpoint)
				? 'online'
				: lastSeen !== undefined && now - lastSeen < REACHABLE_MS
					? 'reachable'
					: 'offline';
			peers[p.endpoint] = { label: p.label, reach, lastSeen, error: this.errors.get(p.endpoint) };
		}
		return { db: this.db, endpoint: this.endpoint, peers };
	}

	post(): void {
		const state = this.state();
		states.update((all) => ({ ...all, [this.db]: state }));
		channel?.postMessage({ state });
	}
}

/** Lead `db`'s sync for as long as this tab lives. */
async function lead(db: string): Promise<void> {
	const leader = new Leader(db);
	led.set(db, leader);
	await leader.refresh().catch((e) => console.warn(`sync ${db}:`, e));
	onAnyChange(() => void leader.push().catch(() => {}), db);
	// A database that does not sync has nothing to pull, nor any reach to age.
	setInterval(() => {
		if (leader.endpoint) void leader.push(true).catch(() => {});
	}, PULL_MS);
	setInterval(() => leader.endpoint && leader.post(), REACHABLE_MS / 4);
	void leader.push(true).catch(() => {});
	await new Promise(() => {});
}

function listen(): BroadcastChannel {
	if (channel) return channel;
	channel = new BroadcastChannel(CHANNEL);
	channel.onmessage = ({ data }) => {
		if (data.state) states.update((all) => ({ ...all, [data.state.db]: data.state }));
		const leader = led.get(data.db);
		if (!leader) return;
		if (data.ask) leader.post();
		if (data.touched) void leader.refresh().catch(() => {});
		if (data.call)
			leader.call(data.call).then(
				(value) => channel?.postMessage({ reply: data.id, ok: true, value }),
				(e) => channel?.postMessage({ reply: data.id, ok: false, error: (e as Error).message })
			);
	};
	return channel;
}

/** Take part in `db`'s sync: lead it when this tab's turn comes. */
export function startSync(db: string): void {
	if (started.has(db) || typeof navigator === 'undefined' || !navigator.locks) return;
	started.add(db);
	listen().postMessage({ db, ask: true });
	void navigator.locks.request(`datum-sync:${db}`, () => lead(db));
}

/** Run `c` on `db`'s leader, this tab or another — once there is one. */
async function call<T>(db: string, c: Call): Promise<T> {
	startSync(db);
	// The channel never delivers to its sender, so a request is posted only
	// once another tab is seen to lead; `lead` records this one first thing.
	for (;;) {
		const leader = led.get(db);
		if (leader) return leader.call(c) as Promise<T>;
		const { held = [] } = await navigator.locks.query();
		if (held.some((l) => l.name === `datum-sync:${db}`) && !led.has(db)) break;
		await new Promise((r) => setTimeout(r, 50));
	}
	const id = crypto.randomUUID();
	const ch = listen();
	return new Promise((resolve, reject) => {
		const hear = ({ data }: MessageEvent) => {
			if (data.reply !== id) return;
			ch.removeEventListener('message', hear);
			if (data.ok) resolve(data.value);
			else reject(new Error(data.error));
		};
		ch.addEventListener('message', hear);
		ch.postMessage({ db, id, call: c });
	});
}

/**
 * A ticket for another copy to redeem (`datum sync chain invite`): a new chain
 * invite, starting an open chain when the database is in none.
 */
export function syncOffer(db: string): Promise<string> {
	return call(db, { op: 'offer' });
}

/** Redeem `ticket` and sync (`datum sync chain connect`). */
export function syncConnect(db: string, ticket: string): Promise<SyncReport> {
	return call(db, { op: 'connect', ticket });
}

/** The databases this tab leads — whose endpoint, if any, is here. */
export function syncLeads(): string[] {
	return [...led.keys()];
}

/** Say that `db`'s sync setup changed (its chain): its leader reads it again. */
export function syncTouched(db: string): void {
	const leader = led.get(db);
	if (leader) void leader.refresh().catch(() => {});
	else listen().postMessage({ db, touched: true });
}
