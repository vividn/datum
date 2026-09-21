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
	import { fileCommand, progress } from './dbio.js';
	import { echoArgv } from './echo.js';
	import { fitArgv } from './panes.js';
	import { run, type RunResult } from './run.js';

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
	let body: HTMLDivElement | undefined;
	/** The scrollback's content width, in whole pixels, once measured. */
	let width: number | null = null;

	$: ordered = reversed ? [...$repl.scrollback].reverse() : $repl.scrollback;
	// `skip` takes from the NEWEST end whichever way the list runs.
	$: entries = skip ? (reversed ? ordered.slice(skip) : ordered.slice(0, -skip)) : ordered;

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
		if (!body) return;
		const observer = new ResizeObserver(([entry]) => {
			width = Math.floor(entry.contentRect.width);
		});
		observer.observe(body);
		return () => observer.disconnect();
	});

	/**
	 * Run an argv — the bottom bar's line, a widget press (da-nz29).
	 *
	 * A typed `line` is echoed as typed, quotes and all. A press has no
	 * line: the scrollback echoes the argv the CALLER asked for, so a press
	 * teaches exactly the command it asked for. A prettier string rebuilt
	 * from the press could drift from it; this cannot. What it must NOT
	 * echo is the argv the runner received — the web appends flags of its
	 * own on the way through (`--width`, and `--show json` since da-xfa4),
	 * and those are the web's plumbing, not a command to teach.
	 *
	 * A file command ($lib/dbio.ts) goes the browser's way, before any
	 * await: its dialog needs the keypress — unless the caller brought
	 * `stdin` itself, which is a file already chosen (`engine.press`).
	 */
	export async function runArgv(argv: string[], line?: string, stdin?: string): Promise<RunResult> {
		const ranAt = Date.now();
		const result = await (
			(stdin === undefined ? fileCommand(argv) : null) ??
			run(echoArgv(fitArgv(argv, width)), stdin)
		);
		await land(line ?? argv.join(' '), result, ranAt);
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
	<!-- Keyed by the entry itself, not its position: newest-first puts every
	     new command at position 0, and a key by index rebuilt the whole
	     scrollback on each one (review on #33). -->
	{#each entries as entry, i (entry)}
		<!-- A rule stands on a SEAM — between this row and one above it —
		     and `i > 0` asked a narrower question: "is this my first row".
		     Those were the same question while the repl was the whole list.
		     They stopped being it when the bar's output section became row
		     one and this view began at `skip` (da-qdm7, vividn: "there is a
		     missing line between the first entry in the repl and the
		     second"): the seam above the log's first row is the one edge
		     neither renderer owns, and it went undrawn. `skip` is what says
		     there IS a row above that this view does not draw — and it is 0
		     in a pane, where the first row is the first row and a rule over
		     it would be a rule against the pane's own body. -->
		{#if i > 0 || skip}
			<hr class="rule" />
		{/if}
		<!-- The row itself is `ReplEntry.svelte`, which the bar's output
		     section mounts too: one list, one shape for a row of it. -->
		<ReplEntry {entry} all />
	{/each}
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

	/* One dim hairline between commands, and the air either side of it. */
	.rule {
		margin: 0.5rem 0;
		border: none;
		border-top: 1px solid var(--tint);
	}

	.progress {
		margin: 0;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}
</style>
