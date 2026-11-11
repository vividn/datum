<!--
	The Sync widget (da-x4t0a), redone as a view with its actions under it
	(da-yoef7). vividn: «you have a nice view, and then actions for changing
	sync setup and stuff are on the lower side of the widget together.»

	Top, this device (da-5wlnr): its name, and under it, smaller, its
	person's @username, both edited in place under its ✎; beside them one
	line that is true of the page's database: «only on this device» while no
	other device has it, «✓ synced», or how many devices it has not reached.
	vividn, 2026-11-10: «at the top, let's put device name, and then a
	pencil icon next to it, which let's you edit it».

	Under it, the grid (da-n0uj1.7), the only list of devices: every device
	this install's databases know of against the page's database, the others
	behind «other dbs ›» (da-opbpa). A database in no chain is drawn too, with
	this device's row: there is no «Turn on sync», its chain starts as it is
	first shared. A cell is the device's standing there as `sync grid` has it,
	the state of its last sync before it, and a dropdown of what may be done
	to it — a role (an invite, sealed to the device in a chain both share, or a
	role change), expel, unjoin, wipe, each of the last three confirming in a
	dialog that says what happens to whose data. An expelled or unjoined
	device is not drawn at all: «completely expelled/removed devices should not
	appear in the sync grid at all». Only the CLI's commands: the invite's
	ticket is the sync worker's, as [Add device]'s is.

	Bottom, every act together: [Add device], and for a database in a chain
	pause, leave and the admins (da-bjxts). Under them, in every state, the
	passkey account (da-nf1p4): [Sign in with a passkey] and [New passkey], or
	«Signed in» and [Sign out]. The header's `?` opens the guide to all of it
	(da-oyvsz).
	[Add device], or `sync chain invite` typed on the page (da-5g0nk), opens one panel — the QR, until when it works,
	and its own buttons at its bottom — with the ticket under a closed
	«join from a terminal». Once the invite is used, or expired, the QR and
	link go: the panel names the device that joined, or says it expired, and
	offers [Add another device] (da-2bvxq). The cards a link opens — `/sync/approve#…`,
	which names what the passkey signs before it offers the passkey, since
	a link is anyone's to send, and `/sync/join#…` (da-l9kdr), which names
	the inviting device, its chain and the page's database before it joins
	or says why it cannot — are the same shape, on top. A join kept until a
	member admits this device (da-4qpwf) says under the header that it waits
	for its first sync, or why a member refused it, in place of [Add device];
	a link works for days, so its panel says until which day. Notices of new
	members and unlocks stand with the acts until acknowledged.

	Every CLI read and act is #lib/syncView.ts's. Every button is the
	commands widget's `button.command` (app.css): the browser's own grey
	under the page's pale ink had been a contrast of 1.1.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { resolve as routed } from '$app/paths';
	import { onDestroy, untrack } from 'svelte';
	import { get } from 'svelte/store';
	import Pane from '#lib/Pane.svelte';
	import { onAnyChange } from '#lib/changes.ts';
	import { currentDb, INTERNAL_DB, withDb } from '#lib/database.ts';
	import { listDatabases } from '#lib/databases.ts';
	import { deliverMs, inviting, joinLink, pendingWords, suggestedLabel, syncConnect, syncedSince, syncNow, syncOffer, syncSignIn, syncStates, ticketAbout, type Invite, type SyncState } from '#lib/sync.ts';
	import {
		ackNotice,
		approvalWords,
		approveInto,
		approving,
		canLeave,
		holdsEntries,
		joinedInto,
		joining,
		joinWords,
		newDbName,
		newDbRefusal,
		markOwn,
		newInvite,
		ownKeys,
		unjoinWords,
		cellActs,
		wipeWords,
		qr,
		readSync,
		redeemer,
		syncAct,
		table,
		type Admin,
		type Admins,
		type DbSync,
		type Grid,
		type GridCell,
		cellWords,
		type Cell,
		type Mark,
		type Notice,
		type PurgePreview
	} from '#lib/syncView.ts';
	import type { TicketAbout } from '#lib/syncProtocol.ts';
	import { approve, approveRecords, createPasskey, NO_PRF, prfOffered } from '#lib/passkey.ts';
	import { account, setAccount } from '#lib/account.ts';
	import { ask, confirm } from '#lib/confirm.ts';
	import { cliError, QUIET_MS, settled, type RunResult } from '#lib/run.ts';
	import {
		adminAddArgv,
		adminListArgv,
		adminRemoveArgv,
		labelCheckArgv,
		leaveArgv,
		pausedArgv,
		unjoinArgv,
		renameArgv,
		unsetRenameArgv,
		expelArgv,
		grantArgv,
		gridArgv,
		roleArgv,
		userArgv,
		wipeArgv
	} from '#lib/calls.ts';
	import { deviceName, setDeviceName, setUsername, username } from '#lib/clientId.ts';
	import { act } from '#lib/act.ts';
	import { now } from '#lib/clock.ts';
	import { explainGuide } from '#lib/help.ts';

	let {
		epoch = 0,
		otherDbs = false,
		onchange
	}: {
		epoch?: number;
		/** The grid's other databases unfolded, held for the session (da-opbpa). */
		otherDbs?: boolean;
		onchange?: (otherDbs: boolean) => void;
	} = $props();

	const SYMBOL: Record<Mark, string> = { synced: '✓', syncing: '↻', pending: '◔', offline: '○', error: '!', none: '—' };
	/** A standing still in its chain: an expelled, unjoined or wiped device is drawn nowhere (da-5wlnr). */
	const LIVE = ['member', 'reader'];
	const canShare = browser && typeof navigator.share === 'function';

	/** The page's database: the one this widget is about. */
	const db = browser ? currentDb() : '';
	let dbs = $state.raw<DbSync[]>([]);
	/** Devices × chains, as `sync grid` has it; and what the CLI said of the last grid act. */
	let grid = $state.raw<Grid | null>(null);
	let said = $state('');
	let reading = false;
	let again = false;
	/** Writes heard to the databases listed (`later`), and how many of them the latest read began after. */
	let heard = 0;
	let begun = 0;
	let stops: (() => void)[] = [];
	/** An act under way, and how the last one failed. */
	let busy = $state(false);
	let failure = $state('');
	/** What an act under way waits on, in words. */
	let waiting = $state('');
	/** This device's name and username as kept, and as edited in place under ✎ (da-5wlnr). */
	let kept = $state(browser ? { name: deviceName(), user: username() } : { name: '', user: '' });
	/** `was`: the name the edit began from, so a rename that lands meanwhile is not read as this one. */
	let edit = $state<{ name: string; user: string; was: string } | null>(null);
	/** The join link of the last device invited, its QR, until when it works, its invite's id, and whether it names this device's relay. */
	let offer = $state<{ db: string; link: string; svg: string; until: string; expires: number; id: string; relay: boolean } | null>(null);
	/** The approval card's outcome: written here, or the code to paste. */
	let approval: { done?: string; code?: string } = $state({});
	/**
	 * The card's record resolved against the held copy of its chain, before
	 * the passkey is offered: which database (none held: the code), and what
	 * approving it does, in words.
	 */
	let card = $state<{ link: string; db?: string; words: string[] } | null>(null);

	/**
	 * The join card (da-l9kdr): the ticket of the link the page was opened
	 * on and what it says of itself, the database it joins — one of this
	 * browser's (`target`) or a new one named `fresh` (da-bedyz), each with its
	 * chain and whether it may leave it — and the name it joins as, none
	 * typed being the suggestion (da-uyyyg). Not a join link: `refused`.
	 */
	let join = $state<{
		ticket: string;
		about?: TicketAbout;
		dbs: { db: string; d: DbSync; alone: boolean }[];
		target: string | null;
		fresh: string;
		named?: string;
		user: string;
		refused?: string;
		done?: string;
	} | null>(null);
	/** The database the card joins, as chosen. */
	const into = $derived(join ? (join.target ?? join.fresh.trim()) : '');
	/** What joining `into` does, or why it cannot, in words, before anything is dialed. */
	const joinSaid = $derived.by((): { words: string[]; refused?: string } => {
		if (!join?.about) return { words: [], refused: join?.refused };
		if (join.target === null) {
			const said = joinWords(join.about, into, undefined, Date.now(), false, true);
			const refused = said.refused ?? newDbRefusal(into, join.dbs.map((c) => c.db));
			return { ...said, refused };
		}
		const c = join.dbs.find((c) => c.db === into);
		return joinWords(join.about, into, c?.d, Date.now(), c?.alone);
	});
	// The page's database is the one a join opens on; what a join into another said is kept across that load,
	// and shown once: the store is cleared as it is read, so no later mount shows it again.
	let joinedSaid = $state(browser ? get(joinedInto) : null);
	if (browser) joinedInto.set(null);

	$effect.pre(() => {
		const asked = $approving;
		if (browser) untrack(() => void resolve(asked));
	});
	$effect.pre(() => {
		const asked = $joining;
		if (browser) untrack(() => void resolveJoin(asked));
	});
	// `sync chain invite` typed on the page (da-5g0nk): the offer [Add device] makes.
	$effect.pre(() => {
		const asked = $inviting;
		if (browser && asked)
			untrack(() => {
				void readSync([asked]).then(([d]) => addDevice(asked, d?.chain ? undefined : startName(asked)));
				inviting.set(null);
			});
	});

	// A used or expired invite admits no one (da-2bvxq): its panel says so in place of the QR.
	const joined = $derived(offer && redeemer(dbs.find((d) => d.db === offer?.db), offer.id));
	const expired = $derived(!!offer && !joined && +$now >= offer.expires);

	$effect.pre(() => {
		const at = epoch;
		if (browser) untrack(() => void read(at));
	});
	const cur = $derived(dbs.find((d) => d.db === db) ?? { db, chain: null, members: [] });
	/** Each device's last sync in each database it is in: a grid cell's state beside its standing. */
	const rows = $derived(table(dbs, $syncStates));
	const overall = $derived(standing(cur, $syncStates));
	/** This device as named in the page's database's chain, else as kept here, else as its chain would assemble it. */
	const named = $derived(cur.this?.label ?? (kept.name || suggestedLabel(db)));
	/** The other devices holding the page's database, and those of them its last changes have not reached. */
	const peers = $derived(rows.filter((r) => !r.here && r.endpoints[db]));
	const unreached = $derived(peers.filter((r) => !['synced', 'syncing'].includes(r.cells[db].mark)).length);
	/** The one line beside the name: never «synced» with no one to sync with. */
	const status = $derived.by((): { mark: Mark; words: string } => {
		if (cur.chain && cur.paused) return { mark: 'none', words: 'paused' };
		if (!peers.length) return { mark: 'none', words: 'only on this device' };
		if (unreached) return { mark: overall.mark, words: `${SYMBOL[overall.mark]} ${unreached} device${unreached === 1 ? '' : 's'} not reached` };
		return { mark: overall.mark, words: `${SYMBOL[overall.mark]} ${overall.mark === 'syncing' ? 'syncing' : 'synced'}` };
	});
	/** The grid's columns: every chain, then each database in none, which only a copy fills. */
	const columns = $derived([
		...(grid?.chains ?? []),
		...dbs.filter((d) => !d.chain && !grid?.chains.some((c) => c.held && c.db === d.db)).map((d) => ({ chain: null, db: d.db, held: true }))
	]);
	/**
	 * The columns drawn: the page's database first, its chain's or its own,
	 * then the others as above once «other dbs ›» unfolds them (da-opbpa).
	 */
	// `sync grid` names a chain's column after the page's database where it holds it, the others' after theirs.
	const here = $derived(columns.filter((c) => c.held && c.db === db));
	const drawn = $derived(otherDbs ? [...here, ...columns.filter((c) => !here.includes(c))] : here);
	type GridRow = Grid['devices'][number];
	/**
	 * The grid's rows: this device first, drawn where no chain names it yet,
	 * then every other still in a chain or offered one; an expelled or
	 * unjoined device is drawn nowhere.
	 */
	const devices = $derived.by((): GridRow[] => {
		if (!grid) return [];
		const me = grid.devices.find((r) => r.this) ?? { device: grid.device, endpoint: null, label: named, this: true, cells: {} };
		const live = grid.devices.filter((r) => !r.this && (Object.values(r.cells).some((c) => c && LIVE.includes(c.state)) || Object.keys(r.grants ?? {}).length || r.sealed?.length));
		return [me, ...live];
	});

	/** A device's last sync in a column's database, as its row in `table` has it: none where it is not in it. */
	function lastSync(row: GridRow, col: { db: string }, cell: GridCell | null): Cell | undefined {
		const r = row.this ? rows.find((r) => r.here) : cell ? rows.find((r) => r.endpoints[col.db] === cell.endpoint) : undefined;
		const c = r?.cells[col.db];
		return c && c.mark !== 'none' ? c : undefined;
	}

	/** How `d` stands as a whole: the worst of its devices, which this copy's cell already is. */
	function standing(d: DbSync, states: Record<string, SyncState>): { mark: Mark; reader?: boolean; filter?: unknown } {
		if (!d.chain) return { mark: 'none' };
		return table([d], states)[0].cells[d.db];
	}

	async function databases(): Promise<string[]> {
		const names = (await listDatabases()).filter((n) => n !== INTERNAL_DB);
		return names.includes(db) ? names : [db, ...names];
	}

	async function read(_epoch?: number): Promise<void> {
		if (reading) return void (again = true);
		reading = true;
		try {
			do {
				again = false;
				begun = heard;
				const listed = await databases();
				if (listed.join() !== dbs.map((d) => d.db).join()) {
					stops.forEach((stop) => stop());
					stops = listed.map((db) => onAnyChange(() => void later(), db));
				}
				// Not the JSON asked for (a process killed mid-write): the view stays as it was.
				dbs = await readSync(listed).catch(() => dbs);
				// After every database is read, so the worker holds each one's file for the grid to open.
				grid = await syncAct(db, gridArgv()).then((r) => (r.code === 0 ? (JSON.parse(r.stdout) as Grid) : grid), () => grid);
			} while (again);
		} finally {
			reading = false;
		}
	}

	/**
	 * Every write to a database listed, read once the page has answered and
	 * been quiet a beat past a layout behind's second (`lag`, Layout.svelte),
	 * unless a read began since (da-77hqt). A write the page's epoch carries
	 * has been read on it by then, in front at once and behind after that
	 * second; what is left is a write the epoch does not carry: to another
	 * database, a sync session's own run, a quiet one. So every write is read
	 * by a read begun after it was heard: the epoch's, its own, or a later
	 * write's. A read of every database at each write, the moment it landed,
	 * made a tap in collect wait on them with this layout behind.
	 */
	async function later(): Promise<void> {
		const mine = ++heard;
		await settled();
		await new Promise((resolve) => setTimeout(resolve, QUIET_MS + 100));
		if (mine === heard && begun < mine) void read();
	}

	onDestroy(() => stops.forEach((stop) => stop()));

	async function resolve(link: string | null): Promise<void> {
		// A new link is a new card: the last one's outcome goes with it.
		card = null;
		approval = {};
		const records = link && approveRecords(link);
		if (!link || !records) return;
		// A batch approves at once (da-7bjai): every record's words. One starting a chain is held by no copy here.
		const d = (await readSync(await databases())).find((d) => d.chain && d.chain === records[0].chain && records[0].type !== 'genesis');
		if (link === $approving) card = { link, db: d?.db, words: records.flatMap((r) => approvalWords(r, d, ownKeys())) };
	}

	async function resolveJoin(ticket: string | null): Promise<void> {
		join = null;
		if (!ticket) return;
		const about = await ticketAbout(ticket).catch(() => undefined);
		const names = await databases();
		const dbs = await Promise.all((await readSync(names)).map(async (d) => ({ db: d.db, d, alone: !!d.chain && (await canLeave(d.db)) })));
		// Joining a database that holds entries mixes them with the chain's: a new one is chosen then (da-bedyz).
		const target = (await holdsEntries(db)) ? null : db;
		const refused = about ? undefined : 'Not a join link: it carries no ticket.';
		if (ticket === $joining) join = { ticket, about, dbs, target, fresh: newDbName(about?.from, names), user: username(), refused };
	}

	/**
	 * How far a join is: its apply's `done` of `of` ops, once the ops are in.
	 * Each step holds the database, so a command typed meanwhile waits for it.
	 */
	const joinProgress = (a?: SyncState['applying']) =>
		a
			? `Joining: ${a.done.toLocaleString()} of ${a.of.toLocaleString()} ops applied… A command typed meanwhile waits for the step under way.`
			: 'Joining…';

	const joinHere = (j: NonNullable<typeof join>, into: string) =>
		doing(async () => {
			// A name typed is kept as typed; the suggestion, untouched, grows a
			// short id on a collision. Either is refused before the join as
			// `--label` is.
			const typed = j.named?.trim() && j.named !== suggestedLabel(into) ? j.named : undefined;
			ok(await syncAct(into, labelCheckArgv(typed ?? suggestedLabel(into))));
			// The username is this browser's, which every chain it enters is told (da-xza37).
			setUsername(j.user);
			const r = await syncConnect(into, j.ticket, typed);
			joining.drop();
			// No member answered: the join is kept, and the first sync starts once one is online (da-4qpwf).
			// Across a sync floor it joined, and no entry moved (da-52vdj).
			const from = j.about?.from ?? 'the inviting device';
			const done = r.pending
				? `${r.said ? `${r.said.charAt(0).toUpperCase()}${r.said.slice(1)}. ` : ''}Joined: syncing starts when ${from} or another device in the chain is online.`
				: r.floor
					? `Joined. ${r.floor}`
					: `Joined: ${r.got} op(s) came in from ${r.label}, ${r.sent} went to it.`;
			if (into === db) return void (join = { ...j, done });
			// The page opens on the database joined, so what came in is what is shown (da-bedyz).
			joinedInto.set(done);
			location.assign(withDb(routed('/sync/join'), into));
		});

	async function doing(what: () => Promise<unknown>): Promise<void> {
		busy = true;
		failure = '';
		try {
			await what();
		} catch (e) {
			failure = (e as Error).message;
		} finally {
			busy = false;
			void read();
		}
	}

	function ok(r: RunResult): RunResult {
		if (r.code !== 0) throw cliError(r.stderr);
		return r;
	}

	// A declaration, hoisted: the effect above runs as it is created, before this line.
	function addDevice(db: string, named?: string): Promise<void> {
		return doing(async () => {
			if (named) ok(await syncAct(db, labelCheckArgv(named)));
			// The ticket waits for this device to reach its relay, up to 30 s (da-kfhty).
			waiting = 'Making the invite…';
			try {
				const invite = await newInvite(db);
				// The chain it started describes this device: a name typed is its label.
				if (named) ok(await syncAct(db, renameArgv(named)));
				const ticket = await syncOffer(db, invite);
				const { relay = false } = await ticketAbout(ticket);
				const link = joinLink(ticket);
				const expires = Date.parse(invite.expires);
				offer = { db, link, svg: await qr(link), until: when(expires), expires, id: invite.id, relay };
			} finally {
				waiting = '';
			}
		});
	}

	const unedit = (e: KeyboardEvent): void => void (e.key === 'Escape' && (edit = null));

	/** The name a first invite starts `db`'s chain with: the one kept here, unless it is the suggestion. */
	const startName = (db: string): string | undefined => (kept.name && kept.name !== suggestedLabel(db) ? kept.name : undefined);

	const share = (link: string) => navigator.share({ title: 'Join a datum sync chain', url: link }).catch(() => {});

	/**
	 * ✎'s edit kept: this device renamed in every database it syncs, a client
	 * being one name, and kept here for each chain it starts; the username
	 * beside it this browser's in every chain (da-xza37). An empty name is
	 * none: the chain assembles one (da-xijlw).
	 */
	const saveName = (e: { name: string; user: string; was: string }) =>
		doing(async () => {
			const [name, user] = [e.name.trim(), e.user.trim()];
			const [renamed, told] = [name !== e.was.trim(), user !== kept.user];
			if (renamed && name) ok(await syncAct(db, labelCheckArgv(name)));
			setUsername(user);
			if (renamed) setDeviceName(name);
			kept = { name: deviceName(), user: username() };
			if (renamed || told) for (const d of dbs) if (d.this) ok(await syncAct(d.db, !renamed ? userArgv(user) : name ? renameArgv(name) : unsetRenameArgv()));
			edit = null;
		});

	const expel = (label: string, endpoint: string, d = db) =>
		doing(async () => {
			const words = `Expel ${label} from ${d}? It can no longer sync ${d} until a new invite admits it again. It keeps its own copy, and what it wrote stays on every device.`;
			if (!(await confirm(words, 'expel'))) return;
			ok(await syncAct(d, expelArgv(endpoint)));
		});

	/** An unjoin previews first (da-ry11c): the confirm names the device and what goes. */
	const unjoin = (label: string, endpoint: string, d = db) =>
		doing(async () => {
			const preview = JSON.parse(ok(await syncAct(d, unjoinArgv(endpoint))).stdout) as PurgePreview;
			if (!(await confirm(unjoinWords(label, d, preview), 'unjoin', label))) return;
			ok(await syncAct(d, unjoinArgv(endpoint, true)));
		});

	/** A wipe takes typing the device's name, as an unjoin does. */
	const wipe = (label: string, endpoint: string, d: string) =>
		doing(async () => {
			if (!(await confirm(wipeWords(label, d), 'wipe', label))) return;
			said = ok(await syncAct(d, wipeArgv(endpoint))).stdout.trim();
		});

	/**
	 * `device` invited to `d` as `role`, the invite's ticket made here as [Add device]'s is: what the CLI says of it.
	 * Each chain the device is live in here is synced, so the invite reaches it now if it is online: the
	 * CLI picks one of them as the carrier (one this copy writes to) and does not say which, so all are
	 * pushed, and one that fails is a device away, which the invite waits for in the chain.
	 */
	async function granting(d: string, device: string, role: 'member' | 'reader', name?: string, made?: Invite): Promise<string> {
		waiting = 'Making the invite…';
		try {
			const invite = made ?? (await newInvite(d, role === 'reader'));
			const said = ok(await syncAct(d, grantArgv(device, role, await syncOffer(d, invite), invite.id, name))).stdout.trim();
			const row = grid?.devices.find((r) => r.device === device);
			for (const c of grid?.chains ?? []) if (c.db !== d && ['member', 'reader'].includes(row?.cells[c.chain]?.state ?? '')) void syncNow(c.db).catch(() => {});
			return said;
		} finally {
			waiting = '';
		}
	}

	/** A cell's dropdown chose `what`: a role is an invite, or a role change for a live member; the rest confirm first. */
	function cellAct(row: GridRow, col: { chain: string | null; db: string }, what: string): void {
		const cell = col.chain ? row.cells[col.chain] : null;
		if (what === 'expel' && cell) return void expel(row.label, cell.endpoint, col.db);
		if (what === 'unjoin' && cell) return void unjoin(row.label, cell.endpoint, col.db);
		if (what === 'wipe' && cell) return void wipe(row.label, cell.endpoint, col.db);
		const role = what as 'member' | 'reader';
		void doing(async () => {
			if (cell && (cell.state === 'member' || cell.state === 'reader')) return void (said = ok(await syncAct(col.db, roleArgv(cell.endpoint, role))).stdout.trim());
			if (!row.device) return;
			const name = await ask(`Invite ${row.label} to ${col.db} as a ${role === 'reader' ? 'reader' : 'member'}. The name it takes on that device:`, 'invite', col.db);
			if (name) said = await granting(col.db, row.device, role, name === col.db ? undefined : name);
		});
	}

	const addAdmin = (d: DbSync) =>
		doing(async () => {
			const label = await ask('Name the passkey', 'add', 'passkey');
			if (!label) return;
			ok(await syncAct(d.db, adminAddArgv(await createPasskey(label))));
			// Remembered, so an approval card can say a key is this browser's.
			const before = new Set((d.admins ?? []).map((a) => a.key));
			const { admins } = JSON.parse(ok(await syncAct(d.db, adminListArgv())).stdout) as Admins;
			markOwn(admins.map((a) => a.key).filter((k) => !before.has(k)));
		});

	/** Sign this tab in to sync with a passkey, a new one when `make` (da-nf1p4). */
	const signInHere = (make: boolean) =>
		doing(async () => {
			if (!(await prfOffered())) throw new Error(NO_PRF);
			const label = make ? await ask('Name the passkey', 'make', 'datum') : undefined;
			if (make && !label) return;
			await syncSignIn(label ?? undefined);
		});

	const removeAdmin = (d: DbSync, a: Admin) =>
		doing(async () => {
			const last = d.admins?.length === 1;
			const unlock = 'This unlocks the chain: any device can add, expel or rename devices.';
			if (last && !(await confirm(unlock, 'unlock'))) return;
			ok(await syncAct(d.db, adminRemoveArgv(a.key, last)));
		});

	const ack = (db: string, n: Notice) => doing(async () => ok(await ackNotice(db, n)));

	/** Leave the chain from any chain, for a fresh start (da-j8c4l). */
	const leave = (d: DbSync) =>
		doing(async () => {
			const words = `Leave this sync chain with ${d.db}? Every entry is kept, this device’s own and those it received. It stops syncing with the chain’s other devices; adding a device after starts a new chain.`;
			if (!(await confirm(words, 'leave'))) return;
			const refused = (r: RunResult) => r.code !== 0 && r.stderr.includes('no member holds');
			// A departure leaves only once a member holds it (da-dsob1), which this page's sync
			// delivers: each session it ends well is a reason to try again (da-2o19r).
			const until = Date.now() + deliverMs();
			try {
				for (;;) {
					const since = Date.now();
					const r = await syncAct(d.db, leaveArgv());
					if (!refused(r)) return void ok(r);
					waiting = 'Telling the chain’s other devices that this one is leaving…';
					if (!(await syncedSince(d.db, since, until - Date.now()))) break;
				}
			} finally {
				waiting = '';
			}
			const unheard = `No device of the chain could be reached to receive this device’s departure. ${d.db} stays in the chain, writing nothing, until a sync delivers it; press Leave again then. Leave now, telling no one?`;
			if (!(await confirm(unheard, 'leave anyway'))) return;
			// A member may have come up while the question was open (da-48g3o): one sync, and the leave again, which forces only if still unheard.
			await syncNow(d.db);
			const again = await syncAct(d.db, leaveArgv());
			ok(refused(again) ? await syncAct(d.db, leaveArgv(true)) : again);
		});

	const pause = (d: DbSync) => doing(async () => ok(await syncAct(d.db, pausedArgv(!d.paused))));

	const approveHere = (c: { link: string; db?: string }) =>
		doing(async () => {
			const code = await approve(c.link);
			if (!c.db) return void (approval = { code });
			ok(await approveInto(c.db, c.link, code));
			approving.drop();
			approval = {
				done: `Approved in ${c.db}. It reaches the device that asked when it next syncs: press Enter at its prompt — nothing to paste.`
			};
		});

	function dismiss(): void {
		approving.set(null);
		approval = {};
	}

	const hm = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
	/** `at` as a link's «until»: the time today, the date and time on another day — an invite lasts days (da-4qpwf). */
	const when = (at: number) =>
		new Date(at).toDateString() === new Date().toDateString() ? hm(at) : `${new Date(at).toLocaleDateString([], { month: 'short', day: 'numeric' })} ${hm(at)}`;
