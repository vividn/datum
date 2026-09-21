<!--
	The stats widget — a DRAFT to shape from (dashboard D3, da-n7r0).

	Pick a field (and a state, if it has them) and a range; the body is what
	`datum stats` prints for that selection, through CliOutput. The header
	above it is the same selection's `--json`, laid out as tiles — a glimpse
	of what a designed view could be, and still the CLI's numbers: nothing
	here computes a statistic.

	`settings` is the widget's state and `change` hands a new one out, so the
	dashboard engine can save it with the layout.

	A FIELD PILL on the bar overrides the field in it (da-8810n, vividn: "So
	stats widget would filter to this field"). Overrides, and does not
	write: the pill is a filter a reader holds for a moment and the settings
	are what the layout keeps, so a picked field is still picked when the
	pill clears. Its state goes with it — the pilled field has states of its
	own, and the one picked for another field is not among them.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { browser } from '$app/environment';
	import { fieldPill } from './bar.js';
	import CliOutput from './CliOutput.svelte';
	import { run, type RunResult } from './run.js';
	import type { FieldListing } from './fields.js';
	import {
		DEFAULT_STATS,
		statesOf,
		statsArgv,
		type FieldStats,
		type StateStats,
		type StatsRange,
		type StatsSettings
	} from './stats.js';

	export let settings: StatsSettings = DEFAULT_STATS;
	/** Any change re-runs everything: a new row changes the numbers. */
	export let epoch = 0;

	const dispatch = createEventDispatcher<{ change: StatsSettings }>();

	const PERIODS: { range: StatsRange; label: string }[] = [
		{ range: 'all', label: 'all time' },
		{ range: 'today', label: 'today' },
		{ range: 'week', label: 'week' },
		{ range: 'month', label: 'month' }
	];

	/** The custom range's two inputs, by the flag each one fills. */
	const EDGES = ['since', 'until'] as const;

	let fields: FieldListing[] = [];
	let result: RunResult | null = null;
	let summary: FieldStats | StateStats | null = null;
	/** How many runs have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	/**
	 * What the widget SHOWS: its settings, narrowed by a field pill.
	 *
	 * Not `shown`, which is this file's newest-run-on-screen counter.
	 */
	$: view = $fieldPill ? { ...settings, field: $fieldPill, state: undefined } : settings;
	$: argv = statsArgv(view);
	$: custom = typeof settings.range === 'object' ? settings.range : null;
	$: states = statesOf(fields.find((f) => f.field === view.field));
	$: tiles = summary ? tilesOf(summary) : [];

	// Not on mount: there is no `run` outside the browser.
	$: if (browser) void list(epoch);
	$: if (browser) void refresh(argv, epoch);

	/** Listings started and the newest one on screen — `list`'s own pair. */
	let listStarted = 0;
	let listShown = -1;

	async function list(_epoch: number): Promise<void> {
		// Last started wins here too: two epoch bumps in quick succession put
		// two listings in flight, and the older answer must not paint over
		// the newer one (review thread on #165).
		const at = ++listStarted;
		const listed = await run(['fields', '--json']);
		if (at < listShown) return;
		listShown = at;
		if (listed.code === 0) fields = parsed(listed.stdout) ?? fields;
	}

	/**
	 * The CLI's JSON, or null when it is not JSON. A process killed
	 * mid-write leaves partial stdout, and a widget that throws on it is a
	 * blank pane with a console error nobody sees.
	 */
	function parsed<T>(text: string): T | null {
		try {
			return JSON.parse(text) as T;
		} catch {
			return null;
		}
	}

	async function refresh(argv: string[] | null, _epoch: number): Promise<void> {
		const at = ++started;
		const [text, json] = argv ? await Promise.all([run(argv), run([...argv, '--json'])]) : [null, null];
		// Last started wins, not last to return (see CliPane.svelte).
		if (at < shown) return;
		shown = at;
		result = text;
		summary = json?.code === 0 ? parsed(json.stdout) : null;
	}

	function set(next: Partial<StatsSettings>): void {
		settings = { ...settings, ...next };
		dispatch('change', settings);
	}

	function tilesOf(s: FieldStats | StateStats): [string, string][] {
		if ('state' in s)
			return [
				['entries', String(s.count)],
				['time', duration(s.duration_seconds)],
				['share', s.ratio === null ? '—' : `${(s.ratio * 100).toFixed(1)}%`]
			];
		return [
			['entries', String(s.total_count)],
			['time', duration(s.total_duration_seconds)],
			['states', String(s.states.length)]
		];
	}

	function duration(secs: number): string {
		const h = Math.floor(secs / 3600);
		const m = Math.floor((secs % 3600) / 60);
		return h ? `${h}h ${m}m` : m ? `${m}m` : `${secs}s`;
	}
</script>

<div class="stats">
	<aside>
		<div class="chips" role="group" aria-label="fields">
			{#each fields as { field } (field)}
				<button
					class="field"
					class:on={view.field === field}
					aria-pressed={view.field === field}
					on:click={() => set({ field, state: undefined })}>{field}</button
				>
			{:else}
				<p class="quiet">nothing tracked yet</p>
			{/each}
		</div>
		{#if states.length}
			<div class="chips" role="group" aria-label="states of {view.field}">
				{#each states as state (state)}
					<button
						class="state"
						class:on={view.state === state}
						aria-pressed={view.state === state}
						on:click={() => set({ state: view.state === state ? undefined : state })}
						>{state}</button
					>
				{/each}
			</div>
		{/if}
	</aside>

	<section>
		<div class="chips range" role="group" aria-label="time range">
			{#each PERIODS as { range, label } (range)}
				<button
					class:on={settings.range === range}
					aria-pressed={settings.range === range}
					on:click={() => set({ range })}>{label}</button
				>
			{/each}
			<button
				class:on={!!custom}
				aria-pressed={!!custom}
				on:click={() => set({ range: { since: '', until: '' } })}>custom</button
			>
		</div>
		{#if custom}
			<div class="custom">
				{#each EDGES as edge (edge)}
					<label
						>{edge}
						<input
							type="datetime-local"
							value={custom[edge]}
							on:change={(e) => set({ range: { ...custom, [edge]: e.currentTarget.value } })}
						/></label
					>
				{/each}
			</div>
		{/if}

		{#if !argv}
			<p class="quiet">pick a field</p>
		{:else}
			<code class="argv">{argv.join(' ')}</code>
			{#if tiles.length}
				<dl class="tiles">
					{#each tiles as [label, value] (label)}
						<div class="tile {label}"><dt>{label}</dt><dd>{value}</dd></div>
					{/each}
				</dl>
			{/if}
			<div class="body">
				{#if result?.stdout}
					<CliOutput text={result.stdout} />
				{:else if result?.stderr}
					<CliOutput text={result.stderr} stream="stderr" error={result.code !== 0} />
				{/if}
			</div>
		{/if}
	</section>
</div>

<style>
	.stats {
		display: grid;
		grid-template-columns: minmax(8rem, 14rem) minmax(0, 1fr);
		gap: 1rem;
		font-family: monospace;
	}

	/* The phone breakpoint the day frame uses: the fields stack above. */
	@media (max-width: 720px) {
		.stats {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	aside,
	section {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		min-width: 0;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}

	button {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 999px;
		padding: 0.25rem 0.7rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.85rem;
		cursor: pointer;
	}

	button:hover,
	button.on {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	button.state {
		font-size: 0.8rem;
	}

	.custom {
		display: flex;
		flex-wrap: wrap;
		gap: 0.6rem;
		color: var(--muted);
		font-size: 0.8rem;
	}

	input {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
		color-scheme: dark;
	}

	.argv {
		color: var(--muted);
		font-size: 0.75rem;
	}

	.tiles {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin: 0;
	}

	.tile {
		border: 1px solid var(--rule);
		border-radius: 6px;
		padding: 0.4rem 0.8rem;
	}

	dt {
		color: var(--muted);
		font-size: 0.7rem;
	}

	dd {
		margin: 0;
		color: var(--ink-bright);
		font-size: 1.2rem;
	}

	.body {
		overflow-x: auto;
	}

	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
</style>
