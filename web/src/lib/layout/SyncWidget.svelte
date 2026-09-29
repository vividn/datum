<!--
	The Sync widget, read-only half (da-x4t0a): devices × databases. A row
	is a device — this one first — with its status dot from the sync
	worker's broadcast (`syncStates`); a column is a database on this
	datum.day; a cell is how that device's copy stands, a shape AND a
	colour. A device not reachable now shows its last-known state grey,
	with its time: never a green ✓ for it. Compact (symbols and dots) at
	the legible floor, full (names, clients, times) above `FULL_PX`.

	Every CLI read is $lib/syncView.ts's. The actions (da-bjxts) go in the
	two empty `.actions` slots: a database's header, and a device's row.

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
	import { syncStates } from '$lib/sync.js';
	import { readSync, table, type DbSync, type Mark } from '$lib/syncView.js';
	import { act } from '$lib/act.js';
	import { prefill } from '$lib/bar.js';
	import { grammar } from './engine.js';

	export let epoch = 0;

	/** Content-box width from which names, clients and times are drawn. */
	const FULL_PX = 320;
	const SYMBOL: Record<Mark, string> = { synced: '✓', syncing: '↻', pending: '◔', error: '!', none: '—' };
	const DOT = { online: '●', reachable: '◐', offline: '○' };

	let dbs: DbSync[] = [];
	let width = 0;
	let reading = false;
	let again = false;
	let stops: (() => void)[] = [];
	let opened: string | null = null;

	$: if (browser) void read(epoch);
	$: rows = table(dbs, $syncStates);
	$: full = width >= FULL_PX;

	async function read(_epoch?: number): Promise<void> {
		if (reading) return void (again = true);
		reading = true;
		try {
			do {
				again = false;
				const names = (await listDatabases()).filter((n) => n !== INTERNAL_DB);
				const listed = names.length ? names : [currentDb()];
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
		<table>
			<thead>
				<tr>
					<th class="device" scope="col">{full ? 'device' : ''}</th>
					{#each dbs as d (d.db)}
						<th scope="col" data-db={d.db} title={d.db}><span class="name">{d.db}</span><span class="actions"></span></th>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.key)}
					<tr data-device={row.key} data-reach={row.reach} class:here={row.here}>
						<th class="device" scope="row" title="{row.label}{row.client ? ` · ${row.client}` : ''} · {seen(row.reach, row.lastSeen)}">
							<span class="dot {row.reach}" aria-label={seen(row.reach, row.lastSeen)}>{DOT[row.reach]}</span>
							<span class="name">{row.label}</span>
							{#if full}
								{#if row.client && row.client !== row.label}<span class="client">{row.client}</span>{/if}
								<span class="seen">{row.here ? 'this device' : seen(row.reach, row.lastSeen)}</span>
							{/if}
							<span class="actions"></span>
						</th>
						{#each dbs as d (d.db)}
							{@const cell = row.cells[d.db]}
							<td data-db={d.db} data-mark={cell.mark} class="{cell.mark}" class:stale={cell.stale} title={cell.title}>
								{#if cell.error}
									<button type="button" class="symbol" aria-label={cell.title} on:click={() => (opened = opened === `${row.key} ${d.db}` ? null : `${row.key} ${d.db}`)}>{SYMBOL[cell.mark]}</button>
								{:else}
									<span class="symbol" aria-label={cell.title}>{SYMBOL[cell.mark]}</span>
								{/if}
								{#if full && cell.stale && time(cell.at)}<span class="at">{time(cell.at)}</span>{/if}
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
			<span class="coming" title="partial sync: coming">⛛{#if full}&nbsp;partial (coming){/if}</span>
			<span class="coming" title="read-only: coming">🔒︎{#if full}&nbsp;read-only (coming){/if}</span>
		</p>
		<details class="advanced">
			<summary>Advanced</summary>
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
	.device {
		position: sticky;
		left: 0;
		z-index: 1;
		text-align: left;
		background: var(--ground);
	}
	.sync:not(.full) .device .name {
		display: inline-block;
		max-width: 5ch;
		overflow: hidden;
		text-overflow: ellipsis;
		vertical-align: bottom;
	}
	.client,
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
	.coming,
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
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.8rem;
		margin: 0.5rem 0 0;
		font-size: 0.85em;
	}
</style>
