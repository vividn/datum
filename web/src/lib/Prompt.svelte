<!--
	The prompt line: the caret, the text box, and the pills that ride after
	what is typed in it. The bottom bar is its one user since the input
	widget went (da-22vg) — the REPL lost its own prompt in da-rfs9 — so
	this is indirection kept on purpose, not shared code.

	The clock left in da-xfa4: it is under the logo now, in the bar's left
	block, where it lines up with this row. One implementation still
	($lib/clock.ts) and now one caller, so there is no prop here deciding
	whether to draw it.

	Enter hands the line to the owner as `submit` and empties the box, the
	way a shell consumes a line. Up and down walk the history every prompt
	shares ($lib/replStore.ts), shell-style: the draft you were typing is
	kept and handed back when you walk past the newest entry.

	The default slot sits at the end of the line, for what the owner has to
	say about the last command without an output pane to say it in.

	THERE IS NO PLACEHOLDER while the database is open. vividn, 2026-09-14:
	"Let's get rid of the always present placeholder `occur coffee cups=2`,
	that doesn't add to the experience for new users and is annoying for
	non-new users." This supersedes da-e5u7, which chose that text after
	the one before it (`occur coffee 2 cups`) turned out not to parse: the
	worked examples the empty REPL offers are still there, run by
	`hints.test.mjs`, and they are where a first command comes from now.
	`opening database…` stays: that one says STATE, not example.

	`ghost` is a command the reader did not type, shown for about a second
	where the placeholder was and fading out of it (da-jprg). A span over
	the box rather than the placeholder itself because Firefox animates
	neither a transition nor a keyframe on `::placeholder` — measured
	2026-09-14 on chromium 1223, webkit 2336 and firefox 1538, where only
	the first two interpolate. It is never the input's value: the box is
	the reader's, and the moment they touch a key the ghost is gone.

	THE `pills` SLOT rides just after the typed text (da-xfa4). vividn: "move
	the -t pills to be just slightly to the right of the >, so that they
	are visible as you type text (they can move to the right as you type
	more text)". Following the text needs the text's width, and
	`field-sizing: content` is not everywhere it has to work — measured
	2026-09-14: chromium 1223 has it, webkit 2336 and firefox 1538 do not —
	so the width comes from a hidden MIRROR span in the box's own font,
	which every engine can measure. The mirror carries the ghost's line
	while a ghost is up, so the pills are part of the ghost and go back
	leftward when it fades.

	When text and pills outgrow the box the pills STOP following and stand
	at its right edge, and the box reserves their width as padding so the
	text scrolls under the caret rather than under the pills: what a pill
	says is state the next command carries, and it has to stay both legible
	and clickable however long the line is.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { Ghost } from '$lib/bar.js';
	import { repl } from '$lib/replStore.js';

	/** The text in the prompt. Bind it to type a command in from outside. */
	export let input = '';
	export let disabled = false;
	/** A command nobody typed, to show and fade. Bind it: a keystroke takes it away. */
	export let ghost: Ghost | null = null;

	const dispatch = createEventDispatcher<{ submit: string }>();

	/** The air between the `❯` and a pill, so the box stays easy to click into. */
	const GAP = 10;

	/** Index into the history, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';

	let boxWidth = 0;
	let textWidth = 0;
	let pillsWidth = 0;

	/** What the pills follow: what is typed, or the ghost while one is up. */
	$: mirrored = input || (ghost ? ghost.line : '');
	$: pillsLeft = Math.max(GAP, Math.min(textWidth + GAP, boxWidth - pillsWidth));

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
	<span class="prompt">❯</span>
	<span class="field" style="--pills: {pillsWidth}px" bind:clientWidth={boxWidth}>
		<input
			type="text"
			bind:value={input}
			on:keydown={recall}
			{disabled}
			placeholder={disabled ? 'opening database…' : ''}
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
		/>
		<!-- The box's own font, out of the flow and out of the way: only its
		     width is wanted. -->
		<span class="mirror" aria-hidden="true" bind:clientWidth={textWidth}>{mirrored}</span>
		<!-- Keyed on the object, so a second press restarts the fade rather
		     than joining the first one's. Hidden while there is anything in
		     the box: the ghost has the placeholder's slot and nothing else. -->
		{#if ghost && !input}
			{#key ghost}
				<span class="ghost" aria-hidden="true">{ghost.line}</span>
			{/key}
		{/if}
		<span class="pills" style="left: {pillsLeft}px" bind:clientWidth={pillsWidth}>
			<slot name="pills" />
		</span>
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

	.prompt {
		color: var(--accent);
	}

	/* The box, the ghost over it, the mirror behind it and the pills on
	   top, one on top of the other. */
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
		padding: 0 var(--pills) 0 0;
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
		inset: 0 var(--pills) 0 0;
		display: flex;
		align-items: center;
		overflow: hidden;
		white-space: nowrap;
		pointer-events: none;
		color: var(--ink);
		font-size: 0.95rem;
		animation: ghost-fade 1s ease-out forwards;
	}

	/* Measured, never seen: the same face, size and spacing as the box, so
	   its width is the text's width in the box. */
	.mirror {
		position: absolute;
		top: 0;
		left: 0;
		visibility: hidden;
		pointer-events: none;
		white-space: pre;
		font: inherit;
		font-size: 0.95rem;
	}

	/* On top of the box, following the text. `left` is animated so a pill
	   left behind by a ghost is SEEN going back to its waiting place rather
	   than teleporting (vividn: "then they can go to the left back towards
	   the start of the input box"); at 120ms it still reads as following
	   while someone types. */
	.pills {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		display: flex;
		align-items: center;
		gap: 0.3rem;
		transition: left 120ms ease-out;
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
		.pills {
			transition: none;
		}
	}

	input::placeholder {
		color: var(--quiet);
	}

	input:disabled {
		opacity: 0.5;
	}
</style>
