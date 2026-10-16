<!--
	The REPL — the day frame's large pane: the scrollback, and nothing to
	type into. The bottom bar's prompt is the page's command line
	($lib/PromptBar.svelte, da-rfs9: "Remove the input box at the bottom of
	the REPL widget it is redundant now").

	`runArgv` is exported so the bar's line and a widget-bar press (da-nz29)
	land in here rather than running silently: the command runs through
	`run(argv)`, and the echoed command plus whatever stdout/stderr came
	back joins the scrollback and the recall history, so the next Up in the
	bar brings it up ready to edit. The teach-loop is the whole point of
	the widgets sitting next to a command line. A command that draws rather
	than writes is asked for at the width of the scrollback it lands in —
	see `fitArgv`.

	Every command carries the instant it was run (da-9ozz), stamped here at
	submit rather than carried back in `RunResult` — the seam is `run(argv)`
	and the CLI's result has no such field. The stamp is dim on purpose: a
	row's own `occur_time`, which the renderer already prints, is the fact
	that matters; when the command was typed is context. The prompt's own
	clock is the same label, live.

	The scrollback and the history are not this component's: they are the
	one store every prompt shares ($lib/replStore.ts, da-0dhc), which is
	also what keeps them across a reload. A command typed into the input
	widget is in this scrollback too, output and all.

	`reversed` (da-gvdc, the repl widget's option) draws the entries newest
	first — each entry keeps its own order, the command and then its answer
	— and pins the view to the top wherever it pins it to the bottom
	otherwise: on mount, on every run, and under a progress line, which is
	drawn first with them.
-->
<script lang="ts">
	import { now, stampLabel } from '$lib/clock.js';
	import { repl, record } from '$lib/replStore.js';
	import { isDegraded, storage } from '$lib/storageState.js';
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import Pane from './Pane.svelte';
	import ReplEntry from './ReplEntry.svelte';
	import { asking, fileCommand, progress } from './dbio.js';
	import { parseEcho, runEchoed } from './echo.js';
	import { charactersIn, fitArgv, helpEnv } from './panes.js';
	import { GAP, columnsIn, rowsOf } from './scrollback.js';
	import { run, type RunResult } from './run.js';
	import { shellJoin } from './shellSplit.js';
	import { syncCommand } from './sync.js';

	/** Newest entry first, the view pinned to the top. */
	export let reversed = false;

	/**
	 * Draw the body alone — no title strip, no outline (Pane's `bare`).
	 *
	 * The bar's log is the caller. vividn, 2026-09-21: "Remove the 'repl'
	 * header line."
	 */
	export let bare = false;

	/**
	 * How many of the newest entries this view does NOT draw.
	 *
	 * The bar's log passes 1, because the bar's own output section is row
	 * one and dragging the log up must not meet it again (vividn, 2026-09-21:
	 * "it shouldn't be repeated when you drag it up"). One list, and the two
	 * surfaces show different slices of it.
	 */
	export let skip = 0;

	const dispatch = createEventDispatcher<{ ran: RunResult }>();

	let pane: Pane;
	/**
	 * The scrolling box, bindable.
	 *
	 * The bar's log is the caller (da-tj6m): down there the ONE NUMBER owns
	 * this box's vertical scroll, so it has to be able to reach it.
	 */
	export let body: HTMLDivElement | undefined = undefined;
	/** The scrollback's content width, in whole pixels, once measured. */
	let width: number | null = null;
	/** Resolved by the first measurement, which a command run before it waits for (da-2flub). */
	let measured: () => void;
	const measuring = new Promise<void>((done) => (measured = done));

	$: ordered = reversed ? [...$repl.scrollback].reverse() : $repl.scrollback;
	// `skip` takes from the NEWEST end whichever way the list runs.
	$: entries = skip ? (reversed ? ordered.slice(skip) : ordered.slice(0, -skip)) : ordered;

	// AS MANY ENTRIES SIDE BY SIDE AS THE BOX HOLDS (da-5iyo, $lib/scrollback.ts).
	// The same `width` a drawn command is asked for is what says how many:
	// it is the body's CONTENT box, which is the room the columns have.
	$: columns = columnsIn(width);
	// An entry whose command answered with its own printed text rather than
	// an action echo — a table, a picture, an error — takes the whole row.
	$: wide = entries.map((entry) => !parseEcho(entry.result.stdout));
	$: rows = rowsOf(wide, columns);
	// The hairline stands on a SEAM (da-qdm7), so it is drawn above every
	// row but the first — and above that one too when this view begins at
	// `skip`, because then there is a row above that it does not draw.
	$: ruled = rows.map((row) => row > 0 || skip > 0);

	/** The view to the newest entry: the bottom, or the top when reversed. */
	function pin(): void {
		if (reversed && body) body.scrollTop = 0;
		else pane?.scrollToBottom();
	}

	// The scrollback is restored before the first paint ($lib/replStore.ts);
	// pin the view to its newest entry the way a run does, once the pane is
	// mounted — and again when the order flips.
	$: reversed, pane && tick().then(pin);
	// A progress line lands beside the newest entry; keep it in view.
	$: if ($progress && pane) tick().then(pin);

	// What a drawn command has to be told (`fitArgv`): the content box, not
	// the element's own width — the body is padded, and an image a fraction
	// wider than the box it lands in is an image the browser scales back
	// down, which is the bug (da-wexl). Only new output is drawn to the new
	// size; scrollback keeps the size it was drawn at, the way a terminal's
	// does.
	onMount(() => {
		if (!body) return measured();
		const observer = new ResizeObserver(([entry]) => {
			width = Math.floor(entry.contentRect.width);
			measured();
		});
		observer.observe(body);
		// Unmounted before it reported — the boot's real tree replacing the
		// one drawn early — a command waiting on it runs unmeasured, not never.
		return () => (observer.disconnect(), measured());
	});

	/**
	 * How many characters fit across the scrollback, in the font its output
	 * is drawn in: a help page is asked for at that width (da-nt53d), and
	 * takes the whole row, having no echo.
	 */
	const characters = () => (body && width ? charactersIn(body, width) : null);

	/**
	 * Run an argv — the bottom bar's line, a widget press (da-nz29).
	 *
	 * A typed `line` is echoed as typed, quotes and all. A press has no
	 * line: the scrollback echoes the argv the CALLER asked for, so a press
	 * teaches exactly the command it asked for. A prettier string rebuilt
	 * from the press could drift from it; this cannot. What it must NOT
	 * echo is the argv the runner received — the web appends flags of its
	 * own on the way through (`--width`; `--show json` goes in the env),
	 * and those are the web's plumbing, not a command to teach.
	 *
	 * A file command ($lib/dbio.ts) goes the browser's way, before any
	 * await: its dialog needs the keypress — unless the caller brought
	 * `stdin` itself, which is a file already chosen (`engine.press`). So
	 * does `sync chain connect`, to the sync worker ($lib/sync.ts).
	 */
	export async function runArgv(argv: string[], line?: string, stdin?: string): Promise<RunResult> {
		const ranAt = Date.now();
		const result = await (
			(stdin === undefined ? (fileCommand(argv) ?? syncCommand(argv)) : null) ??
			measuring.then(() => runEchoed(asking(run), fitArgv(argv, width), stdin, helpEnv(argv, characters())))
		);
		await land(line ?? shellJoin(argv), result, ranAt);
		return result;
	}

	/** Into the shared store, and the view pinned to it. */
	async function land(line: string, result: RunResult, ranAt: number): Promise<void> {
		record(line, ranAt, result);
		await tick();
		pin();
		dispatch('ran', result);
	}
</script>

<Pane title="repl" {bare} alarm={isDegraded($storage)} bind:this={pane} bind:body>
	{#if entries.length === 0}
		<p class="quiet">type a command — <code>switch mood happy</code>, <code>tail</code></p>
	{/if}
	{#if $progress && reversed}
		<p class="progress">{$progress}</p>
	{/if}
	<!-- THE SCROLLBACK IS COLUMNS, not one column of a phone's width
	     (da-5iyo). vividn: "the output of a single entry should be narrow
	     enough to comfortably fit 2 on mobile side by side." How many is
	     measured — `columnsIn` over the body's own box — and the hairline
	     that used to be `<hr>` between two entries is now the top edge of
	     every cell on a row past the first, which is the same seam drawn
	     where a grid can draw it.

	     Keyed by the entry itself, not its position: newest-first puts every
	     new command at position 0, and a key by index rebuilt the whole
	     scrollback on each one (review on #33). -->
	<div class="entries" data-columns={columns} style:--columns={columns} style:--gap="{GAP}px">
		{#each entries as entry, i (entry)}
			<div class="entry" class:ruled={ruled[i]} class:whole={wide[i]}>
				<!-- The row itself is `ReplEntry.svelte`, which the bar's
				     output section mounts too: one list, one shape for a row
				     of it. -->
				<ReplEntry {entry} all />
			</div>
		{/each}
	</div>
	{#if $progress && !reversed}
		<!-- A file command still running says how far it is (da-h9vj). -->
		<p class="progress">{$progress}</p>
	{/if}
</Pane>

<style>
	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}

	.quiet code {
		font-style: normal;
		color: var(--accent);
	}

	.entries {
		display: grid;
		grid-template-columns: repeat(var(--columns, 1), minmax(0, 1fr));
		column-gap: var(--gap);
		/* Entries are not the same height, and a short one stretched to its
		   neighbour's height would put its hairline nowhere near its text. */
		align-items: start;
	}

	/* An entry the columns cannot carry — a table, a drawn day, an error —
	   takes the whole row, at the width the CLI printed it for. */
	.entry.whole {
		grid-column: 1 / -1;
	}

	/* The same dim hairline `<hr class="rule">` drew, and the same air
	   either side of it: a top edge, so every cell on a row starts it at
	   the same y and the row reads as one rule with the gap in it. */
	.entry.ruled {
		margin-top: 0.5rem;
		padding-top: 0.5rem;
		border-top: 1px solid var(--tint);
	}

	.progress {
		margin: 0;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}
</style>
