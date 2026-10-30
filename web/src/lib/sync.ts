// datum.day syncs (da-6kpj5): the page's half of Easy Sync.
//
// ONE ENDPOINT PER DATABASE, WHATEVER THE TABS. Two tabs running one key
// would fight at the relay — the «same endpoint key» two copies are refused
// for (#321) — so a Web Lock per database (`datum-sync:<db>`) elects the tab
// that runs it. The browser releases the lock when that tab closes, and the
// next tab in the lock's queue takes over by itself.
//
// The elected tab — the LEADER — starts #lib/sync.worker.ts, and binds an
// endpoint there if the database syncs: it is in a sync chain. Its store is
// the CLI: the worker asks for `datum sync …` commands
// and they run here, in the database worker, like every other command.
//
// - PUSH on every write to the database, this tab's or any other's
//   (`onAnyChange`): a member whose vector is behind ours is dialed, one
//   session per member at a time, a write during a session making one more.
//   What a session writes itself — the handshake, the ops it applies, how it
//   ended (`record-session`) — is not a write to push on: that dialed again
//   a peer whose dial was still open (da-df6cb). Ops that arrived are pushed
//   on by the session that got them, for the members it did not reach.
// - PULL from every member every `PULL_MS`, which is also what keeps
//   reachability fresh.
// - Ops that ARRIVE are announced to this tab (`announceHere`); the other
//   tabs heard already, from the process that applied them.
// - REACHABILITY is held in memory, never stored: `online` while a session
//   with the peer is open, dialed or served, `reachable` if it answered within `REACHABLE_MS`,
//   else `offline` with when it was last seen. The leader posts it, with the
//   endpoint's id, on the `datum-sync` channel, so every tab shows the same.
// - PAUSED (`datum sync pause`, da-wlbyx), it pushes nothing, and a session
//   that reaches it is refused at the handshake. The state carries when, and
//   the root bar says so in every tab.
// - NOTICES (da-8fucg) — a new device, a new admin, an unlock, until each is
//   acknowledged — ride the state too, read with the setup: at load, after a
//   session that admitted a peer or received ops, and after any `sync`
//   command that wrote (`syncTouched`), an ack among them.
// - UNSYNCED (da-g5jjp): a copy that last synced over an hour before the
//   page opened says when, until the session's first sync lands or its
//   first round of dials is over, an offline peer's failing at the wire's
//   timeout; the day view dims the stretch since. When it last synced is
//   the CLI's, its members' `last_synced` in `sync status`: not `last_seen`,
//   which a handshake moves, so a session cut short read as a sync (da-cthk5).
// - A tab that is not the leader forwards `offer` and `connect` to it.
//   Pairing is the chain's (da-6177s): `offer` writes a chain invite —
//   starting an open chain when there is none — and `connect` redeems one.
// - A ticket travels as a JOIN LINK (da-l9kdr), `/sync/join#<ticket>`, which
//   the CLI's `join` takes too, and `sync chain join` typed on the page
//   comes here (`syncCommand`) rather than to the CLI, which has no network.
//
// A relay other than n0's is named in `localStorage` (`datum.syncRelay`),
// as `DATUM_SYNC_RELAY` names one for the CLI: the tests' local relay. So
// are the tests' short timeouts (`datum.syncTimeoutS`, `shortTimeout`).

import { writable, type Readable } from 'svelte/store';
import { chainListArgv, endpointKeyArgv, newInviteArgv, renameArgv, syncStatusArgv, unsetRenameArgv, vectorArgv } from './calls.js';
import { announceHere, onAnyChange } from './changes.js';
import type { SyncEvent, SyncReport, SyncRequest, TicketAbout } from './syncProtocol.js';
import { cliError, dbOf, type RunResult } from './run.js';
import type { ChainList, Member, Notice, SyncStatus } from './syncView.js';
import { shellSplit } from './shellSplit.js';

const PULL_MS = 30_000;
/** How soon a leader that could not read its setup first reads it again,
 * doubling each time up to `PULL_MS` (da-olsuv). */
const RETRY_MS = 2_000;
/** How a dial given way to a session between the two copies ends: the wire crate's `CARRIED_WHY`. */
const CARRIED = 'is in a session with this copy already';
/** How many of a leader's next reads of its setup fail: a test's lever. */
const FAIL_KEY = 'datum.syncFailRefresh';
/** While set, the worker's `sync pipe apply` is never answered: a test's
 * lever for a join cut short before any op applies (da-38eye). */
