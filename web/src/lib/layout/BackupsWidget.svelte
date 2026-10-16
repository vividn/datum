<!--
	«Backups» (da-9ow8m): the gear's Backups, and the commands widget's
	`db backup` / `db restore`. It is a PLACE (da-ea5q2): each of those
	opens it through `openBackups` ($lib/backups.ts) — a modal on a
	desktop, a tab on a phone — and dropped into a pane it is just this
	widget.

	Top to bottom: a file on this device (the SAVE picker, and «Restore
	from a file», which replaced the root tab menu's «export backup…»);
	what a plan is and what it is not (encrypted, or running while the
	browser is closed); each plan, with its backups by database, newest
	first, each restorable; and a new plan. Every number is the CLI's —
	`$lib/backups.ts` says which command answers which.

	The design language's two outlines: DOUBLE acts at once («Back up
	now», «Back up all now»), SINGLE opens something (a picker, a confirm).
-->
<script lang="ts">
	import { onDestroy, onMount, tick } from 'svelte';
	import Pane from '$lib/Pane.svelte';
	import { act } from '$lib/act.js';
	import { restoreArgv } from '$lib/calls.js';
	import { confirm } from '$lib/confirm.js';
	import { currentDb, INTERNAL_DB } from '$lib/database.js';
	import { listDatabases } from '$lib/databases.js';
	import { restore as restoreFile, saveBackup } from '$lib/dbio.js';
	import type { RunResult } from '$lib/run.js';
	import { ran } from './engine.js';
	import {
		ago,
		ageOf,
		backUp,
		backupSize,
		bytes,
		check,
		defaults,
		deletePlan,
		draft,
		HOUR,
		KINDS,
		listed,
		loadPlans,
		newest,
		offered,
		overdue,
		plans,
		reaches,
		refresh,
		restoreFrom,
		savePlan,
		status,
		toRestore,
		type Backup,
		type Plan,
		type Reach
	} from '$lib/backups.js';
	import type { StepResult } from '$lib/probe.js';

	let body: HTMLElement | undefined;
	let note = '';
	let dbs: string[] = [];
	/** One gzipped backup's size, by database. */
	let sizes: Record<string, number> = {};
	/** Each plan's reach after a day, a month and a year — and the new plan's, under ''. */
	let reach: Record<string, Reach[]> = {};

	let fresh: Plan = draft(currentDb());
	let slotsText = '';
	/** What the new plan's destination already holds, once asked. */
	let found: Backup[] | null = null;
	/** «Test connection»'s steps, once run. */
	let probe: StepResult[] | null = null;
	/** What the new plan's destination needs to allow this page. */
	$: cors = KINDS[fresh.kind]?.cors;

	const EVERY: [number, string][] = [
		[15 * 60, 'every 15 minutes'],
		[HOUR, 'hourly'],
		[6 * HOUR, 'every 6 hours'],
		[24 * HOUR, 'daily']
	];

	let ready = false;
	onMount(() => void opened().then(() => (ready = true)));
	// `db restore` scrolls to the first plan's backups, however the place was
	// reached. `scrollToRestore` clears the request, so this fires once per
	// request; a widget gone before it was ready clears it too, or the next
	// Backups to mount would scroll unasked.
	$: if (ready && $toRestore) void scrollToRestore();
	onDestroy(() => toRestore.set(false));

	async function scrollToRestore(): Promise<void> {
		toRestore.set(false);
		await tick();
		body?.querySelector('[data-restore]')?.scrollIntoView({ block: 'nearest' });
	}

	async function opened(): Promise<void> {
		// Never `_internal`: it holds every plan's keys, to this bucket and others.
		dbs = (await listDatabases().catch(() => [currentDb()])).filter((db) => db !== INTERNAL_DB);
		if (!dbs.includes(currentDb())) dbs = [currentDb(), ...dbs];
		const shown = (await loadPlans()).map((plan) => refresh(plan).then(() => explain(plan)));
		if (fresh.kind) void explain(fresh, '');
		// Ready once the first plan's listing and reach are drawn: they stand
		// above the backups `db restore` scrolls to, and landing after the
		// scroll pushed those back out of view (da-p3bc8).
		await shown[0];
	}

	async function explain(plan: Plan, key = plan.name): Promise<void> {
		for (const db of plan.dbs) if (sizes[db] === undefined) sizes = { ...sizes, [db]: await backupSize(db).catch(() => 0) };
		reach = { ...reach, [key]: await reaches(plan).catch(() => []) };
	}

	/** «after a day, 6 files reaching back 16 h», for each span. */
	const reachText = (plan: Plan, spans: Reach[]): string =>
		spans
			.map((r) => `after ${ago(r.span_secs)}, ${r.files} files reaching back ${r.reach_secs === null ? 'to the first' : ago(r.reach_secs)}`)
			.join('; ');

	const spaceText = (plan: Plan, spans: Reach[]): string => {
		const one = plan.dbs.reduce((n, db) => n + (sizes[db] ?? 0), 0);
		return spans.length && one ? `About ${bytes(one)} a backup, so about ${bytes(one * spans[1].files)} after a month.` : '';
	};

	const everyText = (secs: number): string => EVERY.find(([s]) => s === secs)?.[1] ?? `every ${ago(secs)}`;

	/** «12 backups of datum found, newest 2 h ago» — per database and device the destination holds. */
	function foundText(backups: Backup[]): string {
		if (!backups.length) return 'No backups there yet.';
		const by = new Map<string, Backup[]>();
		for (const b of backups) {
			const key = `${b.source_db}${b.mine ? '' : ` from ${b.owner}`}`;
			by.set(key, [...(by.get(key) ?? []), b]);
		}
		return [...by]
			.map(([what, list]) => `${list.length} backup${list.length === 1 ? '' : 's'} of ${what} found, newest ${ago(list[0].age_secs)} ago`)
			.join('; ');
	}

	async function perform(job: () => Promise<unknown>): Promise<void> {
		note = '';
		try {
			await job();
		} catch (e) {
			note = e instanceof Error ? e.message : String(e);
		}
	}

	const saveLocal = () =>
		perform(async () => {
			const r = await saveBackup();
			note = r.code === 0 ? r.stdout.split('\n').at(-1) ?? '' : r.stderr;
		});

	/** The `⚠` lines a restore printed, which go beside its success sentence (da-vceux). */
	const warnings = (r: RunResult) => r.stderr.split('\n').filter((l) => l.startsWith('⚠'));

	const restoreLocal = () =>
		perform(async () => {
			const r = await restoreFile(restoreArgv());
			note = r.code === 0 ? [r.stdout.split('\n').at(-1) || 'restored', ...warnings(r)].join(' ') : r.stderr;
			// The panes of this tab re-run on it, as after a typed command.
			await ran(r);
		});

	const removePlan = (plan: Plan) =>
		perform(async () => {
			const text = `Delete the plan ${plan.name} from this device? The backups already at its destination stay there.`;
			if (await confirm(text, 'delete')) await deletePlan(plan.name);
		});

	const restoreSlot = (plan: Plan, db: string, backup: Backup) =>
		perform(async () => {
			const r = await restoreFrom(plan, db, backup);
			if (!r) return;
			note = r.code === 0 ? [`${db} restored to its slot ${backup.slot} backup`, ...warnings(r)].join(' ') : r.stderr;
			await ran(r);
		});

	function setFresh(next: Partial<Plan>): void {
		fresh = { ...fresh, ...next };
		found = null;
		probe = null;
		void explain(fresh, '');
	}

	const look = () => perform(async () => (found = (await listed(fresh)).backups));

	const testIt = () =>
		perform(async () => {
			probe = null;
			probe = (await KINDS[fresh.kind].test?.(fresh.config)) ?? null;
		});

	const add = () =>
		perform(async () => {
			const name = fresh.name.trim();
			if (!name) throw new Error('name the plan');
			if ($plans.some((p) => p.name === name)) throw new Error(`there is already a plan named ${name}`);
			if (!fresh.dbs.length) throw new Error('choose a database to back up');
			const plan = { ...fresh, name };
			// Backups already there are said BEFORE the first write: a first
			// press that finds some stops at saying so, and the next adds.
			if (!found) {
				found = (await listed(plan)).backups;
				if (found.length) return;
			}
			await savePlan(plan);
			reach = { ...reach, [name]: reach[''] };
			fresh = draft(currentDb());
			slotsText = '';
			probe = null;
			found = null;
			// A new plan is due at once, unless the destination already has a backup this period.
			await check();
		});

	const toggleDb = (db: string) => setFresh({ dbs: fresh.dbs.includes(db) ? fresh.dbs.filter((d) => d !== db) : [...fresh.dbs, db] });
