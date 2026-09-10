<!--
	The REPL — the day frame's large pane.

	A prompt line at the bottom, scrollback above it. Enter shell-splits the
	typed line the same way the native REPL does, sends it through
	`run(argv)`, and appends the echoed command plus whatever stdout/stderr
	came back. A command that draws rather than writes is asked for at the
	width of the scrollback it lands in — see `fitArgv`. The prompt line
	itself, Up and down included, is $lib/Prompt.svelte.

	`input` is bindable and `runArgv` is exported so the widget bar
	(da-nz29) lands in here rather than running silently: a press logs and
	its executed argv joins the scrollback and the recall history, so the
	next Up brings the command up ready to edit. The teach-loop is the
	whole point of the widgets sitting next to a command line.

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
	import Prompt from './Prompt.svelte';
	import { fitArgv } from './panes.js';
	import { run, type RunResult } from './run.js';
	import { shellSplit } from './shellSplit.js';

	/** The text in the prompt. Bind it to type a command in from outside. */
	export let input = '';
	export let disabled = false;

	const dispatch = createEventDispatcher<{ ran: RunResult }>();

	let pane: Pane;
	let body: HTMLDivElement | undefined;
	/** The scrollback's content width, in whole pixels, once measured. */
	let width: number | null = null;

	// The scrollback is restored before the first paint ($lib/replStore.ts);
	// pin the view to its end the way a run does.
	onMount(() => pane.scrollToBottom());

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

	async function submit({ detail: line }: CustomEvent<string>): Promise<void> {
		const ranAt = Date.now();
		// Split here rather than through `runLine`, because the argv is what
		// gets fitted to the box. The echo is the line as typed.
		await land(line, await run(fitArgv(shellSplit(line), width)), ranAt);
	}

	/**
	 * Run an argv from outside the prompt — a widget press (da-nz29).
	 *
	 * The scrollback echoes `result.argv`, the array the runner actually
	 * received, so a press teaches exactly the command it ran. A prettier
	 * string rebuilt from the press could drift from it; this cannot.
	 */
	export async function runArgv(argv: string[]): Promise<RunResult> {
		const ranAt = Date.now();
		const result = await run(argv);
		await land(result.argv.join(' '), result, ranAt);
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

	<svelte:fragment slot="footer">
		<Prompt bind:input {disabled} on:submit={submit} />
	</svelte:fragment>
</Pane>

<style>
	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}

	.quiet code {
		font-style: normal;
		color: #7c7cff;
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
		color: #8888aa;
	}

	/* Dimmer than the echoed command and much dimmer than the data under
	   it: this is when you typed, and the entry's own occur_time — which
	   the renderer prints two lines down — is what actually matters. */
	.stamp {
		flex: none;
		color: #444466;
		font-variant-numeric: tabular-nums;
	}

	/* One dim hairline between commands, and the air either side of it. */
	.rule {
		margin: 0.5rem 0;
		border: none;
		border-top: 1px solid #1e1e38;
	}

	.prompt {
		color: #7c7cff;
	}
</style>