const HOLD_KEY = 'datum.syncHoldApply';
/** How many `sync pipe apply` runs go through before the rest are held as
 * `HOLD_KEY` holds them: a join cut short once its count shows (da-dwm2y). */
const HOLD_AFTER_KEY = 'datum.syncHoldApplyAfter';
/** While set to N, this page's `sync pipe apply` runs after its first N
 * since it loaded wait until it is removed, then go on: a test's lever for
 * a sync held under way however fast the runner is (da-rn8fs). */
const GATE_KEY = 'datum.syncGateApplyAfter';
/** While set, the worker's `sync pipe verify` is never answered: a test's
 * lever for a session that has applied and not ended (da-ajk4z). */
const HOLD_VERIFY_KEY = 'datum.syncHoldVerify';
const REACHABLE_MS = 60_000;
/** How long since this copy last synced before the day view dims the stretch since (da-g5jjp). */
export const UNSYNCED_MS = 3_600_000;
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
	/** When its sync was paused, while it is. */
	paused?: string;
	/** How far a session's apply is, while one runs (da-89ujs): `done` of `of` ops. */
	applying?: { done: number; of: number };
	/** Its notices not yet acknowledged (da-8fucg). */
	notices?: Notice[];
	/** When it last synced, over an hour before the page opened, until the session's first sync is over (da-g5jjp). */
	unsynced?: number;
	peers: Record<string, PeerState>;
}

/** A member that can be dialled: seen somewhere. */
type Peer = Member & { addr: string };

/** An invite as `sync new-invite` prints it. */
export interface Invite {
	secret: string;
	/** RFC 3339. */
	expires: string;
	/** Its id, which its redeemer is admitted `via` (da-2bvxq). */
	id: string;
}

type Call = { op: 'offer'; invite?: Invite } | { op: 'connect'; ticket: string; label?: string } | { op: 'sync' };

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

/** `datum <argv>` (with `--db` in it) in the database worker, bypassing the seam's spies;
 * `session`: a run the sync worker's session asked for. */
async function cli(argv: string[], stdin?: string, session?: boolean): Promise<RunResult> {
	const { wasiRun } = await import('./runners/wasi.js');
	return wasiRun(argv, stdin, undefined, undefined, undefined, [], undefined, session);
}

/**
 * Whether a `sync chain list --json` answer is a copy's that syncs: in a
 * chain, joining one, or passing on the purge that dissolved one. One in
 * none answers `chain: null` and nothing more (da-y6oh6); an empty
 * `joining`, were the CLI to print one, is no join under way.
 */
export function syncs(r: RunResult): boolean {
	if (r.code !== 0) return false;
	const l = JSON.parse(r.stdout) as ChainList;
	return Boolean(l.chain || l.joining?.length || l.dissolved);
}

async function must(argv: string[], stdin?: string): Promise<string> {
	const r = await cli(argv, stdin);
	if (r.code !== 0) throw cliError(r.stderr);
	return r.stdout;
}

