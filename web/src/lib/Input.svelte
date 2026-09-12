<!--
	The input widget (dashboard D1, da-0dhc): the REPL's prompt line with no
	output pane — a box to type a command into and, when the widget is
	taller than that line, the commands already run and when, newest at the
	bottom.

	What a command printed is not shown here, only recorded: it goes into
	the one store every prompt shares ($lib/replStore.ts), so it is in the
	REPL's scrollback wherever the REPL is, and in the history Up walks in
	either prompt.

	A command that fails still has to say so, because the person who typed
	it is looking at this box and nowhere else: the line flashes and the
	CLI's first line of stderr sits in it for a few seconds.
-->
<script lang="ts">
	import { createEventDispatcher, onDestroy, tick } from 'svelte';
	import { stripAnsi } from '$lib/ansi.js';
	import { now, stampLabel } from '$lib/clock.js';
	import Prompt from '$lib/Prompt.svelte';
	import { repl, record } from '$lib/replStore.js';
	import { fileCommand } from '$lib/dbio.js';
	import { run, type RunResult } from '$lib/run.js';
	import { shellSplit } from '$lib/shellSplit.js';
	import { isDegraded, storage } from '$lib/storageState.js';

	export let disabled = false;

	const dispatch = createEventDispatcher<{ ran: RunResult }>();

	/** How long a failure's message stays in the line. */
	const ERROR_MS = 5000;

	let list: HTMLOListElement | undefined;
	let error = '';
	let fade: ReturnType<typeof setTimeout> | undefined;
	// Navigating away mid-fade: the timer must not outlive the component
	// (review thread on #166).
	onDestroy(() => clearTimeout(fade));

	// Pinned to the newest command whenever one lands — and only then, not
	// on every tick of the clock, which would pull a reader scrolled back
	// through the history down again every second.
	$: pin($repl.scrollback.length);

	async function pin(_: number): Promise<void> {
		await tick();
		if (list) list.scrollTop = list.scrollHeight;
	}

	async function submit({ detail: line }: CustomEvent<string>): Promise<void> {
		const ranAt = Date.now();
		clearTimeout(fade);
		error = '';
		// A file command ($lib/dbio.ts) before any await: its dialog needs this keypress.
		const argv = shellSplit(line);
		const result = await (fileCommand(argv) ?? run(argv));
		record(line, ranAt, result);
		if (result.code !== 0) {
			error =
				stripAnsi(result.stderr)
					.split('\n')
					.find((l) => l.trim()) ?? `exit ${result.code}`;
			fade = setTimeout(() => (error = ''), ERROR_MS);
		}
		dispatch('ran', result);
	}
</script>

<section class="input-widget" class:alarm={isDegraded($storage)}>
	<ol class="history" bind:this={list}>
		{#each $repl.scrollback as entry, i (i)}
			<li>
				<span class="stamp">{stampLabel(new Date(entry.ranAt), $now)}</span>
				<span class="echo"><span class="caret">❯</span> {entry.line}</span>
			</li>
		{/each}
	</ol>
	<div class="entry" class:failed={error}>
		<Prompt {disabled} on:submit={submit}>
			{#if error}<span class="error" role="alert">{error}</span>{/if}
		</Prompt>
	</div>
</section>

<style>
	.input-widget {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 6px;
	}

	/* The REPL pane's alarm: someone typing into a database held in memory
	   is someone about to lose what they type. */
	.input-widget.alarm {
		border-color: var(--alarm);
		box-shadow: 0 0 0 1px var(--alarm) inset;
	}

	/* No vertical padding, so a widget one line tall gives the history
	   nothing at all rather than a sliver. */
	.history {
		flex: 1;
		min-height: 0;
		overflow: auto;
		margin: 0;
		padding: 0 0.6rem;
		list-style: none;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
		line-height: 1.35;
	}

	li {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		padding: 0.15rem 0;
	}

	/* The REPL's stamp and echo, dim for the same reason. */
	.stamp {
		flex: none;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}

	.echo {
		flex: 1;
		min-width: 0;
		white-space: pre-wrap;
		/* Not a token: a step between `--quiet` and `--ink` that three
		   places want and no role names — an echoed line here, in the REPL
		   and in the bottom bar's receipt. Give it a name when there is a
		   fourth, and a sentence to say what the three have in common. */
		color: #8888aa;
	}

	.caret {
		color: var(--accent);
	}

	.entry {
		flex: none;
	}

	.failed {
		animation: flash 0.8s ease-out;
	}

	/* Not a token: the ground a failed line flashes from, which is
	   `--error` taken most of the way down to `--ground` and is a colour
	   nothing else uses. */
	@keyframes flash {
		from {
			background: #4a1616;
		}
	}

	.error {
		flex: 0 1 auto;
		max-width: 50%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--error);
		font-size: 0.85rem;
	}
</style>
