<!--
	A pane that renders one datum command.

	Thin on purpose: it runs `spec.argv` through the `run(argv)` seam and
	shows the text that comes back. No parsing, no formatting, no database
	— everything it displays was produced by datum itself.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import TailTable from './TailTable.svelte';
	import { run, type RunResult } from './run.js';
	import { paneArgv, wantsBox, type PaneBox, type PaneSpec } from './panes.js';

	export let spec: PaneSpec;
	/** Any change re-runs the command. The frame bumps it after each run. */
	export let epoch = 0;
	/**
	 * Stop re-running a command once a run of it has WRITTEN (da-wb2n).
	 *
	 * For a pane whose command the reader types: `occur coffee` there would
	 * log a coffee on every epoch, and with other tabs refreshing on a write,
	 * two tabs would feed each other. So the first run is the one the reader
	 * asked for, and a run the worker reports as `changed` puts the pane on
	 * ▶ from then on. By effect, never by verb — the frame's own rule
	 * (+page.svelte) — so `get`, which moves the last-docs pointer, is on ▶
	 * too. A new `spec` is a new command, and runs once again.
	 */
	export let guard = false;

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
	// re-drawn at the new size rather than stretched to it. Observed whatever
	// the command, since a typed one can become `dayview` (da-wb2n).
	onMount(() => {
		if (!body) return;
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
	// the epoch and the box together, so a resize re-runs a drawn view the
	// same way a write does — and only a drawn view: for any other command
	// `sized` stays null, and an unchanged null invalidates nothing.
	// The tail table reads its own listing (see TailTable.svelte); every
	// other pane is one command through the seam.
	$: sized = wantsBox(spec.argv) ? box : null;
	$: request = { argv: spec.view === 'tail-table' ? null : paneArgv(spec.argv, sized), epoch };
	$: manual = guard && writer === spec;
	$: if (browser && request.argv && !manual) void refresh(request.argv);

	/** The spec whose run wrote: while it is `spec`, the pane runs on ▶ only. */
	let writer: PaneSpec | null = null;
	/** The spec with an automatic run out whose effect is not known yet. */
	let probing: PaneSpec | null = null;
	/** Whether another automatic run was asked for while that one was out. */
	let deferred = false;

	async function refresh(argv: string[], pressed = false): Promise<void> {
		const of = spec;
		if (guard && !pressed) {
			// One at a time until the pane knows whether the command writes:
			// a second run sent before the first came back is the second write.
			if (probing === of) return void (deferred = true);
			probing = of;
		}
		const at = ++started;
		const next = await run(argv);
		if (guard && next.changed) writer = of;
		if (probing === of) {
			probing = null;
			const again = deferred && spec === of && writer !== of;
			deferred = false;
			if (again && request.argv) void refresh(request.argv);
		}
		// Type fast enough — or drag the window — and two refreshes are in
		// flight at once; they can finish out of order, and the loser would
		// paint the older answer over the newer one. Last started wins, not
		// last to return.
		if (at < shown) return;
		shown = at;
		result = next;
	}

	function press(): void {
		if (request.argv) void refresh(request.argv, true);
	}
</script>

<Pane title={spec.title} bind:body>
	<slot name="title" slot="title">{spec.title}</slot>
	{#if manual}
		<p class="quiet">
			this command writes — it runs only when you press
			<button class="play" title="run it" on:click={press}>▶</button>
		</p>
	{/if}
	{#if spec.view === 'tail-table'}
		<!-- The one pane whose content is a grid: `datum tail` IS a table,
		     and this lays the CLI's own cells out in a real one rather than
		     showing the padding a terminal would need (da-6ec2). -->
		<TailTable {epoch} empty={spec.empty ?? ''} />
	{:else if !result}
		<p class="quiet">…</p>
	{:else if result.stdout}
		<CliOutput text={result.stdout} />
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
	.play {
		font: inherit;
		font-style: normal;
		color: #7c7cff;
		background: none;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
		cursor: pointer;
	}
</style>
