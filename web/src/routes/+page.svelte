<!--
	The day frame — the shell's single screen, in three stacked regions:

	  widgets   one-tap logging buttons                     (da-nz29)
	  panes     dayview | nowview | tail, CLI-rendered      (da-1x72)
	  repl      the command line

	Every region talks to datum through `run(argv)` in $lib/run.ts and
	nothing else, so the wasm CLI (da-1vat) lands behind them without
	touching this file.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import Pane from '$lib/Pane.svelte';
	import WidgetBar from '$lib/WidgetBar.svelte';
	import { initDb, getRecentEntries, getAllFields, getFieldSpecs, getObservedStates } from '$lib/db.js';
	import { entryLine } from '$lib/runners/local.js';
	import { run } from '$lib/run.js';
	import { shellSplit } from '$lib/shellSplit.js';
	import { widgetsFor, type Widget } from '$lib/widgets.js';
	import type { RunResult } from '$lib/run.js';
	import type { Entry } from '$lib/types.js';

	/** How many past commands the REPL keeps on screen. */
	const HISTORY_MAX = 20;

	let input = '';
	let ready = false;
	let history: RunResult[] = [];
	let log: HTMLElement;
	let recent: Entry[] = [];
	let widgets: Widget[] = [];

	onMount(async () => {
		await initDb();
		ready = true;
		await refresh();
	});

	async function refresh() {
		const [fields, specs, observed] = await Promise.all([
			getAllFields(),
			getFieldSpecs(),
			getObservedStates()
		]);
		widgets = widgetsFor(fields, specs, observed);
		recent = await getRecentEntries(50);
	}

	/**
	 * The app's one path to datum: everything the user does — typing a line,
	 * pressing a widget — becomes an argv, runs, and lands in the REPL log.
	 *
	 * The log echoes `result.argv`, the array the runner actually received,
	 * so a widget press teaches exactly the command it ran (da-nz29). A
	 * prettier string rebuilt from the press could drift from it; this
	 * cannot.
	 */
	async function execute(argv: string[]) {
		const result = await run(argv);
		history = [...history, result].slice(-HISTORY_MAX);
		await refresh();
		await tick();
		if (log) log.scrollTop = log.scrollHeight;
		return result;
	}

	async function submit() {
		if (!input.trim()) return;
		const result = await execute(shellSplit(input));
		if (result.code === 0) input = '';
	}
</script>

<div class="frame">
	<WidgetBar {widgets} disabled={!ready} on:run={(e) => execute(e.detail)} />

	<div class="panes">
		<Pane title="dayview">
			<p class="pending">the CLI's dayview renders here</p>
		</Pane>

		<Pane title="nowview">
			<p class="pending">the CLI's nowview renders here</p>
		</Pane>

		<!-- A plain list until da-1x72 renders the real `datum tail` output. -->
		<Pane title="tail">
			{#if recent.length === 0}
				<p class="pending">nothing logged yet — try <code>switch mood happy</code></p>
			{:else}
				{#each recent as entry (entry.hid)}
					<div class="row">{entryLine(entry)}</div>
				{/each}
			{/if}
		</Pane>
	</div>

	<!--
		REPL: one line in, one RunResult out — and every command the widget
		bar runs echoed here beside the ones the user typed, so the bar
		teaches the CLI instead of hiding it.
	-->
	<form class="repl" on:submit|preventDefault={submit}>
		{#if history.length > 0}
			<div class="history" bind:this={log}>
				{#each history as result, i (i)}
					<div class="echo"><span class="prompt">❯</span> {result.argv.join(' ')}</div>
					{#if result.stderr || result.stdout}
						<output class:failed={result.code !== 0}>{result.stderr || result.stdout}</output>
					{/if}
				{/each}
			</div>
		{/if}
		<div class="line">
			<span class="prompt">❯</span>
			<input
				type="text"
				bind:value={input}
				disabled={!ready}
				placeholder={ready ? 'occur coffee 2 cups' : 'opening database…'}
				spellcheck="false"
				autocapitalize="none"
				autocomplete="off"
			/>
		</div>
	</form>
</div>

<style>
	.frame {
		display: grid;
		grid-template-rows: auto minmax(0, 1fr) auto;
		gap: 0.75rem;
		height: 100%;
	}

	.panes {
		display: grid;
		/* Three across on a wide screen; stacked and scrolled as a column on
		   a phone, where three side-by-side panes would each be unreadable. */
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.75rem;
		min-height: 0;
	}

	@media (max-width: 720px) {
		.panes {
			grid-template-columns: minmax(0, 1fr);
			grid-auto-rows: minmax(8rem, auto);
			overflow-y: auto;
		}
	}

	.pending {
		margin: 0;
		color: #444466;
		font-style: italic;
	}

	.pending code {
		font-style: normal;
		color: #7c7cff;
	}

	.row {
		padding: 0.1rem 0;
		white-space: pre-wrap;
	}

	.repl {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.history {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		/* Capped and self-scrolling: a long session's echoes must not push
		   the input line off the bottom of the frame. */
		max-height: 8rem;
		overflow-y: auto;
	}

	.echo {
		font-size: 0.85rem;
		color: #c8c8e8;
	}

	output {
		font-size: 0.85rem;
		white-space: pre-wrap;
		color: #4caf88;
	}

	output.failed {
		color: #f4645a;
	}

	.line {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
		padding: 0.45rem 0.7rem;
	}

	.prompt {
		color: #7c7cff;
	}

	input {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: #e0e0ff;
		font: inherit;
		font-size: 1rem;
	}

	input::placeholder {
		color: #444466;
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
