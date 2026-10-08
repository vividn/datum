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
	field the subject is that field, so the bar goes: a field has no
	fields to divide.

	OVER it, THE HEAD, which both states have (da-vpuxg). Whatever the
	subject is, the figures are how much of it there is: entries, time,
	and how many parts — the states a field has been in, the fields a
	database has logged — and since when, the first entry the window
	counts (`earliest`, da-rcmwc). Un-narrowed that is the answer to "nothing is
	selected", which before this was a bar and a table and no headline at
	all:

	    entries 128 · tracked 42h30m · fields 9 · since 4 Sep   datum · this month · stats --month

	and the quiet half of the line is there because a figure whose scope
	the reader has to guess at is worse than no figure — these are ONE
	database's, over the window the chips are holding, and a window leaves
	out a field with nothing in it.

	The head is ONE LINE, and it holds the argv that used to have a line
	of its own, because the widget's height belongs to the table: at the
	240×140 pane — and at the 390×300 one da-ptuwr measured — a headline
	with a line to itself is two or three fewer fields in view. Where even
	one line will not hold both, the source wraps under the figures.

	Nothing here computes a statistic: every number is `stats --json`'s
	own, from the one command the widget already runs.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { browser } from '$app/environment';
	import { fieldPill } from './bar.js';
	import { inkOver } from './fields.js';
	import CliOutput from './CliOutput.svelte';
	import { run, type RunResult } from './run.js';
	import { showAllArgv } from './calls.js';
	import { shellJoin } from './shellSplit.js';
	import { currentDb } from './database.js';
	import {
		DEFAULT_STATS,
		fieldTiles,
		isAllFields,
		statsArgv,
		summaryTiles,
		windowLabel,
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
	$: tiles = all ? summaryTiles(all) : summary ? fieldTiles(summary as FieldStats) : [];

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

	/**
	 * ONE computation per refresh (da-cddng): `--show all` prints the table
	 * and then the `--json` line under it, so the body is the CLI's own text
	 * and the head is read off the last line. Two runs — the text, then
	 * `--json` — cost the queue the whole computation twice.
	 */
	async function refresh(argv: string[], _epoch: number): Promise<void> {
		const at = ++started;
		const r = await run(showAllArgv(argv), undefined, undefined, undefined, undefined, undefined, true);
		// Last started wins, not last to return (see CliPane.svelte).
		if (at < shown) return;
		shown = at;
		const body = r.stdout.replace(/\n$/, '');
		const cut = body.lastIndexOf('\n') + 1;
		const ok = r.code === 0;
		result = ok ? { ...r, stdout: r.stdout.slice(0, cut) } : r;
		summary = ok ? parsed(body.slice(cut)) : null;
	}

	function set(next: Partial<StatsSettings>): void {
		settings = { ...settings, ...next };
		dispatch('change', settings);
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

	<div class="head">
		{#if tiles.length}
			<dl class="tiles">
				{#each tiles as [label, value] (label)}
					<div class="tile {label}"><dt>{label}</dt><dd>{value}</dd></div>
				{/each}
			</dl>
		{/if}
		<span class="source">
			{#if all}<span class="scope">{currentDb()} · {windowLabel(settings.range)}</span>{/if}
			<code class="argv">{shellJoin(argv)}</code>
		</span>
	</div>

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
		/* Every row here is one the table below it does not get. */
		gap: 0.5rem;
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

	/* The figures and where they come from, on ONE line: the widget's
	   whole height belongs to the table under it, and a headline that
	   takes a line of its own takes it from there. On a pane too narrow
	   for both the source wraps under the figures, which costs the line
	   back but only where the line was never going to fit anyway. */
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		/* Tight enough that `since` and the month's source — `datum · this
		   month · stats --month`, the default since da-80do8 — share one
		   line at 390px; at 0.8rem they missed it by 2px. */
		gap: 0 0.5rem;
	}

	/* The figures wrap one by one among the head's items, not as a block:
	   a figure that will not fit beside the others goes down onto the
	   source's line, which is there anyway on a phone, rather than onto a
	   line of its own (da-rcmwc — at 390px `since` cost the table a line). */
	.tiles {
		display: contents;
	}

	.source {
		margin-left: auto;
	}

	.tile {
		display: flex;
		align-items: baseline;
		gap: 0.3rem;
	}

	/* A dot between the figures, drawn rather than written: the tiles are
	   a list, and a separator is not one of its words. */
	.tile + .tile::before {
		content: '·';
		color: var(--rule);
	}

	dt {
		color: var(--muted);
		font-size: 0.75rem;
	}

	dd {
		margin: 0;
		color: var(--ink-bright);
		font-size: 0.95rem;
	}

	/* Which database, over which window, by which command — said rather
	   than assumed, and quieter than the numbers it qualifies. The dot
	   before the argv is drawn and not written, for the same reason the
	   ones between the figures are: `.argv` is the argv, exactly. */
	.source {
		display: flex;
		gap: 0.3rem;
		color: var(--muted);
		font-size: 0.75rem;
	}

	.scope + .argv::before {
		content: '· ';
		color: var(--rule);
	}

	.body {
		overflow-x: auto;
	}
</style>