</script>

<Pane title="sync">
	<div class="sync">
		{#if $approving}
			<section class="card approve">
				<p class="head">Approve with your passkey?</p>
				{#if card}
					<ul class="what">
						{#each card.words as line}<li>{line}</li>{/each}
					</ul>
				{:else if !approveRecords($approving)}
					<p class="failure">Not an approve link: it carries no record.</p>
				{/if}
				{#if approval.done}
					<p class="done">{approval.done}</p>
				{:else if approval.code}
					<p>This browser holds no copy of that chain’s database. Paste this at the prompt that asked:</p>
					<code class="code">{approval.code}</code>
				{/if}
				<div class="actions">
					{#if !approval.done && !approval.code}
						<button type="button" class="command" use:act={'run'} disabled={busy || !card} onclick={() => card && approveHere(card)}>Approve with passkey</button>
					{/if}
					<button type="button" class="command close" use:act={'show'} onclick={dismiss}>Dismiss</button>
				</div>
			</section>
		{/if}
		{#if joinedSaid && !$joining}
			<section class="card join" data-db={db}>
				<p class="head">Joined a sync chain</p>
				<p class="done">{joinedSaid}</p>
				<div class="actions">
					<button type="button" class="command close" use:act={'show'} onclick={() => (joinedSaid = null)}>Dismiss</button>
				</div>
			</section>
		{/if}
		{#if $joining}
			<section class="card join" data-db={join ? into : undefined}>
				<p class="head">Join a sync chain?</p>
				{#if join}
					{#if join.about && !join.done}
						<!-- Which database joins: one of this browser's, or a new one (da-bedyz). -->
						<label class="name into"
							>Join with
							<select
								disabled={busy}
								value={join.target ?? ''}
								onchange={(e) => join && (join.target = e.currentTarget.value || null)}
							>
								{#each join.dbs as c (c.db)}<option value={c.db}>{c.db === db ? `${c.db} (this page’s)` : c.db}</option>{/each}
								<option value="">a new database</option>
							</select></label
						>
						{#if join.target === null}
							<label class="name fresh"
								>New database’s name <input type="text" bind:value={join.fresh} disabled={busy} spellcheck="false" autocomplete="off" /></label
							>
						{/if}
					{/if}
					<ul class="what">
						{#each joinSaid.words as line}<li>{line}</li>{/each}
					</ul>
					{#if join.done}
						<p class="done">{join.done}</p>
					{:else if busy}
						<!-- A first join of years of history takes a while: it says how far it is (da-89ujs). -->
						<p class="quiet joining" role="status">{joinProgress($syncStates[into]?.applying)}</p>
					{:else if joinSaid.refused}
						<p class="failure">{joinSaid.refused}</p>
					{:else}
						<label class="name"
							>This device’s name <input
								type="text"
								value={join.named ?? suggestedLabel(into)}
								oninput={(e) => join && (join.named = e.currentTarget.value)}
								spellcheck="false"
								autocomplete="off"
							/></label
						>
						<label class="name user">Your username (optional) <input type="text" bind:value={join.user} spellcheck="false" autocomplete="username" /></label>
					{/if}
				{/if}
				<div class="actions">
					{#if join && !join.done && !joinSaid.refused}
						<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => join && joinHere(join, into)}>Join</button>
					{/if}
					<button type="button" class="command close" use:act={'show'} onclick={() => joining.set(null)}>Dismiss</button>
				</div>
			</section>
		{/if}

		<header data-db={db}>
			{#if edit}
				<!-- ✎ edits both in place (da-5wlnr): Enter keeps them, Escape puts them back. -->
				<form
					class="me edit"
					onsubmit={(e) => {
						e.preventDefault();
						if (edit) void saveName(edit);
					}}
				>
					<label class="name"
						>This device’s name <input type="text" bind:value={edit.name} disabled={busy} onkeydown={unedit} spellcheck="false" autocomplete="off" /></label
					>
					<label class="name user"
						>Your username (optional) <input type="text" bind:value={edit.user} disabled={busy} onkeydown={unedit} spellcheck="false" autocomplete="username" /></label
					>
					<div class="actions">
						<button type="submit" class="command" use:act={'run'} disabled={busy}>Save</button>
						<button type="button" class="command close" use:act={'show'} onclick={() => (edit = null)}>Cancel</button>
					</div>
				</form>
			{:else}
				<span class="me"
					><span class="device">{named}</span><button
						type="button"
						class="info rename"
						use:act={'show'}
						disabled={busy}
						aria-label="edit this device’s name and your username"
						title="edit this device’s name and your username"
						onclick={() => (edit = { name: named, user: kept.user, was: named })}>✎</button
					>{#if kept.user}<span class="user">@{kept.user}</span>{/if}</span
				>
				<span class="state {status.mark}" data-mark={status.mark}>{status.words}</span>
			{/if}
			<!-- The guide to all of this, in plain words (da-oyvsz): the Help place opens it. -->
			<button type="button" class="info guide" use:act={'show'} aria-label="how to sync" title="how to sync" onclick={() => explainGuide('sync')}>?</button>
		</header>
		{#if !cur.chain && cur.pending}
			<!-- A join kept until a member admits this device: waiting, never an error (da-4qpwf). -->
			<p class="quiet pending" class:failure={!!cur.pending.refused} data-pending={cur.pending.refused ? 'refused' : 'waiting'}>{pendingWords(cur.pending)}</p>
		{/if}

		{#each dbs as d (d.db)}
			{#each d.notices ?? [] as n (n.subject + n.kind)}
				<p class="notice" data-db={d.db}>
					{n.kind === 'member' ? '' : '⚠ '}{d.db}: {n.text}
					<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => ack(d.db, n)}>OK</button>
				</p>
			{/each}
			{#each d.clock ?? [] as c (c.endpoint)}
				<p class="notice clock" data-db={d.db}>⚠ {d.db}: {c.text}</p>
			{/each}
		{/each}
		{#if grid && drawn.length}
			<section class="grid">
				<p class="label">Devices × databases</p>
				<div class="scroll">
					<table>
						<thead>
							<tr>
								<th></th>
								{#each drawn as col (col.chain ?? col.db)}
									<th data-db={col.db} class:here={here.includes(col)}
										><span class="name">{col.db}</span>{#if here.includes(col)}<span class="kind">this db</span>{/if}</th
									>
								{/each}
								{#if columns.length > here.length}
									<th
										><button type="button" class="command fold" use:act={'show'} aria-expanded={otherDbs} onclick={() => onchange?.(!otherDbs)}
											>{otherDbs ? '‹ fewer' : 'other dbs ›'}</button
										></th
									>
								{/if}
							</tr>
						</thead>
						<tbody>
							{#each devices as row (row.device ?? row.endpoint)}
								<tr data-device={row.device ?? row.endpoint} class:here={row.this}>
									<th
										data-sealed={row.sealed?.length || undefined}
										title={[row.this ? `${row.label}: this device` : row.label, ...(row.sealed ?? []).map((g) => g.text)].join('\n')}>{row.label}</th
									>
									{#each drawn as col (col.chain ?? col.db)}
										{@const standing = col.chain ? row.cells[col.chain] : null}
										<!-- A device expelled or unjoined there is not in it: its row stays only for a chain it is still in. -->
										{@const cell = standing && LIVE.includes(standing.state) ? standing : null}
										{@const grant = col.chain ? row.grants?.[col.chain] : undefined}
										{@const last = lastSync(row, col, cell)}
										{@const acts = row.this || !col.chain || !col.held ? [] : cellActs(cell?.state, !!row.device)}
										{@const words = row.this && !col.chain && col.held ? 'only here' : cellWords(cell?.state, grant)}
										<td
											data-db={col.db}
											data-state={cell?.state ?? 'none'}
											data-mark={last?.mark}
											data-grant={grant?.state}
											data-apart={cell?.apart?.updates}
											title={[last?.title, grant?.text, cell?.apart?.words].filter(Boolean).join('\n') || undefined}
										>
											{#if last}<span class="mark {last.stale ? 'stale' : last.mark}">{SYMBOL[last.mark]}</span>{/if}
											{#if acts.length}
												<select
													aria-label="{row.label} in {col.db}"
													disabled={busy}
													onchange={(e) => {
														const what = e.currentTarget.value;
														e.currentTarget.value = '';
														cellAct(row, col, what);
													}}
												>
													<option value="" selected>{words}</option>
													{#each acts as a (a)}<option value={a}>{a}</option>{/each}
												</select>
											{:else}
												<span class="standing">{words}</span>
											{/if}
											<!-- One of the two must update before entries sync (da-52vdj); the words are its title. -->
											{#if cell?.apart}<span class="apart">⚠ {cell.apart.updates === 'this' ? 'update this' : 'update it'}</span>{/if}
										</td>
									{/each}
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				{#if said}<p class="quiet said" role="status">{said}</p>{/if}
			</section>
		{/if}
		{#if failure}<p class="failure" role="alert">{failure}</p>{/if}
		{#if waiting}<p class="quiet waiting" role="status">{waiting}</p>{/if}

		{#if offer}
			<section class="card offer" data-db={offer.db}>
				{#if joined}
					<p class="head joined">{joined.label} joined at {hm(Date.parse(joined.at))}.</p>
				{:else if expired}
					<p class="head expired">This invite expired at {offer.until}, unused.</p>
				{:else if !offer.relay}
					<p class="head unreachable">This device has not reached its relay yet, so no other device can reach this invite. Check the connection and try again.</p>
				{:else}
					<p class="head">Add a device to {offer.db}: scan this with it, or open the link in its browser. It works once, until {offer.until}.</p>
					<div class="qr">{@html offer.svg}</div>
					<details class="terminal">
						<summary>join from a terminal</summary>
						<code class="code ticket">datum sync chain join '{offer.link}'</code>
					</details>
				{/if}
				<div class="actions">
					{#if joined || expired}
						<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => offer && addDevice(offer.db)}>Add another device</button>
					{:else if !offer.relay}
						<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => offer && addDevice(offer.db)}>Try again</button>
					{:else}
						<button type="button" class="command" use:act={'run'} onclick={() => navigator.clipboard?.writeText(offer?.link ?? '')}>Copy link</button>
						{#if canShare}<button type="button" class="command" use:act={'open'} onclick={() => offer && share(offer.link)}>Share</button>{/if}
					{/if}
					<button type="button" class="command" use:act={'show'} onclick={() => (offer = null)}>Done</button>
				</div>
			</section>
		{:else}
			<div class="actions" data-db={db}>
				{#if !cur.chain && cur.pending && !cur.pending.refused}
					<!-- Waiting for its first sync: nothing to share yet (da-4qpwf). -->
				{:else}
					<!-- A database in no chain starts one as it is first shared, with the name kept under ✎ (da-5wlnr). -->
					<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => addDevice(db, cur.chain ? undefined : startName(db))}
						>Add device</button
					>
				{/if}
				{#if cur.chain}
					<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => pause(cur)}>{cur.paused ? 'Resume sync' : 'Pause sync'}</button>
					<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => leave(cur)}>Leave chain</button>
				{/if}
			</div>
			{#if cur.chain}
				<div class="actions chain" data-db={db}>
					{#if cur.admins?.length}
						<span>locked by</span>
						{#each cur.admins as a (a.key)}
							<button type="button" class="command admin" use:act={cur.admins.length === 1 ? 'open' : 'run'} disabled={busy} aria-label="remove {a.label}" onclick={() => removeAdmin(cur, a)}
								>{a.label} ×</button
							>
						{/each}
						<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => addAdmin(cur)}>Add passkey</button>
					{:else}
						<span>no admin: any device can add devices</span>
						<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => addAdmin(cur)}>Add admin</button>
					{/if}
				</div>
			{/if}
		{/if}
		<!-- The account a passkey signs in to (da-nf1p4): this tab's alone, gone at a reload. -->
		<div class="actions account" data-signed={$account ? 'in' : 'out'}>
			{#if $account}
				<span class="signed-in" title={$account.key}>Signed in: chains started here are your account’s, and its devices join them</span>
				<button type="button" class="command" use:act={'show'} disabled={busy} onclick={() => setAccount(null)}>Sign out</button>
			{:else}
				<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => signInHere(false)}>Sign in with a passkey</button>
				<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => signInHere(true)}>New passkey</button>
			{/if}
		</div>
	</div>
</Pane>

<style>
	.sync {
		--synced: #6fcf97;
		font-family: monospace;
		overflow-wrap: anywhere;
	}
	header {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 0.6rem;
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--rule);
	}
	header .me {
		flex: 1;
		min-width: 0;
		color: var(--ink-bright);
	}
	/* The username under the name, smaller (vividn: «a smaller @username under the device name»). */
	.me .user {
		display: block;
		font-size: 0.8em;
		color: var(--muted);
	}
	.me.edit .name {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.2rem 0.6rem;
	}
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.state {
		white-space: nowrap;
	}
	.kind,
	.label {
		margin-left: 0.4rem;
		font-size: 0.8em;
		color: var(--muted);
	}
	.label {
		margin: 0.6rem 0 0;
	}
	.synced {
		color: var(--synced);
	}
	.syncing {
		color: var(--accent);
	}
	.pending {
		color: var(--warn);
	}
	.error {
		color: var(--error);
	}
	.none,
	.offline,
	.stale {
		color: var(--muted);
	}
	.quiet {
		color: var(--muted);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin-top: 0.6rem;
	}
	.chain span {
		flex: 1 1 100%;
		color: var(--muted);
	}
	.card,
	.notice,
	.failure {
		margin: 0 0 0.5rem;
		padding: 0.5rem;
		border: 1px solid var(--rule);
	}
	.card .actions {
		margin-top: 0.5rem;
	}
	.join .name {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.2rem 0.6rem;
	}
	.name.user {
		margin-top: 0.3rem;
	}
	.join .into,
	.join .fresh {
		margin-bottom: 0.3rem;
	}
	.join input,
	.join select,
	.edit input {
		flex: 1 1 14ch;
		min-width: 0;
		background: var(--page);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
	}
	.head {
		margin: 0;
		color: var(--ink-bright);
	}
	.notice {
		color: var(--warn);
	}
	.failure {
		color: var(--error);
	}
	.qr :global(svg) {
		display: block;
		width: min(12rem, 100%);
		height: auto;
		aspect-ratio: 1;
		margin: 0.5rem 0;
	}
	.terminal summary {
		color: var(--muted);
		cursor: pointer;
	}
	.code {
		display: block;
		margin-top: 0.3rem;
		user-select: all;
		font-size: 0.8em;
	}
	.what {
		margin: 0.3rem 0;
		padding-left: 1.2rem;
		list-style: disc;
		color: var(--ink-bright);
	}
	.info {
		padding: 0 0.2rem;
		font: inherit;
		color: var(--ink);
		background: none;
		border: 0;
		cursor: pointer;
	}
	.info:focus-visible {
		outline: 1px solid var(--accent);
	}
	.grid .scroll {
		overflow-x: auto;
	}
	.grid table {
		border-collapse: collapse;
		font-size: 0.85em;
	}
	.grid th,
	.grid td {
		padding: 0.2rem 0.4rem;
		border-bottom: 1px solid var(--rule);
		text-align: left;
		font-weight: normal;
		white-space: nowrap;
	}
	.grid thead th {
		vertical-align: bottom;
		color: var(--ink-bright);
	}
	.grid thead .name {
		display: block;
	}
	.grid tbody th {
		max-width: 14ch;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.grid .here th {
		color: var(--ink-bright);
	}
	.grid select {
		background: var(--page);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 4px;
		font: inherit;
	}
	.grid thead .here .name {
		font-weight: bold;
	}
	.grid .mark {
		margin-right: 0.3rem;
	}
	.grid .fold {
		padding: 0.1rem 0.3rem;
		font-size: 0.85em;
	}
	.said {
		white-space: pre-line;
	}
</style>
