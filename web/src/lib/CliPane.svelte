<!--
	A pane that renders one datum command.

	Thin on purpose: it runs `spec.argv` through the `run(argv)` seam and
	shows the text that comes back. No parsing, no formatting, no database
	— everything it displays was produced by datum itself, so the wasm CLI
	(da-1vat) upgrades every pane at once without touching this file.
-->
<script lang="ts">
	import { browser } from '$app/environment';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { run, EXIT_UNKNOWN_COMMAND, type RunResult } from './run.js';
	import type { PaneSpec } from './panes.js';

	export let spec: PaneSpec;
	/** Any change re-runs the command. The frame bumps it after each run. */
	export let epoch = 0;

	let result: RunResult | null = null;
	/** The newest epoch already on screen — see the ordering note below. */
	let shown = -1;

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser.
	$: if (browser) void refresh(epoch);

	async function refresh(at: number): Promise<void> {
		const next = await run(spec.argv);
		// Type fast enough and two refreshes are in flight at once; they can
		// finish out of order, and the loser would paint the older database
		// over the newer one. Last epoch wins, not last to return.
		if (at < shown) return;
		shown = at;
		result = next;
	}
</script>

<Pane title={spec.title}>
	{#if !result}
		<p class="quiet">…</p>
	{:else if result.stdout}
		<CliOutput text={result.stdout} />
	{:else if result.code === EXIT_UNKNOWN_COMMAND}
		<!-- The TypeScript stand-in does not answer this command; the real
		     CLI will. Not an error the reader can do anything about, so it
		     is worded and coloured as a note. -->
		<p class="quiet">{result.stderr}</p>
	{:else if result.stderr}
		<CliOutput text={result.stderr} error />
	{:else if spec.empty}
		<p class="quiet">{spec.empty}</p>
	{/if}
</Pane>

<style>
	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}
</style>
