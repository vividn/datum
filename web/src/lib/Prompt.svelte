<!--
	The prompt line: a live clock, the caret, the text box. The bottom
	bar's and the input widget's are this one component (da-0dhc), so the
	two cannot drift apart in how they look or in what Up brings back.

	Enter hands the line to the owner as `submit` and empties the box, the
	way a shell consumes a line. Up and down walk the history every prompt
	shares ($lib/replStore.ts), shell-style: the draft you were typing is
	kept and handed back when you walk past the newest entry.

	The default slot sits at the end of the line, for what the owner has to
	say about the last command without an output pane to say it in.

	`ghost` is a command the reader did not type, shown for about a second
	where the placeholder goes and fading out of it (da-jprg). A span over
	the box rather than the placeholder itself because Firefox animates
	neither a transition nor a keyframe on `::placeholder` — measured
	2026-09-14 on chromium 1223, webkit 2336 and firefox 1538, where only
	the first two interpolate. It is never the input's value: the box is
	the reader's, and the moment they touch a key the ghost is gone.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { Ghost } from '$lib/bar.js';
	import { clockSecondsLabel, now } from '$lib/clock.js';
	import { repl } from '$lib/replStore.js';

	/** The text in the prompt. Bind it to type a command in from outside. */
	export let input = '';
	export let disabled = false;
	/** A command nobody typed, to show and fade. Bind it: a keystroke takes it away. */
	export let ghost: Ghost | null = null;

	const dispatch = createEventDispatcher<{ submit: string }>();

	/** Index into the history, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';

	function submit(): void {
		const line = input;
		if (!line.trim()) return;
		input = '';
		draft = '';
		cursor = -1;
		dispatch('submit', line);
	}

	function recall(event: KeyboardEvent): void {
		// The box is the reader's the moment they type in it.
		ghost = null;
		const { history } = $repl;
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

<form class="line" on:submit|preventDefault={submit}>
	<!-- Live, not restored: a command's stamp is history, this one is the
	     clock. On a narrow screen the seconds go and the minutes stay —
	     the input's width is not the clock's to take. -->
	<span class="stamp"
		>{clockSecondsLabel($now).slice(0, 5)}<span class="seconds"
			>{clockSecondsLabel($now).slice(5)}</span
		></span
	>
	<span class="prompt">❯</span>
	<span class="field">
		<input
			type="text"
			bind:value={input}
			on:keydown={recall}
			{disabled}
			placeholder={disabled ? 'opening database…' : 'occur coffee cups=2'}
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
		/>
		<!-- Keyed on the object, so a second press restarts the fade rather
		     than joining the first one's. Hidden while there is anything in
		     the box: the ghost has the placeholder's slot and nothing else. -->
		{#if ghost && !input}
			{#key ghost}
				<span class="ghost" aria-hidden="true">{ghost.line}</span>
			{/key}
		{/if}
	</span>
	<slot />
</form>

<style>
	.line {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		border-top: 1px solid var(--tint);
		padding: 0.45rem 0.6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}

	.stamp {
		flex: none;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}

	.prompt {
		color: var(--accent);
	}

	/* The box and the ghost over it, one on top of the other. */
	.field {
		position: relative;
		display: flex;
		flex: 1;
		min-width: 0;
	}

	input {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		padding: 0;
		color: var(--ink-bright);
		font: inherit;
		font-size: 0.95rem;
	}

	/* Where the placeholder is, in the placeholder's font — the same
	   position, no second row, no layout shift — and out of the way of the
	   caret and of a click. It arrives near the brightness of a typed line
	   (vividn: "a slightly dimmed version") and fades to nothing: arriving
	   dim and fading dimmer is a change too small to read. */
	.ghost {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		overflow: hidden;
		white-space: nowrap;
		pointer-events: none;
		color: var(--ink);
		font-size: 0.95rem;
		animation: ghost-fade 1s ease-out forwards;
	}

	@keyframes ghost-fade {
		from {
			opacity: 1;
		}
		to {
			opacity: 0;
		}
	}

	/* No fade for a reader who asked for no motion: it is shown, and the
	   owner's own timer takes it away at the same moment (PromptBar.svelte). */
	@media (prefers-reduced-motion: reduce) {
		.ghost {
			animation: none;
		}
	}

	@media (max-width: 480px) {
		.stamp .seconds {
			display: none;
		}
	}

	input::placeholder {
		color: var(--quiet);
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
