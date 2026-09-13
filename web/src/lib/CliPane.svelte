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
	import { record } from './replStore.js';
	import { run, type RunResult } from './run.js';
	import { paneArgv, wantsBox, type PaneBox, type PaneSpec } from './panes.js';
	import { settle } from './layout/resizing.js';

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
	//
	// Through `settle`: the observer fires on every frame of a border drag,
	// and a redraw per frame was the jitter (da-jgrn). The box lands once
	// the drag is dropped, or once a burst of resizes has been over for a
	// beat; until then the last picture stays, scaled to the box.
	onMount(() => {
		if (!body) return;
		const settled = settle<PaneBox>((next) => {
			if (box?.width !== next.width || box?.height !== next.height) box = next;
		});
		const observer = new ResizeObserver(([entry]) => {
			// Whole pixels, rounded down: a band a fraction of a pixel wider
			// than its box is a band the browser scales, and the text with it.
			settled.push({
				width: Math.floor(entry.contentRect.width),
				height: Math.floor(entry.contentRect.height)
			});
		});
		observer.observe(body);
		return () => {
			observer.disconnect();
			settled.stop();
		};
	});

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser. The request is rebuilt from
	// the epoch and the box together, so a resize re-runs a drawn view the
	// same way a write does — and only a drawn view: for any other command
	// `sized` stays null, and an unchanged null invalidates nothing.
	// The tail table reads its own listing, sized to the box (see
	// TailTable.svelte); every other pane is one command through the seam.
	//
	// By the spec's WORDS, not the spec: the layout hands a pane its spec
	// afresh on every change to the tree (a spread prop), and Svelte counts
	// an object handed over again as changed — so a request built from
	// `spec` itself re-ran every drawn pane on every nudge of a border
	// (da-jgrn). Strings and booleans are compared by value, and stay put.
	$: words = JSON.stringify(spec.argv);
	$: table = spec.view === 'tail-table';
	$: drawn = wantsBox(spec.argv);
	$: sized = drawn ? box : null;
	$: request = { argv: table ? null : paneArgv(JSON.parse(words) as string[], sized), epoch };
	$: manual = guard && writer === spec;
	$: if (browser && request.argv && !manual) void refresh(request.argv);

	/**
	 * The spec whose run wrote: while it is `spec`, the pane runs on ▶ only.
	 * A prop so a pane that keeps its command across a mount can keep this
	 * with it — known, not learned again by writing once more (da-v36y).
	 */
	export let writer: PaneSpec | null = null;
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
		if (guard && next.changed?.length) writer = of;
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

	// A press is a command the reader asked for, like a line typed: into
	// the shared history and the bottom bar's receipt (da-0a5o). The
	// automatic runs are the pane's own business and record nothing.
	async function press(): Promise<void> {
		const argv = request.argv;
		if (!argv) return;
		const ranAt = Date.now();
		await refresh(argv, true);
		if (result) record(argv.join(' '), ranAt, result);
	}
</script>

<Pane title={spec.title} bind:body>
	<slot name="title" slot="title">{spec.title}</slot>
	{#if manual}
		<p class="quiet">
			this command writes — it runs only when you press
			<button type="button" class="play" title="run it" aria-label="run it" on:click={press}>▶</button>
		</p>
	{/if}
	{#if spec.view === 'tail-table'}
		<!-- The one pane whose content is a grid: `datum tail` IS a table,
		     and this lays the CLI's own cells out in a real one rather than
		     showing the padding a terminal would need (da-6ec2). -->
		<!-- The box says how many rows fit and the body is where the reader
		     scrolls: `tail -w` in a pane (da-lan0). -->
		<TailTable {epoch} empty={spec.empty ?? ''} {box} scroller={body} />
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
		color: var(--quiet);
		font-style: italic;
	}
	.play {
		font: inherit;
		font-style: normal;
		color: var(--accent);
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		cursor: pointer;
	}
</style>
