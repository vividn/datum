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
-->
<script lang="ts">
	import { now, stampLabel } from '$lib/clock.js';
	import { repl, record } from '$lib/replStore.js';
	import { isDegraded, storage } from '$lib/storageState.js';
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { fileCommand, progress } from './dbio.js';
	import Docs from './Docs.svelte';
	import { echoArgv, parseEcho } from './echo.js';
	import { fitArgv } from './panes.js';
	import { run, type RunResult } from './run.js';

	const dispatch = createEventDispatcher<{ ran: RunResult }>();

	let pane: Pane;
	let body: HTMLDivElement | undefined;
	/** The scrollback's content width, in whole pixels, once measured. */
	let width: number | null = null;

	// The scrollback is restored before the first paint ($lib/replStore.ts);
	// pin the view to its end the way a run does.
	onMount(() => pane.scrollToBottom());
	// A progress line lands under the scrollback; keep it in view.
	$: if ($progress && pane) tick().then(() => pane.scrollToBottom());

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

	/** A command's documents, when it echoed any ($lib/echo.ts). */
	const docsOf = (result: RunResult) => parseEcho(result.stdout);

	/** Into the shared store, and the view pinned to it. */
	async function land(line: string, result: RunResult, ranAt: number): Promise<void> {
		record(line, ranAt, result);
		await tick();
		pane.scrollToBottom();
		dispatch('ran', result);
	}
</script>

<Pane title="repl" alarm={isDegraded($storage)} bind:this={pane} bind:body>
	{#if $repl.scrollback.length === 0}
		<p class="quiet">type a command — <code>switch mood happy</code>, <code>tail</code></p>
	{/if}
	{#each $repl.scrollback as entry, i (i)}
		{#if i > 0}
			<hr class="rule" />
		{/if}
		<div class="head">
			<span class="stamp">{stampLabel(new Date(entry.ranAt), $now)}</span>
			<!-- The stamp is a sibling of `.echo`, not a child of it: the
			     echo's text is the command and nothing else, which is what
			     every reader of it — a test, a copy-paste — already assumes. -->
			<div class="echo"><span class="prompt">❯</span> {entry.line}</div>
		</div>
		{#if docsOf(entry.result)}
			<!-- The same sections the bottom bar draws, one per document
			     (da-xfa4): the web appends `--show json` to every command it
			     runs, so both surfaces get the documents and a multi-document
			     result is side by side in each.
			     THE TRADE: `--show json` carries the CLI's strings with its
			     ANSI stripped, so an action echo in here is no longer painted
			     in datum's field and state colours — a table, a picture and an
			     error still are, because they carry no echo and arrive as the
			     CLI printed them. vividn asked for exactly this ("The whole web
			     interface can take use json as the default show and then
			     render on it's own terms which is nice separation"); the
			     colour is the cost of the one mechanism. -->
			<Docs docs={docsOf(entry.result) ?? []} all />
		{:else if entry.result.stdout}
			<CliOutput text={entry.result.stdout} />
		{/if}
		{#if entry.result.stderr}
			<CliOutput text={entry.result.stderr} stream="stderr" error={entry.result.code !== 0} />
		{/if}
	{/each}
	{#if $progress}
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

	.head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.echo {
		flex: 1;
		min-width: 0;
		white-space: pre-wrap;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   bottom bar's receipt uses (PromptBar.svelte). */
		color: #8888aa;
	}

	/* Dimmer than the echoed command and much dimmer than the data under
	   it: this is when you typed, and the entry's own occur_time — which
	   the renderer prints two lines down — is what actually matters. */
	.stamp {
		flex: none;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}

	/* One dim hairline between commands, and the air either side of it. */
	.rule {
		margin: 0.5rem 0;
		border: none;
		border-top: 1px solid var(--tint);
	}

	.prompt {
		color: var(--accent);
	}

	.progress {
		margin: 0;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}
</style>
