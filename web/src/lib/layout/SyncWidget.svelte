<!--
	The Sync widget (da-x4t0a), redone as a view with its actions under it
	(da-yoef7). vividn: «you have a nice view, and then actions for changing
	sync setup and stuff are on the lower side of the widget together.»

	Top, the page's database: its name and how it stands overall, then ONE
	LINE PER DEVICE in its chain, this one first — the reachability dot
	from the sync worker's broadcast (`syncStates`), the state as a shape
	AND a word, and when it was last seen. A device not reachable now shows
	its last-known state grey: never a green ✓ for it. One in error says
	why under its line; one offline, its last session never reaching it, is
	grey with when it was last seen, and no why (da-ueb4p). A reader (da-9yvb6) carries the lock beside its
	state, a filtered one (da-u81u1) the filter too. A list, not a table: a
	table of clients × databases was 459px on a 390px phone.

	Middle, every other database on one line: its state, and «same devices»
	when it is in this one's chain. A tap switches to it as the logo does,
	by a load (Brand.svelte says why).

	Under it, the grid (da-n0uj1.7): every device this install's databases
	know of against every database, each cell its standing there as `sync
	grid` has it and a dropdown of what may be done to it — a role (a grant,
	sealed to the device in a chain both share, or a role change), expel,
	unjoin, wipe, each of the last three confirming in a dialog that says
	what happens to whose data. A column's «grant like…» gives its database
	another's devices (`sync grant --like`), starting its chain with that one's
	admins when it is in none. Only the CLI's commands: the grant's ticket is
	the sync worker's, as [Add device]'s is.

	Bottom, every act together: [Turn on sync] alone for a database in no
	chain; else [Add device], [Rename this device] and [More], which holds
	pause, the other devices' rename, expel and unjoin, the admins (da-bjxts) and
	the raw `sync` subcommands, each filling `sync <sub> ` on the line
	(da-6du3o). The header's `?` opens the guide to all of it (da-oyvsz).
	[Add device], or `sync chain invite` typed on the page (da-5g0nk), opens one panel — the QR, until when it works,
	and its own buttons at its bottom — with the ticket under a closed
	«connect from a terminal». Once the invite is used, or expired, the QR and
	link go: the panel names the device that joined, or says it expired, and
	offers [Add another device] (da-2bvxq). The cards a link opens — `/sync/approve#…`,
	which names what the passkey signs before it offers the passkey, since
	a link is anyone's to send, and `/sync/join#…` (da-l9kdr), which names
	the inviting device, its chain and the page's database before it joins
	or says why it cannot — are the same shape, on top. Notices of new
	members and unlocks stand with the acts until acknowledged.

	Every CLI read and act is #lib/syncView.ts's. Every button is the
	commands widget's `button.command` (app.css): the browser's own grey
	under the page's pale ink had been a contrast of 1.1.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { onDestroy, untrack } from 'svelte';
	import Pane from '#lib/Pane.svelte';
	import { onAnyChange } from '#lib/changes.ts';
	import { currentDb, INTERNAL_DB, withDb } from '#lib/database.ts';
	import { listDatabases } from '#lib/databases.ts';
	import { deliverMs, inviting, joinLink, suggestedLabel, syncConnect, syncedSince, syncNow, syncOffer, syncStates, ticketAbout, type Invite, type SyncState } from '#lib/sync.ts';
	import {
		ackNotice,
		approvalWords,
		approveInto,
		approving,
		canLeave,
		joining,
		joinWords,
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
		type Deferred,
		type Grid,
		type Mark,
		type Notice,
		type PurgePreview,
		type Row
	} from '#lib/syncView.ts';
	import { approve, approveRecords, createPasskey } from '#lib/passkey.ts';
	import { ask, choose, confirm } from '#lib/confirm.ts';
	import { cliError, type RunResult } from '#lib/run.ts';
	import {
		adminAddArgv,
		adminListArgv,
		adminRemoveArgv,
		labelCheckArgv,
		leaveArgv,
		pausedArgv,
		unjoinArgv,
		renameArgv,
		expelArgv,
		grantLikeArgv,
		grantArgv,
		gridArgv,
		roleArgv,
		wipeArgv
	} from '#lib/calls.ts';
	import { act } from '#lib/act.ts';
	import { now } from '#lib/clock.ts';
	import { prefill } from '#lib/bar.ts';
	import { explainGuide } from '#lib/help.ts';
	import { grammar } from './engine.js';

	let {
		epoch = 0
	}: {
		epoch?: number;
	} = $props();

	const SYMBOL: Record<Mark, string> = { synced: '✓', syncing: '↻', pending: '◔', offline: '○', error: '!', none: '—' };
	const WORD: Record<Mark, string> = { synced: 'synced', syncing: 'syncing', pending: 'pending', offline: 'offline', error: 'error', none: 'not syncing' };
	const DOT = { online: '●', reachable: '◐', offline: '○' };
	/** Beside the state of a copy that is a reader in its chain (da-9yvb6). */
	const READER = '🔒︎';
	/** Beside it too when the reader is sent only some fields (da-u81u1). */
	const FILTER = '⛛';
	/** The order a legend and a database's overall state are read in. */
	const MARKS: Mark[] = ['synced', 'syncing', 'pending', 'offline', 'error', 'none'];
	const canShare = browser && typeof navigator.share === 'function';

	/** The page's database: the one this widget is about. */
	const db = browser ? currentDb() : '';
	let dbs = $state.raw<DbSync[]>([]);
	/** Devices × chains, as `sync grid` has it; and what the CLI said of the last grid act. */
	let grid = $state.raw<Grid | null>(null);
	let said = $state('');
	let reading = false;
	let again = false;
	let stops: (() => void)[] = [];
	/** An act under way, and how the last one failed. */
	let busy = $state(false);
	let failure = $state('');
	/** What an act under way waits on, in words. */
	let waiting = $state('');
	/** More's acts shown, and the whole legend. */
	let more = $state(false);
	let key = $state(false);
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
	 * on, the page's database it joins, and what joining does — or why it
	 * cannot — in words, before anything is dialed — and the name it joins
	 * as, the suggestion filled in (da-uyyyg).
	 */
	let join = $state<{ ticket: string; db: string; name: string; words: string[]; refused?: string; done?: string } | null>(null);

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
				void addDevice(asked);
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
	const others = $derived(dbs.filter((d) => d.db !== db));
	const rows = $derived(cur.chain ? table([cur], $syncStates) : []);
	const overall = $derived(standing(cur, $syncStates));
	// An expelled device has no line, only a count of them: «More» lists them (da-73gyz).
	const live = $derived(rows.filter((r) => !r.revoked));
	const hidden = $derived(rows.filter((r) => r.revoked).length);
	const shown = $derived([...live.map((r) => r.cells[db]), ...others.map((d) => standing(d, $syncStates))]);
	/** The grid's columns: every chain, then each database in none, which only a copy fills. */
	const columns = $derived([
		...(grid?.chains ?? []),
		...dbs.filter((d) => !d.chain && !grid?.chains.some((c) => c.db === d.db)).map((d) => ({ chain: null, db: d.db }))
	]);
	const used = $derived(MARKS.filter((m) => shown.some((c) => c.mark === m)));

	/** How `d` stands as a whole: the worst of its devices, which this copy's cell already is. */
	function standing(d: DbSync, states: Record<string, SyncState>): { mark: Mark; reader?: boolean; filter?: unknown } {
		if (!d.chain) return { mark: 'none' };
		return table([d], states)[0].cells[d.db];
	}

	/** What another database's line says beside its state. */
	function devices(d: DbSync): string {
		if (!d.chain) return WORD.none;
		const live = d.members.filter((m) => !m.revoked);
		const failed = live.filter((m) => m.state === 'error').map((m) => `${m.label}: error`);
		const who = d.chain === cur.chain ? 'same devices' : `${live.length + (d.this ? 1 : 0)} devices`;
		return [who, ...failed].join(' · ');
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
				const listed = await databases();
				if (listed.join() !== dbs.map((d) => d.db).join()) {
					stops.forEach((stop) => stop());
					stops = listed.map((db) => onAnyChange(() => void read(), db));
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
		const db = currentDb();
		const said = await ticketAbout(ticket).then(
			async (about) => {
				const d = (await readSync([db]))[0];
				return joinWords(about, db, d, Date.now(), !!d?.chain && (await canLeave(db)));
			},
			() => ({ words: [], refused: 'Not a join link: it carries no ticket.' })
		);
		if (ticket === $joining) join = { ticket, db, name: suggestedLabel(db), ...said };
	}

	/**
	 * How far a join is: its apply's `done` of `of` ops, once the ops are in.
	 * Each step holds the database, so a command typed meanwhile waits for it.
	 */
	const joinProgress = (a?: SyncState['applying']) =>
		a
			? `Joining: ${a.done.toLocaleString()} of ${a.of.toLocaleString()} ops applied… A command typed meanwhile waits for the step under way.`
			: 'Joining…';

	const joinHere = (j: NonNullable<typeof join>) =>
		doing(async () => {
			// A name typed is kept as typed; the suggestion, untouched, grows a
			// short id on a collision. Either is refused before the join as
			// `--label` is.
			const typed = j.name.trim() && j.name !== suggestedLabel(j.db) ? j.name : undefined;
			ok(await syncAct(j.db, labelCheckArgv(typed ?? suggestedLabel(j.db))));
			const r = await syncConnect(j.db, j.ticket, typed);
			joining.drop();
			join = { ...j, done: `Joined: ${r.got} op(s) came in from ${r.label}, ${r.sent} went to it.` };
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
	function addDevice(db: string): Promise<void> {
		return doing(async () => {
			// The ticket waits for this device to reach its relay, up to 30 s (da-kfhty).
			waiting = 'Making the invite…';
			try {
				const invite = await newInvite(db);
				const ticket = await syncOffer(db, invite);
				const { relay = false } = await ticketAbout(ticket);
				const link = joinLink(ticket);
				const expires = Date.parse(invite.expires);
				offer = { db, link, svg: await qr(link), until: hm(expires), expires, id: invite.id, relay };
			} finally {
				waiting = '';
			}
		});
	}

	const share = (link: string) => navigator.share({ title: 'Join a datum sync chain', url: link }).catch(() => {});

	/** This device, in every database it syncs: a client is one name. */
	const renameHere = () =>
		doing(async () => {
			const was = cur.this?.label ?? '';
			const label = await ask('Rename this device to', 'rename', was);
			if (!label || label === was) return;
			for (const d of dbs) if (d.this) ok(await syncAct(d.db, renameArgv(label)));
		});

	const rename = (row: Row) =>
		doing(async () => {
			const label = await ask(`Rename ${row.label} to`, 'rename', row.label);
			if (!label || label === row.label) return;
			ok(await syncAct(db, renameArgv(label, row.endpoints[db])));
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
	 * Each chain the device is live in here is synced, so the grant reaches it now if it is online: the
	 * CLI picks one of them as the carrier (one this copy writes to) and does not say which, so all are
	 * pushed, and one that fails is a device away, which the grant waits for in the chain.
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

	type GridRow = Grid['devices'][number];

	/** A cell's dropdown chose `what`: a role is a grant, or a role change for a live member; the rest confirm first. */
	function cellAct(row: GridRow, col: { chain: string | null; db: string }, what: string): void {
		const cell = col.chain ? row.cells[col.chain] : null;
		if (what === 'expel' && cell) return void expel(row.label, cell.endpoint, col.db);
		if (what === 'unjoin' && cell) return void unjoin(row.label, cell.endpoint, col.db);
		if (what === 'wipe' && cell) return void wipe(row.label, cell.endpoint, col.db);
		const role = what as 'member' | 'reader';
		void doing(async () => {
			if (cell && (cell.state === 'member' || cell.state === 'reader')) return void (said = ok(await syncAct(col.db, roleArgv(cell.endpoint, role))).stdout.trim());
			if (!row.device) return;
			const name = await ask(`Grant ${row.label} ${role === 'reader' ? 'read-only access to' : 'membership of'} ${col.db}. The name it takes on that device:`, 'grant', col.db);
			if (name) said = await granting(col.db, row.device, role, name === col.db ? undefined : name);
		});
	}

	/** «grant like…»: `col` takes another database's devices, each with its role there, the grants left to this page: one approval for all their invites (da-7bjai). */
	const grantLike = (col: { chain: string | null; db: string }) =>
		doing(async () => {
			const sources = columns.filter((c) => c.chain && c.db !== col.db).map((c) => c.db);
			const which = col.chain ? `Each of its devices ${col.db} lacks` : `${col.db} starts a sync chain with the same admins, and each of its devices`;
			const from = await choose(`Grant ${col.db} the devices of which database? ${which} is granted the role it has there, and joins ${col.db} when it next syncs.`, sources);
			if (!from) return;
			const r = ok(await syncAct(col.db, grantLikeArgv(from)));
			// `grant --like --defer` prints the grants left as its one stdout line, saying all else on stderr.
			const { grants } = JSON.parse(r.stdout.split('\n').find((l) => l.startsWith('{"grants"')) ?? '') as Deferred;
			const lines = [r.stderr.trim()];
			for (const g of grants) lines.push(await granting(col.db, g.device, g.role, undefined, g.invite));
			said = lines.filter(Boolean).join('\n') || `${from} has no device ${col.db} lacks`;
		});

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

	/** When a device was last seen, relative: «now», «2 min ago», «3 h ago», a date. */
	function ago(at?: number): string {
		if (at === undefined || Number.isNaN(at)) return 'never';
		const s = Math.max(0, (Date.now() - at) / 1000);
		if (s < 60) return 'now';
		if (s < 3600) return `${Math.floor(s / 60)} min ago`;
		if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
		return new Date(at).toLocaleDateString([], { month: 'short', day: 'numeric' });
	}
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
		{#if $joining}
			<section class="card join" data-db={join?.db}>
				<p class="head">Join a sync chain?</p>
				{#if join}
					<ul class="what">
						{#each join.words as line}<li>{line}</li>{/each}
					</ul>
					{#if join.done}
						<p class="done">{join.done}</p>
					{:else if busy}
						<!-- A first join of years of history takes a while: it says how far it is (da-89ujs). -->
						<p class="quiet joining" role="status">{joinProgress($syncStates[join.db]?.applying)}</p>
					{:else if join.refused}
						<p class="failure">{join.refused}</p>
					{:else}
						<label class="name">This device’s name <input type="text" bind:value={join.name} spellcheck="false" autocomplete="off" /></label>
					{/if}
				{/if}
				<div class="actions">
					{#if join && !join.done && !join.refused}
						<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => join && joinHere(join)}>Join</button>
					{/if}
					<button type="button" class="command close" use:act={'show'} onclick={() => joining.set(null)}>Dismiss</button>
				</div>
			</section>
		{/if}

		<header data-db={db}>
			<span class="title">Sync · {db}</span>
			<span class="state {overall.mark}" data-mark={overall.mark}
				>{SYMBOL[overall.mark]} {cur.chain && cur.paused ? 'paused' : overall.mark === 'synced' ? 'all synced' : WORD[overall.mark]}</span
			>
			<!-- The guide to all of this, in plain words (da-oyvsz): the Help place opens it. -->
			<button type="button" class="info guide" use:act={'show'} aria-label="how to sync" title="how to sync" onclick={() => explainGuide('sync')}>?</button>
		</header>
		{#if cur.chain}
			<ul class="devices">
				{#each live as row (row.key)}
					{@const cell = row.cells[db]}
					<li
						data-client={row.key}
						data-reach={row.reach}
						data-mark={cell.mark}
						class:here={row.here}
						class:stale={cell.stale}
						title={cell.title}
					>
						<span class="dot {row.reach}" aria-label={row.reach}>{DOT[row.reach]}</span>
						<span class="who"
							><span class="name">{row.label}</span
							>{#if row.here}<span class="kind">this device</span>{:else if row.kind && row.kind !== row.label}<span class="kind">{row.kind}</span>{/if}</span
						>
						<span class="state {cell.stale ? 'stale' : cell.mark}"
							><span class="symbol">{SYMBOL[cell.mark]}</span> {WORD[cell.mark]}{#if cell.reader}<span class="reader" aria-hidden="true">{READER}</span>{/if}{#if cell.filter}<span class="filter" aria-hidden="true">{FILTER}</span>{/if}</span
						>
						<span class="seen">{row.here ? 'now' : ago(row.lastSeen)}</span>
						{#if cell.error}<p class="reason">{cell.error}</p>{/if}
					</li>
				{/each}
			</ul>
			{#if hidden}<p class="quiet revoked">{hidden} former member{hidden === 1 ? '' : 's'} hidden — More lists {hidden === 1 ? 'it' : 'them'}</p>{/if}
		{:else}
			<p class="quiet">On this device only: no other device has a copy.</p>
		{/if}

		{#if others.length}
			<p class="label">Other databases</p>
			<ul class="others">
				{#each others as d (d.db)}
					{@const s = standing(d, $syncStates)}
					<li data-db={d.db} data-mark={s.mark}>
						<a href={withDb(location.pathname, d.db)} data-sveltekit-reload
							><span class="name">{d.db}</span><span class="state {s.mark}">{SYMBOL[s.mark]}</span><span class="seen">{devices(d)}</span></a
						>
					</li>
				{/each}
			</ul>
		{/if}

		{#if used.length}
			<p class="legend">
				{#each used as mark (mark)}<span class={mark}>{SYMBOL[mark]} {WORD[mark]}</span>{/each}
				{#if shown.some((c) => c.filter)}<span class="filter" title="a reader sent only some fields">{FILTER} partial</span>{/if}
				{#if shown.some((c) => c.reader)}<span class="reader">{READER} read-only</span>{/if}
				<button type="button" class="info" use:act={'show'} aria-label="what the marks mean" aria-expanded={key} onclick={() => (key = !key)}>ⓘ</button>
			</p>
		{/if}
		{#if key}
			<p class="key">
				{#each MARKS as mark (mark)}<span class={mark}>{SYMBOL[mark]} {mark === 'none' ? 'not in a chain' : WORD[mark]}</span>{/each}
				<span>{FILTER} partial: a reader sent only some fields</span>
				<span>{READER} read-only</span>
				<span>● online ◐ reachable ○ offline, its state as last seen</span>
			</p>
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
		{#if columns.length && grid?.chains.length}
			<section class="grid">
				<p class="label">Devices × databases</p>
				<div class="scroll">
					<table>
						<thead>
							<tr>
								<th></th>
								{#each columns as col (col.db)}
									<th data-db={col.db}
										><span class="name">{col.db}</span>{#if columns.some((c) => c.chain && c.db !== col.db)}<button
												type="button"
												class="command like"
												use:act={'open'}
												disabled={busy}
												onclick={() => grantLike(col)}>grant like…</button
											>{/if}</th
									>
								{/each}
							</tr>
						</thead>
						<tbody>
							{#each grid.devices as row (row.device ?? row.endpoint)}
								<tr data-device={row.device ?? row.endpoint} class:here={row.this}>
									<th title={row.this ? `${row.label}: this device` : row.label}>{row.label}</th>
									{#each columns as col (col.db)}
										{@const cell = col.chain ? row.cells[col.chain] : null}
										{@const acts = row.this || !col.chain ? [] : cellActs(cell?.state, !!row.device)}
										<td data-db={col.db} data-state={cell?.state ?? 'none'}>
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
													<option value="" selected>{cell?.state ?? '—'}</option>
													{#each acts as a (a)}<option value={a}>{a}</option>{/each}
												</select>
											{:else}
												<span class="standing">{cell?.state ?? '—'}</span>
											{/if}
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
						<summary>connect from a terminal</summary>
						<code class="code ticket">datum sync chain connect '{offer.link}'</code>
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
				{#if !cur.chain}
					<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => addDevice(db)}>Turn on sync</button>
				{:else}
					<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => addDevice(db)}>Add device</button>
					<button type="button" class="command" use:act={'open'} disabled={busy} onclick={renameHere}>Rename this device</button>
					<button type="button" class="command" use:act={'show'} aria-expanded={more} onclick={() => (more = !more)}>More <span aria-hidden="true">{more ? '▾' : '▸'}</span></button>
				{/if}
			</div>
			{#if more && cur.chain}
				<section class="more">
					<div class="actions">
						<button type="button" class="command" use:act={'run'} disabled={busy} onclick={() => pause(cur)}>{cur.paused ? 'Resume sync' : 'Pause sync'}</button>
						<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => leave(cur)}>Leave chain</button>
					</div>
					{#each live.filter((r) => !r.here) as row (row.key)}
						<div class="actions device" data-client={row.key}>
							<span class="name">{row.label}</span>
							<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => rename(row)}>Rename</button>
							<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => expel(row.label, row.endpoints[db])}>Expel</button>
							<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => unjoin(row.label, row.endpoints[db])}>Unjoin</button>
						</div>
					{/each}
					{#each cur.members.filter((m) => m.revoked) as m (m.endpoint)}
						<div class="actions device" data-endpoint={m.endpoint}>
							<span class="name">{m.label} · {m.purged ? 'unjoined' : 'expelled'}</span>
							{#if !m.purged}
								<button type="button" class="command" use:act={'open'} disabled={busy} onclick={() => unjoin(m.label, m.endpoint)}>Unjoin</button>
							{/if}
						</div>
					{/each}
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
					<div class="actions advanced">
						{#each $grammar?.get('sync')?.subcommands ?? [] as sub (sub)}
							<button
								type="button"
								class="command"
								use:act={'fill'}
								data-command="sync {sub}"
								onclick={() => prefill.set({ line: `sync ${sub} `, keyboard: true })}>sync {sub}</button
							>
						{/each}
					</div>
				</section>
			{/if}
		{/if}
	</div>
</Pane>

<style>
	.sync {
		--synced: #6fcf97;
		font-family: monospace;
		overflow-wrap: anywhere;
	}
	header,
	.devices li,
	.others a {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 0.6rem;
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--rule);
	}
	header .title {
		flex: 1;
		color: var(--ink-bright);
	}
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.who {
		flex: 1 1 8ch;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.state,
	.seen {
		white-space: nowrap;
	}
	.kind,
	.seen,
	.label {
		margin-left: 0.4rem;
		font-size: 0.8em;
		color: var(--muted);
	}
	.label {
		margin: 0.6rem 0 0;
	}
	.others a {
		color: inherit;
		text-decoration: none;
	}
	.others .name {
		flex: 1 1 8ch;
		min-width: 0;
	}
	.dot.online {
		color: var(--synced);
	}
	.dot.reachable {
		color: var(--accent);
	}
	.dot.offline {
		color: var(--quiet);
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
	.reason {
		flex-basis: 100%;
		margin: 0.2rem 0 0 1.4rem;
		color: var(--error);
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
	.device .name {
		flex: 0 0 10ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--muted);
	}
	.more {
		margin-top: 0.4rem;
		padding-top: 0.2rem;
		border-top: 1px solid var(--rule);
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
	.join input {
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
	.legend,
	.key {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.2rem 0.8rem;
		margin: 0.5rem 0 0;
		font-size: 0.85em;
	}
	.legend span,
	.key span {
		white-space: nowrap;
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
	.grid .like {
		padding: 0.1rem 0.3rem;
		font-size: 0.85em;
	}
	.said {
		white-space: pre-line;
	}
</style>
