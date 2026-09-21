<!--
	The stats widget (da-n7r0, revamped by da-ptuwr).

	It shows EVERY FIELD until another widget narrows it. vividn, 2026-09-21:
	"Remove the field selector in there, and just show general stats across
	all fields until a field filter is applied via another widget". So there
	is no picker here at all: the body is what `datum stats` prints — the
	ranked table of fields with no selector, one field's own states with
	one — and the only thing that chooses between them is the FIELD PILL
	the fields widget mints (`da-8810n`, $lib/bar.ts).

	What the range chips choose is the WINDOW, which is the widget's whole
	settings now (`StatsSettings`). `change` hands a new one out and it
	lands in THIS SESSION (da-vhwq, `layout/session.ts`): it changes what
	this tab is showing and the layout does not hear of it, so a reload
	starts back at what the layout says and «Save options with layout» is
	the one thing that hands a window over.

	Above the table is the same numbers as a picture: the ratio column as
	one stacked bar in the fields' own colours (`color` on `stats --json`,
	resolved by the CLI exactly as it paints them elsewhere). It is the
	answer the 240×140 pane shows FIRST — a whole database in ten pixels —
	and the table under it is what the reader scrolls. Narrowed to one
	field the subject is that field, so the tiles take the bar's place:
	its entries, its time, how many states it has been in.

	Nothing here computes a statistic.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { browser } from '$app/environment';
	import { fieldPill } from './bar.js';
	import { inkOver } from './fields.js';
	import CliOutput from './CliOutput.svelte';
	import { run, type RunResult } from './run.js';
	import {
		DEFAULT_STATS,
		isAllFields,
		statsArgv,
		type AllFieldsStats,
		type FieldStats,
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

	let result: RunResult | null = null;
	let summary: FieldStats | AllFieldsStats | null = null;
	/** How many runs have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	$: argv = statsArgv(settings, $fieldPill);
	$: custom = typeof settings.range === 'object' ? settings.range : null;
	$: all = isAllFields(summary) ? summary : null;
	$: tiles = summary && !all ? tilesOf(summary as FieldStats) : [];

	// Not on mount: there is no `run` outside the browser.
	$: if (browser) void refresh(argv, epoch);

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

	async function refresh(argv: string[], _epoch: number): Promise<void> {
		const at = ++started;
		const [text, json] = await Promise.all([run(argv), run([...argv, '--json'])]);
		// Last started wins, not last to return (see CliPane.svelte).
		if (at < shown) return;
		shown = at;
		result = text;
		summary = json.code === 0 ? parsed(json.stdout) : null;
	}

	function set(next: Partial<StatsSettings>): void {
		settings = { ...settings, ...next };
		dispatch('change', settings);
	}

	function tilesOf(s: FieldStats): [string, string][] {
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

	/**
	 * The share bar's segments: the fields that have a share at all. A
	 * field with none — a point field, whose entries take no time — is not
	 * drawn rather than drawn as nothing; the table below it still counts
	 * it.
	 */
	$: shares = (all?.fields ?? []).filter((f) => (f.ratio ?? 0) > 0);
</script>

<div class="stats">
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

	{#if shares.length}
		<div class="share" role="img" aria-label="share of tracked time by field">
			{#each shares as f (f.field)}
				<span
					class="slice"
					style:flex-grow={f.ratio}
					style:background={f.color}
					style:color={inkOver(f.color)}
					title="{f.field} — {((f.ratio ?? 0) * 100).toFixed(1)}%">{f.field}</span
				>
			{/each}
		</div>
	{/if}
	{#if tiles.length}
		<dl class="tiles">
			{#each tiles as [label, value] (label)}
				<div class="tile {label}"><dt>{label}</dt><dd>{value}</dd></div>
			{/each}
		</dl>
	{/if}

	<code class="argv">{argv.join(' ')}</code>
	<div class="body">
		{#if result?.stdout}
			<CliOutput text={result.stdout} />
		{:else if result?.stderr}
			<CliOutput text={result.stderr} stream="stderr" error={result.code !== 0} />
		{/if}
	</div>
</div>

<style>
	.stats {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		min-width: 0;
		font-family: monospace;
		/* The pane's title lies OVER its content on the bar's edge
		   (da-13bc), and the frame says how far in `--under-top` — "for a
		   widget whose own controls sit in that corner to keep clear of
		   it" (PaneFrame.svelte). The range chips are that corner's
		   controls, so they keep clear; a widget with no bar above it has
		   no such edge and the variable is not set. */
		padding-top: var(--under-top, 0px);
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

	/* One row, the fields' shares side by side. The name rides inside its
	   own slice and is clipped by it, so a thin slice is a colour and a
	   wide one is a colour with a word in it — never a label that pushes
	   the bar wider than the pane. */
	.share {
		display: flex;
		border-radius: 3px;
		overflow: hidden;
		min-height: 1.1rem;
	}

	.slice {
		flex-basis: 0;
		min-width: 0;
		overflow: hidden;
		white-space: nowrap;
		text-align: center;
		font-size: 0.7rem;
		line-height: 1.1rem;
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
</style>
