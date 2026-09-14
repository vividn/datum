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
	 * line: the scrollback echoes `result.argv`, the array the runner
	 * actually received, so a press teaches exactly the command it ran. A
	 * prettier string rebuilt from the press could drift from it; this
	 * cannot. A file command ($lib/dbio.ts) goes the browser's way, before
	 * any await: its dialog needs the keypress — unless the caller brought
	 * `stdin` itself, which is a file already chosen (`engine.press`).
	 */
	export async function runArgv(argv: string[], line?: string, stdin?: string): Promise<RunResult> {
		const ranAt = Date.now();
		const result = await (
			(stdin === undefined ? fileCommand(argv) : null) ?? run(fitArgv(argv, width), stdin)
		);
		await land(line ?? result.argv.join(' '), result, ranAt);
		return result;
	}

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
		{#if entry.result.stdout}
			<CliOutput text={entry.result.stdout} />
		{/if}
		{#if entry.result.stderr}
			<CliOutput text={entry.result.stderr} error={entry.result.code !== 0} />
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
		/* Not a token: the same step between `--quiet` and `--ink` the input
		   widget's echo uses (Input.svelte). */
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
