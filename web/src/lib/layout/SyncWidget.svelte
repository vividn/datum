<!--
	The Sync widget, read-only half (da-x4t0a): clients × databases. A row
	is a client — an installation: a CLI data directory or a browser
	profile, so one laptop may be two (da-lfsei) — this one first, with its
	status dot from the sync worker's broadcast (`syncStates`); a column is
	a database on this datum.day; a cell is how that client's copy stands,
	a shape AND a colour. A client not reachable now shows its last-known
	state grey, with its time: never a green ✓ for it. A copy that is a
	reader in its chain (da-9yvb6) carries the lock beside its state, and
	one sent only some fields (da-u81u1) the filter too, its title saying
	«filtered: n of m fields».
	Compact (symbols and dots) at the legible floor, full (names, kinds,
	times) above `FULL_PX`.

	Every CLI read and act is $lib/syncView.ts's. The actions (da-bjxts),
	drawn full-size only: a database's header turns sync on or adds a
	device (its join link as a QR and as text, da-l9kdr); a client's row renames or
	revokes it; under the table each chain says whether it is open or which
	passkeys lock it. An act in a locked chain is approved with a passkey
	in this page, and `/sync/approve#<record>` draws the approval card on
	top, which names what the passkey signs — the device, the database, the
	key, and whether this browser made it — before it offers the passkey,
	since a link is anyone's to send: approved, the record is written into this browser's copy of that
	chain's database when it holds one, and sync takes it to the device that
	asked — else the card shows the code to paste. `/sync/join#<ticket>`
	draws the join card the same way (da-l9kdr): it names the inviting
	device, its chain and the page's database before it offers to join, and
	says instead why it cannot — the link expired, the database is in a
	chain already. Notices of new members and unlocks stay above the table
	until acknowledged.

	It is a PLACE (da-6du3o): the commands widget's one `Sync` button opens
	it (a modal on a desktop, a tab on a phone — da-jozgv), so the raw `sync` subcommands the widget stood in for are
	here, under Advanced, each filling `sync <sub> ` on the line.