function syncWorker(): Worker {
	if (worker) return worker;
	worker = new Worker(new URL('./sync.worker.ts', import.meta.url), { type: 'module' });
	worker.onmessage = ({ data }: MessageEvent<SyncEvent>) => {
		if ('run' in data) {
			if (data.argv.includes('apply') && held()) return;
			if (data.argv.includes('verify') && lever(HOLD_VERIFY_KEY)) return;
			const gate = data.argv.includes('apply') ? gated() : Promise.resolve();
			void gate.then(() => cli(data.argv, data.stdin, true)).then((r) => {
				// What a peer sent is this tab's to show once it applies, not
				// once the session's verify, a hash of every row on both
				// sides, is over too: seconds on a big history (da-ajk4z).
				// The other tabs heard it from the process already.
				if (r.changed?.length && data.argv.includes('apply')) announceHere([dbOf(data.argv)]);
				worker?.postMessage({ ran: data.run, result: { code: r.code, stdout: r.stdout, stderr: r.stderr } });
			});
		} else if ('served' in data) {
			led.get(data.served)?.served(data);
		} else if ('serving' in data) {
			led.get(data.serving)?.serving(data.peer, data.on);
		} else if ('progress' in data) {
			led.get(data.progress)?.progress(data.done < data.of ? data : undefined);
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

function lever(key: string): boolean {
	try {
		return localStorage.getItem(key) !== null;
	} catch {
		return false;
	}
}

function held(): boolean {
	try {
		if (localStorage.getItem(HOLD_KEY) !== null) return true;
		const after = localStorage.getItem(HOLD_AFTER_KEY);
		// No lever: nothing held. A lever spent to 0: everything from here.
		if (after === null) return false;
		const n = Number(after);
		if (n <= 0) return true;
		localStorage.setItem(HOLD_AFTER_KEY, String(n - 1));
		return false;
	} catch {
		return false;
	}
}

let applies = 0;
async function gated(): Promise<void> {
	let n: number;
	try {
		n = Number(localStorage.getItem(GATE_KEY) ?? Infinity);
	} catch {
		return;
	}
	if (applies++ < n) return;
	while (lever(GATE_KEY)) await new Promise((r) => setTimeout(r, 100));
}

/** Throws while `FAIL_KEY` counts down (da-zyox0's injection, da-olsuv). */
function failInjected(): void {
	let n = 0;
	try {
		n = Number(localStorage.getItem(FAIL_KEY));
		if (n > 0) localStorage.setItem(FAIL_KEY, String(n - 1));
	} catch {
		return;
	}
	if (n > 0) throw new Error(`${FAIL_KEY}: failed on purpose`);
}

/**
 * Whole seconds for the wire's idle timeout (60 s) and `deliverMs` (20 s)
 * alike, as `DATUM_SYNC_TIMEOUT_S` shortens the CLI's: a test's lever
 * (da-eqjri). Whole, as the wire counts them.
 */
function shortTimeout(): number | undefined {
	try {
		const s = Number(localStorage.getItem('datum.syncTimeoutS'));
		return Number.isInteger(s) && s > 0 ? s : undefined;
	} catch {
		return undefined;
	}
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
	/** The key `endpoint` is bound with: a leave takes a new one (da-j8c4l). */
	key?: string;
	peers: Peer[] = [];
	chain = false;
	/** The chain's id, while there is one. */
	chainId?: string;
	/** In no chain yet, joining one through `peers`: a join cut short (da-idw54). */
	joining = false;
	paused?: string;
	notices: Notice[] = [];
	/** When a session with a member last ended well, as the CLI recorded it. */
	lastSynced = 0;
	unsynced?: number;
	label = 'datum.day';
	/** What the CLI runs (`sync status --json`'s `this`), said to every peer (da-3zqyv). */
	version?: { datum: string; schema: number; floor: number };
	/** Per peer: its vector when last in step, a session in flight (settled when it ends), one more asked for. */
	inStep = new Map<string, string>();
	busy = new Map<string, Promise<void>>();
	/** Per peer: the sessions it dialed that this endpoint is serving now (da-eglw0). */
	serves = new Map<string, number>();
	/** Per peer: who waits for this endpoint to stop serving it. */
	unserved = new Map<string, (() => void)[]>();
	again = new Set<string>();
	seen = new Map<string, number>();
	errors = new Map<string, string>();
	applying?: { done: number; of: number };
	/** A push reading the vector, and a write since it began. */
	pushing = false;
	pushAgain = false;

	// Not a parameter property: node's type stripping, which the pure tests
	// import this through (engine.ts → `syncCommand`), refuses one.
	readonly db: string;
	constructor(db: string) {
		this.db = db;
	}

	/**
	 * Read whether the database syncs, and start an endpoint if it does.
	 * `sync chain list` writes nothing; `sync status` would mint an endpoint
	 * key in a database in no chain, on every fresh device's first boot, so
	 * only a copy that `syncs` is asked it — a member, or a copy joining a
	 * chain, which it names and which has its key (da-idw54). A join
	 * cut short, by a reload say, resumes there: the endpoint starts and
	 * dials the copies it was joining through, and once in the chain the
	 * copy says what it is, as `join` does.
	 *
	 * A read that fails changes nothing (da-zyox0): emptying the peers
	 * first left a failed one pushing to nobody until the next refresh.
	 */
	async refresh(): Promise<void> {
		failInjected();
		const listed = syncs(await cli(['--db', this.db, ...chainListArgv()]));
		const status = listed ? (JSON.parse(await must(['--db', this.db, ...syncStatusArgv()])) as SyncStatus) : undefined;
		const chain = Boolean(status?.chain);
		const joined = this.joining && chain;
		this.joining = listed && !chain;
		this.chain = chain;
		this.chainId = status?.chain ?? undefined;
		this.label = status?.this.label ?? this.label;
		this.version = status?.this ?? this.version;
		this.paused = status?.paused ?? undefined;
		this.notices = status?.notices ?? [];
		this.peers = (status?.members ?? []).filter((p): p is Peer => !p.revoked && !!p.addr);
		this.lastSynced = Math.max(0, ...(status?.members ?? []).map((m) => Date.parse(m.last_synced ?? '') || 0));
		if (listed) await this.start();
		// It joined as the name its admit records: one it was told stays, the
		// suggestion goes for the assembled label, as after a join not cut short.
		const named = this.label === suggestedLabel(this.db) ? unsetRenameArgv() : renameArgv(this.label);
		if (joined) await cli(['--db', this.db, ...named]);
		this.post();
	}

	async start(): Promise<void> {
		const key = (await must(['--db', this.db, ...endpointKeyArgv()])).trim();
		if (this.endpoint && key === this.key) return;
		// Held only once bound: a failed bind is tried again on the next start.
		const { datum, schema, floor } =
			this.version ?? (this.version = (JSON.parse(await must(['--db', this.db, ...syncStatusArgv()])) as SyncStatus).this);
		this.endpoint = await ask<string>(this.db, { op: 'start', key, relay: relay(), idle: shortTimeout(), label: this.label, datum, schema, floor });
		this.key = key;
		this.post();
	}

	/**
	 * Dial every peer whose vector is not ours (`all`: every peer). A burst
	 * of writes reads the vector once more after the one it interrupted,
	 * not once per write.
	 */
	async push(all = false): Promise<void> {
		if (!this.endpoint || !this.peers.length || this.paused) return;
		if (this.pushing) return void (this.pushAgain = true);
		this.pushing = true;
		try {
			do {
				this.pushAgain = false;
				const vector = all ? '' : (await must(['--db', this.db, ...vectorArgv()])).trim();
				for (const p of this.peers) if (all || this.inStep.get(p.endpoint) !== vector) void this.dial(p);
				all = false;
			} while (this.pushAgain);
		} finally {
			this.pushing = false;
		}
	}

	async dial(p: Peer): Promise<void> {
		if (this.busy.has(p.endpoint)) return void this.again.add(p.endpoint);
		let ended = () => {};
		this.busy.set(p.endpoint, new Promise((r) => (ended = r)));
		this.post();
		try {
			this.done(p.endpoint, await ask<SyncReport>(this.db, { op: 'dial', addr: p.addr }));
		} catch (e) {
			this.errors.set(p.endpoint, (e as Error).message);
		} finally {
			this.busy.delete(p.endpoint);
			this.applying = undefined;
			this.post();
			if (this.again.delete(p.endpoint)) void this.push();
			ended();
		}
	}

	/**
	 * Dial every peer once more and wait for it (da-48g3o): after a dial
	 * under way, which may have begun before the peer came up. A dial begun
	 * since that one ended counts as this one: a poll for a free peer lost
	 * the race to the leader's own re-dials for as long as the peer was down.
	 */
	async syncNow(): Promise<void> {
		if (!this.endpoint || this.paused) return;
		await Promise.all(
			this.peers.map(async (p) => {
				// A dial given way to a session between the two copies waits that
				// session out and dials again, as `datum sync` does (da-221fe), so
				// the day view's dim lasts until the sync lands (da-vjhju): the
				// peer's session may be served here, or one it still holds with
				// this copy before a reload, until that connection times out.
				// The wait is bounded, and the next dial is what says the session
				// is over: an end never heard (a dead worker, a rekey) is found
				// out by a dial that no longer gives way.
				const endpoint = this.endpoint;
				for (;;) {
					await this.busy.get(p.endpoint);
					await (this.busy.get(p.endpoint) ?? this.dial(p));
					if (this.paused || this.endpoint !== endpoint || !this.errors.get(p.endpoint)?.endsWith(CARRIED)) return;
					const served = this.serves.has(p.endpoint);
					await new Promise<void>((r) => {
						setTimeout(r, served ? PULL_MS : RETRY_MS);
						if (served) this.unserved.set(p.endpoint, [...(this.unserved.get(p.endpoint) ?? []), r]);
					});
				}
			})
		);
	}

	/** A session's apply moved on; undefined once it is over. */
	progress(applying?: { done: number; of: number }): void {
		this.applying = applying && { done: applying.done, of: applying.of };
		this.post();
	}

	/** A session that ended well, dialed or served. */
	done(id: string, report: SyncReport): void {
		this.unsynced = undefined;
		this.seen.set(id, Date.now());
		this.errors.delete(id);
		this.inStep.set(id, JSON.stringify(report.vector));
		if (report.got) void this.push().catch(() => {});
		// A peer this leader does not list yet: admitted in a session that was
		// cut short (da-idw54), which the leader hears of only once that dead
		// connection times out, ~35 s on.
		const unknown = !this.peers.some((p) => p.endpoint === id);
		// What came in may be a chain record, and with it a notice (da-8fucg).
		if (report.got || report.added || this.joining || unknown) void this.refresh();
	}

	serving(peer: string, on: boolean): void {
		const n = (this.serves.get(peer) ?? 0) + (on ? 1 : -1);
		if (n > 0) this.serves.set(peer, n);
		else {
			this.serves.delete(peer);
			this.unserved.get(peer)?.forEach((r) => r());
			this.unserved.delete(peer);
		}
		this.post();
	}

	served(e: Extract<SyncEvent, { served: string }>): void {
		// A `sync diff` this copy answered moved nothing, and is no session (da-n0ruf).
		if ('report' in e && e.report.diffed) return;
		if ('report' in e) {
			this.done(e.report.id, e.report);
			this.applying = undefined;
		}
		// One that failed may have admitted its peer all the same: a join
		// whose first sync was cut short (da-idw54). It leaves another
		// session's apply shown: a member refused as busy, mid-join, blanked
		// the join's count until its next chunk (da-dwm2y).
		else void this.refresh().catch(() => {});
		this.post();
	}

	async call(c: Call): Promise<unknown> {
		if (c.op === 'sync') return this.syncNow();
		if (c.op === 'connect') {
			await this.start();
			// The admit records the name it joins as, so the inviter holds it at once.
			const label = c.label ?? suggestedLabel(this.db);
			const report = await ask<SyncReport>(this.db, { op: 'connect', ticket: c.ticket, label }).finally(() => this.progress());
			this.done(report.id, report);
			// Admitted to a chain, it says what it is (`datum sync chain join`):
			// a name it was told as told, else none: the chain assembles one (da-xijlw).
			// Its peers hear on the next sync, as natively: pushed on, it dialed
			// the inviter, whose invite has ended, and held the page's one
			// session until the dial timed out (da-qlffb).
			await this.refresh();
			const named = c.label ? renameArgv(c.label) : unsetRenameArgv();
			if (this.chain) await cli(['--db', this.db, ...named], undefined, true);
			return report;
		}
		// An invite the page wrote already (the Sync widget, approved with a
		// passkey in the tab tapped — da-bjxts), or a new one.
		const invite: Invite = c.invite ?? JSON.parse(await must(['--db', this.db, ...newInviteArgv()]));
		// The first invite starts the chain, and with it the endpoint.
		await this.refresh();
		// Any member can admit the redeemer.
		const also = JSON.stringify(this.peers.map((p) => JSON.parse(p.addr)));
		// What the redeemer is asked before it joins (da-l9kdr).
		const expires = Math.floor(Date.parse(invite.expires) / 1000);
		const about = JSON.stringify({ from: this.label, chain: this.chainId, expires } satisfies TicketAbout);
		return ask<string>(this.db, { op: 'ticket', secret: invite.secret, also, about });
	}

	state(): SyncState {
		const now = Date.now();
		const peers: Record<string, PeerState> = {};
		for (const p of this.peers) {
			const lastSeen = this.seen.get(p.endpoint);
			const open = this.busy.has(p.endpoint) || this.serves.has(p.endpoint);
			const reach: Reach = open
				? 'online'
				: lastSeen !== undefined && now - lastSeen < REACHABLE_MS
					? 'reachable'
					: 'offline';
			peers[p.endpoint] = { label: p.label, reach, lastSeen, error: this.errors.get(p.endpoint) };
		}
		const { db, endpoint, paused, applying, notices, unsynced } = this;
		return { db, endpoint, paused, applying, notices, unsynced, peers };
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
	// Until the first read of its setup succeeds, a leader has no endpoint and
	// pushes nothing, so one that failed once — taking over from a closed tab,
	// say — never synced again, every tab's writes with it (da-zyox0). It backs
	// off, so one that never succeeds costs a process per pull, not per 2 s.
	for (let wait = RETRY_MS; ; wait = Math.min(wait * 2, PULL_MS)) {
		try {
			await leader.refresh();
			break;
		} catch (e) {
			console.warn(`sync ${db}:`, e);
			await new Promise((r) => setTimeout(r, wait));
		}
	}
	onAnyChange((session) => session || void leader.push().catch(() => {}), db);
	// A database that does not sync has nothing to pull, nor any reach to age.
	setInterval(() => {
		if (leader.endpoint) void leader.push(true).catch(() => {});
	}, PULL_MS);
	setInterval(() => leader.endpoint && leader.post(), REACHABLE_MS / 4);
	if (leader.peers.length && !leader.paused && leader.lastSynced && Date.now() - leader.lastSynced > UNSYNCED_MS) {
		leader.unsynced = leader.lastSynced;
		leader.post();
	}
	void leader
		.syncNow()
		.catch(() => {})
		.finally(() => {
			leader.unsynced = undefined;
			leader.post();
		});
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
 * A ticket for another copy to redeem (`datum sync chain invite`): for the
 * invite whose `secret` is given, or a new one, starting an open chain when
 * the database is in none.
 */
export function syncOffer(db: string, invite?: Invite): Promise<string> {
	return call(db, { op: 'offer', invite });
}

/** The link a browser opens to redeem `ticket` (da-l9kdr): the ticket in the fragment, which no server sees. */
export function joinLink(ticket: string): string {
	return new URL(`/sync/join#${ticket}`, location.origin).href;
}

/** What a ticket or join link says of itself — who made it, its chain, when it expires; never its secret. */
export function ticketAbout(ticket: string): Promise<TicketAbout> {
	return ask('', { op: 'about', ticket });
}

/** The database a bare `sync chain invite` typed on the page asked an invite of (da-5g0nk), until the Sync widget starts it. */
export const inviting = writable<string | null>(null);

/**
 * `sync chain join <TICKET|LINK>` typed on the page (da-l9kdr): the page's
 * CLI has no network, so the ticket goes to the sync worker (`syncConnect`),
 * and the result says what native `join` says. The argv it echoes holds
 * no ticket. A bare `sync chain invite` (da-5g0nk) waits for a redeemer, which
 * no prompt line can show, so it opens the Sync widget with the invite under
 * way there, as a bare `sync` opens it (da-s9r58). Null for anything
 * else — `--help` and an invite's flags too — which runs as typed.
 */
export function syncCommand(argv: string[]): Promise<RunResult> | null {
	if (words(argv).join(' ') === 'sync') return syncHere(argv);
	if (chainArgs(argv, 'invite')?.length === 0) return inviteHere(argv);
	const [ticket, ...rest] = chainArgs(argv, 'join') ?? [];
	if (!ticket || ticket.startsWith('-') || rest.length) return null;
	const shown = argv.map((w) => (w === ticket ? '<ticket>' : w));
	return syncConnect(dbOf(argv), ticket).then(
		(r) => ({
			argv: shown,
			code: 0,
			stdout: [
				r.left ? `this copy's own sync chain ${r.left[0]} (just itself) is replaced by sync chain ${r.left[1]}` : '',
				r.added ? `added sync peer ${r.label} (${r.id.slice(0, 10)})` : '',
				`${r.label}: sent ${r.sent}, got ${r.got}, conflicts ${r.conflicts}, ${r.verify}`
			]
				.filter(Boolean)
				.join('\n'),
			stderr: ''
		}),
		(e) => ({ argv: shown, code: 1, stdout: '', stderr: `error: ${(e as Error).message}` })
	);
}

/**
 * A bare `sync` (da-s9r58) opens the Sync widget, where its devices stand —
 * or how to start a chain, outside one — and asks the leader for a round,
 * which does nothing outside a chain or while paused.
 */
async function syncHere(argv: string[]): Promise<RunResult> {
	void syncNow(dbOf(argv)).catch(() => {});
	await (await import('./layout/shell.js')).openPlace('sync');
	return { argv, code: 0, stdout: 'sync is in the sync widget: each device and how it stands, or how to start a chain', stderr: '' };
}

async function inviteHere(argv: string[]): Promise<RunResult> {
	inviting.set(dbOf(argv));
	// Here and not at the top: the pure tests import this file, and the layout is the page's.
	await (await import('./layout/shell.js')).openPlace('sync');
	return { argv, code: 0, stdout: 'the invite is in the sync widget: scan its QR with the other device, or open its link there', stderr: '' };
}

/**
 * What a browser joining a chain is called unless told, until it describes
 * itself (da-uyyyg): it has no user or host, so its database's name. Then
 * its label is assembled as every member's is (da-xijlw).
 */
export function suggestedLabel(db: string): string {
	return `datum.day (${db})`;
}

/** Redeem `ticket` and sync (`datum sync chain join [--label]`): `label`, or `suggestedLabel`. */
export function syncConnect(db: string, ticket: string, label?: string): Promise<SyncReport> {
	return call(db, { op: 'connect', ticket, label });
}

/** Sync `db` with every member now, on its leader, and wait for it. */
export function syncNow(db: string): Promise<void> {
	return call(db, { op: 'sync' });
}

/** The databases this tab leads — whose endpoint, if any, is here. */
export function syncLeads(): string[] {
	return [...led.keys()];
}

/** How long a leave or a purge's departure waits for a sync to deliver it (da-2o19r). */
export function deliverMs(): number {
	return (shortTimeout() ?? 20) * 1000;
}

/**
 * True once `db`'s leader, this tab or another, ends a session well after
 * `since` (ms since the epoch); false after `ms` with none, or at once when
 * it has no member to dial.
 */
export function syncedSince(db: string, since: number, ms: number): Promise<boolean> {
	return new Promise((resolve) => {
		// Spent: a session seen meanwhile would answer at once, and a caller's deadline never pass.
		if (ms <= 0) return resolve(false);
		let over = false;
		const end = (synced: boolean) => {
			if (over) return;
			over = true;
			clearTimeout(timer);
			// `subscribe` answers at once, before `stop` is set.
			queueMicrotask(() => stop());
			resolve(synced);
		};
		const timer = setTimeout(() => end(false), ms);
		const stop = states.subscribe((all) => {
			const peers = all[db] && Object.values(all[db].peers);
			if (peers?.some((p) => (p.lastSeen ?? 0) > since)) end(true);
			else if (peers?.length === 0) end(false);
		});
	});
}

/** Say that `db`'s sync setup changed (its chain): its leader reads it again. */
export function syncTouched(db: string): void {
	const leader = led.get(db);
	if (leader) void leader.refresh().catch(() => {});
	else listen().postMessage({ db, touched: true });
}

/** `argv`'s words, `--db` and its name left out wherever they stand. */
function words(argv: string[]): string[] {
	const words: string[] = [];
	for (let i = 0; i < argv.length; i++) {
		if (argv[i] === '--db') i++;
		else if (!argv[i].startsWith('--db=')) words.push(argv[i]);
	}
	return words;
}

/** The words after `sync chain <sub>` in `argv`; null for any other command. */
function chainArgs(argv: string[], sub: string): string[] | null {
	const [sync, chain, verb, ...rest] = words(argv);
	return sync === 'sync' && chain === 'chain' && verb === sub ? rest : null;
}

/**
 * `line` with every word a `sync chain join` could take as its ticket
 * written `<ticket>` — split as `syncCommand` reads it, so a `--db` anywhere
 * hides none — for the scrollback and the recall history, which keep no
 * secret.
 */
export function maskTicket(line: string): string {
	for (const word of chainArgs(shellSplit(line), 'join') ?? []) {
		if (word.startsWith('-')) continue;
		const esc = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
		line = line.replace(new RegExp(`(^|[\\s'"])${esc}(?=$|[\\s'"])`, 'g'), '$1<ticket>');
	}
	return line;
}
