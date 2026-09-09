<!--
	A pane that renders one datum command.

	Thin on purpose: it runs `spec.argv` through the `run(argv)` seam and
	shows the text that comes back. No parsing, no formatting, no database
	— everything it displays was produced by datum itself, so the wasm CLI
	(da-1vat) upgrades every pane at once without touching this file.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import TailTable from './TailTable.svelte';
	import { run, EXIT_UNKNOWN_COMMAND, type RunResult } from './run.js';
	import { paneArgv, type PaneBox, type PaneSpec } from './panes.js';

	export let spec: PaneSpec;
	/** Any change re-runs the command. The frame bumps it after each run. */
	export let epoch = 0;

	let result: RunResult | null = null;
	/** How many runs have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	let body: HTMLDivElement | undefined;
	/** The body's content box, once measured. See `paneArgv`. */
	let box: PaneBox | null = null;

	// The content box, not the element's own width: the body is padded, and
	// a band authored 19px wider than the box it is shown in is scaled back
	// down — which is the bug (da-m0cj). ResizeObserver reports the content
	// box directly, and reports it again on every resize, so the band is
	// re-drawn at the new size rather than stretched to it.
	onMount(() => {
		if (!spec.sized || !body) return;
		const observer = new ResizeObserver(([entry]) => {
			// Whole pixels, rounded down: a band a fraction of a pixel wider
			// than its box is a band the browser scales, and the text with it.
			const width = Math.floor(entry.contentRect.width);
			const height = Math.floor(entry.contentRect.height);
			if (box?.width !== width || box?.height !== height) box = { width, height };
		});
		observer.observe(body);
		return () => observer.disconnect();
	});

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser. The request is rebuilt from
	// the epoch and the box together, so a resize re-runs the command the
	// same way a write does.
	// The tail table reads its own listing (see TailTable.svelte); every
	// other pane is one command through the seam.
	$: request = { argv: spec.view === 'tail-table' ? null : paneArgv(spec, box), epoch };
	$: if (browser && request.argv) void refresh(request.argv);

	async function refresh(argv: string[]): Promise<void> {
		const at = ++started;
		const next = await run(argv);
		// Type fast enough — or drag the window — and two refreshes are in
		// flight at once; they can finish out of order, and the loser would
		// paint the older answer over the newer one. Last started wins, not
		// last to return.
		if (at < shown) return;
		shown = at;
		result = next;
	}
</script>

<Pane title={spec.title} bind:body>
	{#if spec.view === 'tail-table'}
		<!-- The one pane whose content is a grid: `datum tail` IS a table,
		     and this lays the CLI's own cells out in a real one rather than
		     showing the padding a terminal would need (da-6ec2). -->
		<TailTable {epoch} empty={spec.empty ?? ''} />
	{:else if !result}
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