-->
<script lang="ts">
	import { browser } from '$app/environment';
	import { onDestroy } from 'svelte';
	import Pane from '$lib/Pane.svelte';
	import { onAnyChange } from '$lib/changes.js';
	import { currentDb, INTERNAL_DB } from '$lib/database.js';
	import { listDatabases } from '$lib/databases.js';
	import { joinLink, syncConnect, syncOffer, syncStates, ticketAbout } from '$lib/sync.js';
	import {
		approvalWords,
		approveInto,
		approving,
		joining,
		joinWords,
		markOwn,
		newInvite,
		ownKeys,
		qr,
		readSync,
		syncAct,
		table,
		type Admin,
		type DbSync,
		type Mark,
		type Notice,
		type Row
	} from '$lib/syncView.js';
	import { approve, approveRecord, createPasskey } from '$lib/passkey.js';
	import { ask, confirm } from '$lib/confirm.js';
	import type { RunResult } from '$lib/run.js';
	import { act } from '$lib/act.js';
	import { prefill } from '$lib/bar.js';
	import { grammar } from './engine.js';

	export let epoch = 0;

	/** Content-box width from which names, clients and times are drawn. */
	const FULL_PX = 320;
	const SYMBOL: Record<Mark, string> = { synced: '✓', syncing: '↻', pending: '◔', error: '!', none: '—' };
	const DOT = { online: '●', reachable: '◐', offline: '○' };
	/** Beside the state of a copy that is a reader in its chain (da-9yvb6). */
	const READER = '🔒︎';
	/** Beside it too when the reader is sent only some fields (da-u81u1). */
	const FILTER = '⛛';

	let dbs: DbSync[] = [];
	let width = 0;
	let reading = false;
	let again = false;
	let stops: (() => void)[] = [];
	let opened: string | null = null;
	/** An act under way, and how the last one failed. */
	let busy = false;
	let failure = '';
	/** The join link of the last device invited, and its QR. */
	let offer: { db: string; link: string; svg: string } | null = null;
	/** The approval card's outcome: written here, or the code to paste. */
	let approval: { done?: string; code?: string } = {};
	/**
	 * The card's record resolved against the held copy of its chain, before
	 * the passkey is offered: which database (none held: the code), and what
	 * approving it does, in words.
	 */
	let card: { link: string; db?: string; words: string[] } | null = null;

	/**
	 * The join card (da-l9kdr): the ticket of the link the page was opened
	 * on, the page's database it joins, and what joining does — or why it
	 * cannot — in words, before anything is dialed.
	 */
	let join: { ticket: string; db: string; words: string[]; refused?: string; done?: string } | null = null;

	$: if (browser) void resolve($approving);
	$: if (browser) void resolveJoin($joining);

	$: if (browser) void read(epoch);
	$: rows = table(dbs, $syncStates);
	$: full = width >= FULL_PX;

	async function databases(): Promise<string[]> {
		const names = (await listDatabases()).filter((n) => n !== INTERNAL_DB);
		return names.length ? names : [currentDb()];
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
				// Not the JSON asked for (a process killed mid-write): the table stays as it was.
				dbs = await readSync(listed).catch(() => dbs);
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
		const record = link && approveRecord(link);
		if (!link || !record) return;
		const d = (await readSync(await databases())).find((d) => d.chain && d.chain === record.chain);
		if (link === $approving) card = { link, db: d?.db, words: approvalWords(record, d, ownKeys()) };
	}

	async function resolveJoin(ticket: string | null): Promise<void> {
		join = null;
		if (!ticket) return;
		const db = currentDb();
		const said = await ticketAbout(ticket).then(
			async (about) => joinWords(about, db, (await readSync([db]))[0], Date.now()),
			() => ({ words: [], refused: 'Not a join link: it carries no ticket.' })
		);
		if (ticket === $joining) join = { ticket, db, ...said };
	}

	const joinHere = (j: NonNullable<typeof join>) =>
		doing(async () => {
			const r = await syncConnect(j.db, j.ticket);
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
		if (r.code !== 0) throw new Error(r.stderr.replace(/^error: /, '').trim());
		return r;
	}

	const addDevice = (db: string) =>
		doing(async () => {
			const link = joinLink(await syncOffer(db, await newInvite(db)));
			offer = { db, link, svg: await qr(link) };
		});

	const rename = (row: Row) =>
		doing(async () => {
			const label = await ask(`Rename ${row.label} to`, 'rename', row.label);
			if (!label || label === row.label) return;
			for (const [db, endpoint] of Object.entries(row.endpoints))
				ok(await syncAct(db, ['chain', 'rename', ...(row.here ? [] : [endpoint]), label]));
		});

	const revoke = (row: Row) =>
		doing(async () => {
			const dbs = Object.keys(row.endpoints);
			if (!(await confirm(`Revoke ${row.label}? It can no longer sync ${dbs.join(', ')}.`, 'revoke'))) return;
			for (const db of dbs) ok(await syncAct(db, ['chain', 'revoke', row.endpoints[db]]));
		});

	const addAdmin = (d: DbSync) =>
		doing(async () => {
			const label = await ask('Name the passkey', 'add', 'passkey');
			if (!label) return;
			ok(await syncAct(d.db, ['admin', 'add', await createPasskey(label)]));
			// Remembered, so an approval card can say a key is this browser's.
			const before = new Set((d.admins ?? []).map((a) => a.key));
			const { admins } = JSON.parse(ok(await syncAct(d.db, ['admin', 'list', '--json'])).stdout) as { admins: Admin[] };
			markOwn(admins.map((a) => a.key).filter((k) => !before.has(k)));
		});

	const removeAdmin = (d: DbSync, a: Admin) =>
		doing(async () => {
			const last = d.admins?.length === 1;
			const unlock = 'This unlocks the chain: any device can add, revoke or rename devices.';
			if (last && !(await confirm(unlock, 'unlock'))) return;
			ok(await syncAct(d.db, ['admin', 'remove', a.key, ...(last ? ['--allow-unlock'] : [])]));
		});

	const ack = (db: string, n: Notice) =>
		doing(async () => ok(await syncAct(db, ['chain', 'ack', n.kind === 'unlock' ? 'unlock' : n.subject])));

	const pause = (d: DbSync) => doing(async () => ok(await syncAct(d.db, [d.paused ? 'resume' : 'pause'])));

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

	function time(at?: number): string {
		if (at === undefined || Number.isNaN(at)) return '';
		const d = new Date(at);
		const hm = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
		return d.toDateString() === new Date().toDateString() ? hm : `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${hm}`;
	}

	function seen(reach: string, at?: number): string {
		const t = time(at);
		return reach === 'offline' ? `offline${t ? `, last seen ${t}` : ''}` : reach;
	}
</script>

<Pane title="sync">
	<div class="sync" class:full bind:clientWidth={width}>
		{#if $approving}
			<section class="card approve">
				<p>
					Approve with your passkey?
					<button type="button" class="close" aria-label="dismiss" on:click={dismiss}>×</button>
				</p>
				{#if card}
					<ul class="what">
						{#each card.words as line}<li>{line}</li>{/each}
					</ul>
				{:else if !approveRecord($approving)}
					<p class="failure">Not an approve link: it carries no record.</p>
				{/if}
				{#if approval.done}
					<p class="done">{approval.done}</p>
				{:else if approval.code}
					<p>This browser holds no copy of that chain’s database. Paste this at the prompt that asked:</p>
					<code class="code">{approval.code}</code>
				{:else}
					<button type="button" use:act={'run'} disabled={busy || !card} on:click={() => card && approveHere(card)}>approve with passkey</button>
				{/if}
			</section>
		{/if}
		{#each dbs as d (d.db)}
			{#each d.notices ?? [] as n (n.subject + n.kind)}
				<p class="notice" data-db={d.db}>
					⚠ {d.db}: {n.text}
					<button type="button" use:act={'run'} disabled={busy} on:click={() => ack(d.db, n)}>ok</button>
				</p>
			{/each}
			{#each d.clock ?? [] as c (c.endpoint)}
				<p class="notice clock" data-db={d.db}>⚠ {d.db}: {c.text}</p>
			{/each}
		{/each}
		{#if $joining}
			<section class="card join" data-db={join?.db}>
				<p>
					Join a sync chain?
					<button type="button" class="close" aria-label="dismiss" on:click={() => joining.set(null)}>×</button>
				</p>
				{#if join}
					<ul class="what">
						{#each join.words as line}<li>{line}</li>{/each}
					</ul>
					{#if join.done}
						<p class="done">{join.done}</p>
					{:else if join.refused}
						<p class="failure">{join.refused}</p>
					{:else}
						<button type="button" use:act={'run'} disabled={busy} on:click={() => join && joinHere(join)}>join</button>
					{/if}
				{/if}
			</section>
		{/if}
		{#if offer}
			<section class="card offer" data-db={offer.db}>
				<p>
					Add a device to {offer.db}, within 10 minutes — scan, open the link in its browser, or on the device run:
					<button type="button" class="close" aria-label="dismiss" on:click={() => (offer = null)}>×</button>
				</p>
				<div class="qr">{@html offer.svg}</div>
				<code class="code ticket">datum sync chain connect {offer.link}</code>
				<button type="button" use:act={'run'} on:click={() => navigator.clipboard?.writeText(`datum sync chain connect ${offer?.link}`)}>copy</button>
			</section>
		{/if}
		{#if failure}<p class="failure" role="alert">{failure}</p>{/if}
		<table>
			<thead>
				<tr>
					<th class="client" scope="col">{full ? 'client' : ''}</th>
					{#each dbs as d (d.db)}
						<th scope="col" data-db={d.db} title={d.db}
							><span class="name">{d.db}</span
							>{#if full}<span class="actions"
									><button type="button" use:act={'run'} disabled={busy} on:click={() => addDevice(d.db)}
										>{d.chain ? '+ device' : 'turn on sync'}</button
									></span
								>{/if}</th
						>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.key)}
					<tr data-client={row.key} data-reach={row.reach} class:here={row.here} class:revoked={row.revoked}>
						<th class="client" scope="row" title="{row.label}{row.kind ? ` · ${row.kind}` : ''} · {seen(row.reach, row.lastSeen)}">
							<span class="dot {row.reach}" aria-label={seen(row.reach, row.lastSeen)}>{DOT[row.reach]}</span>
							<span class="name">{row.label}</span>
							{#if full}
								{#if row.kind && row.kind !== row.label}<span class="kind">{row.kind}</span>{/if}
								<span class="seen">{row.here ? 'this client' : row.revoked ? 'revoked' : seen(row.reach, row.lastSeen)}</span>
								{#if Object.keys(row.endpoints).length}
									<span class="actions">
										<button type="button" use:act={'open'} disabled={busy} on:click={() => rename(row)}>rename</button>
										{#if !row.here}<button type="button" use:act={'open'} disabled={busy} on:click={() => revoke(row)}>revoke</button>{/if}
									</span>
								{/if}
							{/if}
						</th>
						{#each dbs as d (d.db)}
							{@const cell = row.cells[d.db]}
							<td data-db={d.db} data-mark={cell.mark} class="{cell.mark}" class:stale={cell.stale} title={cell.title}>
								{#if cell.error}
									<button type="button" class="symbol" aria-label={cell.title} on:click={() => (opened = opened === `${row.key} ${d.db}` ? null : `${row.key} ${d.db}`)}>{SYMBOL[cell.mark]}</button>
								{:else}
									<span class="symbol" aria-label={cell.title}>{SYMBOL[cell.mark]}</span>
								{/if}
								{#if cell.reader}<span class="reader" aria-hidden="true">{READER}</span>{/if}
								{#if cell.filter}<span class="filter" aria-hidden="true">{FILTER}</span>{/if}
								{#if full && cell.stale && time(cell.at)}<span class="at">{time(cell.at)}</span>{/if}
								{#if full && cell.mark === 'none' && !row.revoked}<button type="button" class="add" use:act={'run'} disabled={busy} aria-label="add to {d.db}" on:click={() => addDevice(d.db)}>+</button>{/if}
								{#if opened === `${row.key} ${d.db}`}<p class="reason">{cell.error}</p>{/if}
							</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
		<p class="legend">
			{#each Object.entries(SYMBOL) as [mark, symbol] (mark)}
				<span class={mark} title={mark}>{symbol}{#if full}&nbsp;{mark === 'none' ? 'not in chain' : mark}{/if}</span>
			{/each}
			<span class="filter" title="partial: a reader sent only some fields">{FILTER}{#if full}&nbsp;partial{/if}</span>
			<span class="reader" title="read-only">{READER}{#if full}&nbsp;read-only{/if}</span>
		</p>
		{#if full}
			{#each dbs.filter((d) => d.chain) as d (d.db)}
				<p class="chain" data-db={d.db}>
					{d.db}:
					{#if d.admins?.length}
						locked by
						{#each d.admins as a (a.key)}
							<span class="admin">{a.label}<button type="button" class="close" disabled={busy} aria-label="remove {a.label}" on:click={() => removeAdmin(d, a)}>×</button></span>
						{/each}
						<button type="button" use:act={'open'} disabled={busy} on:click={() => addAdmin(d)}>+ passkey</button>
					{:else}
						<span class="quiet">no admin: any device can add devices</span>
						<button type="button" use:act={'open'} disabled={busy} on:click={() => addAdmin(d)}>add admin</button>
					{/if}
				</p>
			{/each}
		{/if}
		<details class="advanced">
			<summary>Advanced</summary>
			{#each dbs.filter((d) => d.chain) as d (d.db)}
				<button type="button" class="pause" use:act={'run'} disabled={busy} data-db={d.db} on:click={() => pause(d)}
					>{d.paused ? `resume ${d.db}` : `pause ${d.db}`}</button
				>
			{/each}
			{#each $grammar?.get('sync')?.subcommands ?? [] as sub (sub)}
				<button
					type="button"
					class="control"
					use:act={'fill'}
					data-command="sync {sub}"
					on:click={() => prefill.set({ line: `sync ${sub} `, keyboard: true })}>{sub}</button
				>
			{/each}
		</details>
	</div>
</Pane>

<style>
	.sync {
		--synced: #6fcf97;
		font-family: monospace;
	}
	table {
		border-collapse: collapse;
	}
	th,
	td {
		padding: 0.15rem 0.4rem;
		text-align: center;
		white-space: nowrap;
		font-weight: normal;
		border-bottom: 1px solid var(--rule);
	}
	thead th {
		color: var(--muted);
	}
	.client {
		position: sticky;
		left: 0;
		z-index: 1;
		text-align: left;
		background: var(--ground);
	}
	.sync:not(.full) .client .name {
		display: inline-block;
		max-width: 5ch;
		overflow: hidden;
		text-overflow: ellipsis;
		vertical-align: bottom;
	}
	.kind,
	.seen,
	.at {
		margin-left: 0.4rem;
		font-size: 0.8em;
		color: var(--quiet);
	}
	.at {
		display: block;
		margin: 0;
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
	.symbol {
		font: inherit;
		color: inherit;
		background: none;
		border: 0;
		padding: 0;
		cursor: default;
	}
	button.symbol {
		cursor: pointer;
		text-decoration: underline dotted;
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
	.stale {
		color: var(--quiet);
	}
	.reason {
		margin: 0.2rem 0 0;
		white-space: normal;
		color: var(--ink);
		text-align: left;
	}
	.advanced {
		margin-top: 0.5rem;
		font-size: 0.85em;
	}
	.advanced summary {
		color: var(--muted);
		cursor: pointer;
	}
	.advanced button {
		margin: 0.3rem 0.3rem 0 0;
		padding: 0.1rem 0.5rem;
		min-height: 24px;
	}
	.actions button,
	.add,
	.card button,
	.chain button,
	.notice button {
		margin-left: 0.3rem;
		padding: 0 0.4rem;
		font: inherit;
		font-size: 0.85em;
	}
	/* The design language ($lib/act.ts): a DOUBLE outline acts at once, a
	   SINGLE one of the accent opens something. */
	button:global([data-act='run']) {
		border: 3px double var(--accent);
		color: var(--ink-bright);
	}
	button:global([data-act='open']) {
		border: 1px solid var(--accent);
	}
	.close {
		background: none;
		border: 0;
		color: var(--muted);
		cursor: pointer;
	}
	.card,
	.notice,
	.failure {
		margin: 0 0 0.5rem;
		padding: 0.4rem;
		border: 1px solid var(--rule);
		white-space: normal;
	}
	.notice {
		color: var(--warn);
	}
	.failure {
		color: var(--error);
	}
	.qr :global(svg) {
		display: block;
		width: 12rem;
		height: 12rem;
		margin: 0.4rem 0;
	}
	.code {
		display: block;
		overflow-wrap: anywhere;
		user-select: all;
		font-size: 0.8em;
	}
	.revoked .name {
		text-decoration: line-through;
		color: var(--quiet);
	}
	.chain {
		margin: 0.4rem 0 0;
		font-size: 0.85em;
		color: var(--muted);
	}
	.what {
		margin: 0.3rem 0;
		padding-left: 1.2rem;
		color: var(--ink-bright);
	}
	.quiet {
		color: var(--quiet);
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.8rem;
		margin: 0.5rem 0 0;
		font-size: 0.85em;
	}
</style>