</script>

<Pane title="backups">
	<div class="backups" bind:this={body}>
		<section class="local">
			<h3>A file on this device</h3>
			<div class="row">
				<button type="button" use:act={'open'} data-backup="save" on:click={saveLocal}>Save a backup file</button>
				<button type="button" use:act={'open'} data-backup="restore-file" on:click={restoreLocal}>Restore from a file</button>
			</div>
		</section>

		<section class="about">
			<h3>Plans</h3>
			<p>
				A plan keeps a <b>Tower of Hanoi</b> rotation of backups at a destination: recent backups close together, older
				ones further apart, so a few files reach a long way back. Slot 0, the very first backup, is kept forever.
			</p>
			<p>
				A plan backs up while a datum.day tab is open — at its frequency, and once when a tab opens and it is overdue.
				A closed browser takes no backups.
			</p>
			<p class="warn"><b>Backups are not encrypted.</b> Use a private bucket.</p>
		</section>

		{#if note}<p class="note" role="status">{note}</p>{/if}

		{#each $plans as plan (plan.name)}
			{@const s = $status[plan.name]}
			<section class="plan" data-plan={plan.name}>
				<div class="row head">
					<b>{plan.name}</b>
					<span class="quiet"
						>{KINDS[plan.kind]?.label ?? plan.kind} · {plan.dbs.join(', ')} · {everyText(plan.every)} · {plan.slots
							? `${plan.slots} slots`
							: 'no cap'}</span
					>
					<button type="button" use:act={'run'} data-backup="now" disabled={s?.running} on:click={() => perform(() => backUp(plan))}
						>{s?.running ? 'Backing up…' : 'Back up now'}</button
					>
					<button type="button" use:act={'open'} data-backup="delete" on:click={() => removePlan(plan)}>Delete plan</button>
				</div>
				{#if reach[plan.name]}
					<p class="quiet explain">{reachText(plan, reach[plan.name])}. {spaceText(plan, reach[plan.name])}</p>
				{/if}
				{#if s?.error}<p class="error" role="alert">{s.error}</p>{/if}
				{#each plan.dbs as db}
					{@const last = s && newest(s, db)}
					<div class="db" data-restore={db}>
						<div class="row">
							<span>{db}</span>
							<span class="quiet" data-last
								>{last ? `last backup ${ago(ageOf(s, last))} ago` : 'no backup yet'}</span
							>
							{#if s?.at && overdue(plan, s, db)}<span class="overdue" data-overdue>overdue</span>{/if}
						</div>
						<ul>
							{#each s?.backups.filter((b) => b.source_db === db) ?? [] as backup (backup.folder + backup.file)}
								<li data-slot={backup.slot} data-from={backup.mine ? undefined : backup.owner}>
									<span class="age">{ago(ageOf(s, backup))} ago</span>
									<span class="quiet"
										>slot {backup.slot} · {backup.entry_count} entries · {bytes(backup.size)}{backup.mine ? '' : ` · from ${backup.owner}`}</span
									>
									<button type="button" use:act={'open'} on:click={() => restoreSlot(plan, db, backup)}>Restore</button>
								</li>
							{/each}
						</ul>
					</div>
				{/each}
			</section>
		{/each}

		{#if $plans.length > 1}
			<button type="button" use:act={'run'} data-backup="all" on:click={() => perform(async () => { for (const p of $plans) await backUp(p); })}
				>Back up all now</button
			>
		{/if}

		{#if fresh.kind}
			<section class="add">
				<h3>New plan</h3>
				<label>Name <input data-plan-field="name" bind:value={fresh.name} placeholder="hourly" /></label>
				<label
					>Destination
					<select
						data-plan-field="kind"
						value={fresh.kind}
						on:change={(e) => setFresh({ kind: e.currentTarget.value, config: defaults(e.currentTarget.value) })}
					>
						{#each offered(currentDb()) as kind}<option value={kind}>{KINDS[kind].label}</option>{/each}
					</select>
				</label>
				{#each KINDS[fresh.kind]?.fields ?? [] as field (field.key)}
					<label
						>{field.label}
						<input
							data-plan-field={field.key}
							type={field.secret ? 'password' : 'text'}
							autocomplete="off"
							placeholder={field.placeholder ?? ''}
							value={fresh.config[field.key] ?? ''}
							on:change={(e) => setFresh({ config: { ...fresh.config, [field.key]: e.currentTarget.value } })}
						/></label
					>
					{#if field.note}<p class="quiet note-field">{field.note}</p>{/if}
				{/each}
				{#if cors}
					<details class="cors">
						<summary>{cors.summary}</summary>
						<p class="quiet">{cors.note}</p>
						<pre data-cors>{cors.rules(location.origin)}</pre>
					</details>
				{/if}
				<fieldset>
					<legend>Databases</legend>
					{#each dbs as db}
						<label class="check"><input type="checkbox" checked={fresh.dbs.includes(db)} on:change={() => toggleDb(db)} />{db}</label>
					{/each}
				</fieldset>
				<label
					>Frequency
					<select value={fresh.every} on:change={(e) => setFresh({ every: Number(e.currentTarget.value) })}>
						{#each EVERY as [secs, label]}<option value={secs}>{label}</option>{/each}
					</select>
				</label>
				<label
					>Max slots
					<input
						type="number"
						min="1"
						placeholder="none — grows with time"
						bind:value={slotsText}
						on:change={() => setFresh({ slots: Number(slotsText) > 0 ? Math.floor(Number(slotsText)) : null })}
					/></label
				>
				{#if reach['']}<p class="quiet explain">{reachText(fresh, reach[''])}. {spaceText(fresh, reach[''])}</p>{/if}
				{#if found}<p class="found" data-found
						>{foundText(found)}{found.some((b) => b.mine) ? ' Add plan continues that rotation.' : found.length ? " Add plan keeps this device's own beside them." : ''}</p
					>{/if}
				{#if probe}
					<ul class="probe" data-probe>
						{#each probe as r}
							<li class:error={!r.ok} data-step={r.step}>{r.ok ? '✓' : '✗'} {r.step}{r.ok ? '' : ` failed — ${r.cause}`}</li>
						{/each}
					</ul>
				{/if}
				<div class="row">
					{#if KINDS[fresh.kind]?.test}
						<button type="button" use:act={'run'} data-backup="test" on:click={testIt}>Test connection</button>
					{/if}
					<button type="button" use:act={'show'} on:click={look}>Look for backups there</button>
					<button type="button" use:act={'run'} data-backup="add" on:click={add}>Add plan</button>
				</div>
			</section>
		{/if}
	</div>
</Pane>

<style>
	.backups {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		font-size: 0.85rem;
	}

	h3 {
		margin: 0 0 0.3rem;
		font-size: 0.8rem;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--quiet);
	}

	p {
		margin: 0.2rem 0;
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 0.6rem;
	}

	.quiet {
		color: var(--muted);
	}

	.warn,
	.overdue {
		color: var(--warn);
	}

	.error {
		color: #ff7b7b;
	}

	.plan {
		border-top: 1px solid var(--tint);
		padding-top: 0.5rem;
	}

	.db ul {
		list-style: none;
		margin: 0.2rem 0 0.4rem;
		padding: 0 0 0 0.8rem;
	}

	.db li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.2rem 0.6rem;
	}

	.age {
		min-width: 5rem;
	}

	.add {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		border-top: 1px solid var(--tint);
		padding-top: 0.5rem;
	}

	.add label {
		display: flex;
		gap: 0.5rem;
		align-items: center;
	}

	.note-field {
		margin-top: -0.2rem;
		font-size: 0.8rem;
	}

	.cors pre {
		background: var(--ground);
		border: 1px solid var(--tint);
		border-radius: 3px;
		padding: 0.4rem;
		overflow-x: auto;
		user-select: all;
	}

	.probe {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	fieldset {
		border: 1px solid var(--tint);
		border-radius: 4px;
		display: flex;
		flex-wrap: wrap;
		gap: 0.6rem;
	}

	input,
	select {
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 3px;
		font: inherit;
		padding: 0.2rem 0.4rem;
	}

	button {
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 3px;
		font: inherit;
		padding: 0.3rem 0.7rem;
		min-height: 24px;
		cursor: pointer;
	}

	/* The design language ($lib/act.ts): a DOUBLE outline acts at once, a
	   SINGLE one of the accent opens something. */
	button:global([data-act='run']) {
		border: 3px double var(--accent);
		padding: calc(0.3rem - 2px) calc(0.7rem - 2px);
		color: var(--ink-bright);
	}

	button:global([data-act='open']),
	button:global([data-act='show']) {
		border-color: var(--accent);
	}

	button:hover:not(:disabled) {
		color: var(--accent-bright);
	}

	button:disabled {
		opacity: 0.6;
		cursor: default;
	}
</style>
