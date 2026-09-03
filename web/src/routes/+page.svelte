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
	import { onMount } from 'svelte';
	import Pane from '$lib/Pane.svelte';
	import { initDb, getRecentEntries } from '$lib/db.js';
	import { entryLine } from '$lib/runners/local.js';
	import { runLine } from '$lib/run.js';
	import type { RunResult } from '$lib/run.js';
	import type { Entry } from '$lib/types.js';

	let input = '';
	let ready = false;
	let last: RunResult | null = null;
	let recent: Entry[] = [];

	onMount(async () => {
		await initDb();
		ready = true;
		recent = await getRecentEntries(50);
	});

	async function submit() {
		if (!input.trim()) return;
		last = await runLine(input);
		if (last.code === 0) input = '';
		recent = await getRecentEntries(50);
	}
</script>

<div class="frame">
	<!-- widget bar: da-nz29 fills this region -->
	<div class="widgets"></div>

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

	<!-- REPL: one line in, one RunResult out. da-1x72 grows history + hints. -->
	<form class="repl" on:submit|preventDefault={submit}>
		{#if last && (last.stderr || last.stdout)}
			<output class:failed={last.code !== 0}>{last.stderr || last.stdout}</output>
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

	.widgets {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
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
