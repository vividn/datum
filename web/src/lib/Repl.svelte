<!--
	The REPL — the day frame's large pane.

	A prompt line at the bottom, scrollback above it. Enter sends the typed
	line through `run(argv)` (via `runLine`, which shell-splits it the same
	way the native REPL does) and appends the echoed command plus whatever
	stdout/stderr came back. Up and down walk the history, shell-style: the
	draft you were typing is kept and handed back when you walk past the
	newest entry.

	`input` is bindable and `runArgv` is exported so the widget bar
	(da-nz29) lands in here rather than running silently: a press logs and
	its executed argv joins the scrollback and the recall history, so the
	next Up brings the command up ready to edit. The teach-loop is the
	whole point of the widgets sitting next to a command line.
-->
<script lang="ts">
	import { createEventDispatcher, tick } from 'svelte';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { run, runLine, type RunResult } from './run.js';

	/** The text in the prompt. Bind it to type a command in from outside. */
	export let input = '';
	export let disabled = false;

	const dispatch = createEventDispatcher<{ ran: RunResult }>();

	let scrollback: { line: string; result: RunResult }[] = [];
	let history: string[] = [];
	/** Index into `history`, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';
	let pane: Pane;

	async function submit(): Promise<void> {
		const line = input;
		if (!line.trim()) return;

		// Consume the line the way a shell does — it lives in the
		// scrollback and in the history now, and Up brings it back.
		input = '';
		draft = '';
		cursor = -1;
		remember(line);
		await record(line, await runLine(line));
	}

	/**
	 * Run an argv from outside the prompt — a widget press (da-nz29).
	 *
	 * The scrollback echoes `result.argv`, the array the runner actually
	 * received, so a press teaches exactly the command it ran. A prettier
	 * string rebuilt from the press could drift from it; this cannot.
	 */
	export async function runArgv(argv: string[]): Promise<RunResult> {
		const result = await run(argv);
		const line = result.argv.join(' ');
		remember(line);
		await record(line, result);
		return result;
	}

	function remember(line: string): void {
		if (history[history.length - 1] !== line) history = [...history, line];
	}

	async function record(line: string, result: RunResult): Promise<void> {
		scrollback = [...scrollback, { line, result }];
		await tick();
		pane.scrollToBottom();
		dispatch('ran', result);
	}

	function recall(event: KeyboardEvent): void {
		if (event.key === 'ArrowUp') {
			const next = cursor === -1 ? history.length - 1 : cursor - 1;
			if (next < 0) return;
			if (cursor === -1) draft = input;
			event.preventDefault();
			cursor = next;
			input = history[cursor];
		} else if (event.key === 'ArrowDown') {
			if (cursor === -1) return;
			event.preventDefault();
			cursor += 1;
			if (cursor >= history.length) {
				cursor = -1;
				input = draft;
			} else {
				input = history[cursor];
			}
		}
	}
</script>

<Pane title="repl" bind:this={pane}>
	{#if scrollback.length === 0}
		<p class="quiet">type a command — <code>switch mood happy</code>, <code>tail</code></p>
	{/if}
	{#each scrollback as entry, i (i)}
		<div class="echo"><span class="prompt">❯</span> {entry.line}</div>
		{#if entry.result.stdout}
			<CliOutput text={entry.result.stdout} />
		{/if}
		{#if entry.result.stderr}
			<CliOutput text={entry.result.stderr} error={entry.result.code !== 0} />
		{/if}
	{/each}

	<form slot="footer" class="line" on:submit|preventDefault={submit}>
		<span class="prompt">❯</span>
		<input
			type="text"
			bind:value={input}
			on:keydown={recall}
			{disabled}
			placeholder={disabled ? 'opening database…' : 'occur coffee 2 cups'}
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
		/>
	</form>
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

	.echo {
		white-space: pre-wrap;
		color: #8888aa;
	}

	.prompt {
		color: #7c7cff;
	}

	.line {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		border-top: 1px solid #1e1e38;
		padding: 0.45rem 0.6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}

	input {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: #e0e0ff;
		font: inherit;
		font-size: 0.95rem;
	}

	input::placeholder {
		color: #444466;
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
