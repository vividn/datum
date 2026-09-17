<!--
	THE command line: the caret, the box, the pills that ride after what is
	typed in it, and the Enter that runs it.

	It is drawn in two places and it is ONE line (da-3zon): the bottom bar
	(PromptBar.svelte) and every `input` widget on the page
	(layout/InputWidget.svelte). The box binds straight to `text` in
	$lib/bar.ts and Enter calls `submitLine` there, so nothing here is a
	copy of anything — type in either and both show it, Enter in either
	runs it, and both empty together. Two local strings synced by effects is
	how two boxes drift by a keystroke.

	The clock left in da-xfa4: it is under the logo now, in the bar's left
	block, where it lines up with this row.

	Enter empties the box the way a shell consumes a line. Up and down walk
	the history every prompt shares ($lib/replStore.ts), shell-style: the
	draft you were typing is kept and handed back when you walk past the
	newest entry — kept per BOX, since it is what this reader was in the
	middle of typing here.

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

	THE PILLS ride just after the typed text (da-xfa4). vividn: "move the -t
	pills to be just slightly to the right of the >, so that they are
	visible as you type text (they can move to the right as you type more
	text)". Following the text needs the text's width, and `field-sizing:
	content` is not everywhere it has to work — measured 2026-09-14:
	chromium 1223 has it, webkit 2336 and firefox 1538 do not — so the
	width comes from a hidden MIRROR span in the box's own font, which
	every engine can measure. The mirror carries the ghost's line while a
	ghost is up, so the pills are part of the ghost and go back leftward
	when it fades. Each instance measures its OWN text, so a widget's pills
	sit past what was typed in the widget.

	When text and pills outgrow the box the pills STOP following and stand
	at its right edge, and the box reserves their width as padding so the
	text scrolls under the caret rather than under the pills: what a pill
	says is state the next command carries, and it has to stay both legible
	and clickable however long the line is.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { ghost, pills, registerLine, spent, submitLine, text } from '$lib/bar.js';
	import Pill from '$lib/Pill.svelte';
	import { ready } from '$lib/layout/engine.js';
	import { repl } from '$lib/replStore.js';

	/**
	 * True in an input widget. It is where a focus that nobody clicked
	 * goes while one is on screen (`focusLine` in $lib/bar.ts).
	 */
	export let widget = false;

	/** The air between the `❯` and a pill, so the box stays easy to click into. */
	const GAP = 10;

	/** Index into the history, or -1 while editing a fresh line. */
	let cursor = -1;
	let draft = '';

	let box: HTMLInputElement;
	let boxWidth = 0;
	let textWidth = 0;
	let pillsWidth = 0;

	onMount(() => registerLine(box, widget));

	/**
	 * The keyboard back (da-k34p). A command button focuses the box with
	 * `inputmode="none"` (`focusLine`); a `none` left behind is a prompt
	 * nobody can type in on a phone. So it goes on pointerdown — before the
	 * browser handles the tap, so the keyboard opens on THAT tap — on blur,
	 * and whenever the line is empty again.
	 */
	const keyboard = () => box?.removeAttribute('inputmode');
	$: if (!$text) keyboard();

	/** What the pills follow: what is typed, or the ghost while one is up. */
	$: mirrored = $text || ($ghost ? $ghost.line : '');
	$: pillsLeft = Math.max(GAP, Math.min(textWidth + GAP, boxWidth - pillsWidth));

	function submit(): void {
		const line = $text;
		if (!line.trim()) return;
		$text = '';
		draft = '';
		cursor = -1;
		void submitLine(line);
	}

	function recall(event: KeyboardEvent): void {
		// The box is the reader's the moment they type in it.
		if ($ghost) ghost.set(null);
		const { history } = $repl;
		if (event.key === 'ArrowUp') {
			const next = cursor === -1 ? history.length - 1 : cursor - 1;
			if (next < 0) return;
			if (cursor === -1) draft = $text;
			event.preventDefault();
			cursor = next;
			$text = history[cursor];
		} else if (event.key === 'ArrowDown') {
			if (cursor === -1) return;
			event.preventDefault();
			cursor += 1;
			if (cursor >= history.length) {
				cursor = -1;
				$text = draft;
			} else {
				$text = history[cursor];
			}
		}
	}
</script>

<form class="line" on:submit|preventDefault={submit}>
	<span class="prompt">❯</span>
	<span class="field" style="--pills: {pillsWidth}px" bind:clientWidth={boxWidth}>
		<input
			type="text"
			bind:this={box}
			bind:value={$text}
			on:keydown={recall}
			on:pointerdown={keyboard}
			on:blur={keyboard}
			disabled={!$ready}
			placeholder={$ready ? '' : 'opening database…'}
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
		{#if $ghost && !$text}
			{#key $ghost}
				<span class="ghost" aria-hidden="true">{$ghost.line}</span>
			{/key}
		{/if}
		<span class="pills" style="left: {pillsLeft}px" bind:clientWidth={pillsWidth}>
			<!-- One pill is `Pill.svelte`, which the nowview's margin draws
			     too (da-rskp): one store, two places to see it, and the same
			     press in either. -->
			{#each $pills as pill (pill.id)}
				<Pill {pill} />
			{/each}
			<!-- Spent, not live: no × and no lock, out of the argv and out of
			     the time cursor, fading with the ghost of the command it went
			     into. A different class for a different thing, so a reader of
			     `.pill` — a test included — is never handed one. -->
			{#each $spent as pill (pill.id)}
				<span class="spent {pill.kind}">{pill.text}</span>
			{/each}
		</span>
	</span>
</form>

<style>
	.line {
		/* What is typed, and so what a pill beside it reads at — vividn asked
		   for the two to be one size (da-tnv3), so they are one number. */
		--input-font: 1.15rem;
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
		font-size: var(--input-font);
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
		font-size: var(--input-font);
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
		font-size: var(--input-font);
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

	/* SOLID, and the size of what is typed beside them. vividn, on the
	   deployed build: "We need to make the pills much more prominent, they
	   are almost invisible right now. Solid background color… increase
	   their font size to be the same as the input bar." So the kind's
	   colour — the colour the outline used to be — FILLS the pill and the
	   page's own ground is the ink on it (`--page`'s contract), which is
	   the one pairing the palette already names.

	   `--pill` is that colour, and the only thing a kind sets: the fill,
	   the ink and the locked ring are all derived from it here, so a new
	   kind stays one line. */
	.spent {
		--pill: var(--accent);
		display: inline-flex;
		align-items: center;
		border: none;
		border-radius: 999px;
		color: var(--page);
		background: var(--pill);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: var(--input-font);
		line-height: 1;
		white-space: nowrap;
	}

	.spent {
		padding: 0.25rem 0.55rem;
		pointer-events: none;
		opacity: 0.6;
		animation: spent-fade 1s ease-out forwards;
	}

	@keyframes ghost-fade {
		from {
			opacity: 1;
		}
		to {
			opacity: 0;
		}
	}

	/* From the spent pill's own opacity, not from 1: it is a receipt
	   fading, and it never brightens on its way out. */
	@keyframes spent-fade {
		from {
			opacity: 0.6;
		}
		to {
			opacity: 0;
		}
	}

	/* No fade for a reader who asked for no motion: it is shown, and the
	   store's own timer takes it away at the same moment ($lib/bar.ts). */
	@media (prefers-reduced-motion: reduce) {
		.ghost,
		.spent {
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
