<!--
	One block of CLI output, rendered the way a terminal would.

	Monospace, whitespace preserved, and ANSI translated into spans rather
	than dumped as escape bytes (see $lib/ansi.ts). Lines are NOT wrapped:
	datum sizes its tables to a column count, and a pane that reflowed them
	on every resize would shuffle the columns under the reader. A block
	wider than its pane scrolls sideways inside it instead — and only then:
	the block is as wide as its longest line and no wider.

	`datum dayview` is the exception that proves the pane is rendering CLI
	output faithfully: its stdout is an SVG document, so it is shown as a
	picture. Inline, so its marks are elements a pointer, a finger and the
	keyboard can ask what they are (da-s5da, $lib/hover.ts) — and sanitised
	on the way in, so the runner's stdout does not become an injection
	point the day it carries synced data.
-->
<script lang="ts">
	import { ansiSpans } from './ansi.js';
	import AnsiText from './AnsiText.svelte';
	import { picture } from './hover.js';
	import { resizing } from './layout/resizing.js';

	export let text: string;
	/** Paint the block as an error (stderr). */
	export let error = false;
	/**
	 * Which stream this block is. `error` cannot say: since progress went
	 * to stderr (da-h9vj) a successful command renders two plain blocks,
	 * and a reader asking for "what it printed" got the progress lines
	 * (da-q15y).
	 */
	export let stream: 'stdout' | 'stderr' = 'stdout';
	/** A newer block has been asked for: this one is dimmed until it lands. */
	export let behind = false;

	const SVG_START = /^\s*(?:<\?xml[\s?]|<svg[\s>])/;

	$: svg = SVG_START.test(text);
	$: spans = svg ? [] : ansiSpans(text);
</script>

{#if svg}
	<!-- While a border is being dragged the picture is not re-asked for
	     (layout/resizing.ts); it follows its box as one scaled image
	     until the drop redraws it — whole, so an axis and its labels
	     move together (da-jgrn). -->
	<div class="picture" class:stretch={$resizing} class:behind use:picture={text} />
{:else}
	<pre data-stream={stream} class:error class:behind><AnsiText {spans} /></pre>
{/if}

<style>
	pre {
		margin: 0;
		font: inherit;
		/* `pre`, not `pre-wrap`: see the note above about reflow. A table
		   datum sized for 80 columns is 80 columns of text, so `pre` alone
		   already keeps its shape and the pane scrolls to it. There used
		   to be a `min-width: 80ch` here saying the same thing a second
		   time — which did nothing for the table it was written for and
		   held every SHORT block open to 674px instead, so the tail pane
		   and the REPL scrolled sideways over blank space at every
		   viewport, 1280 included (da-45wt). */
		white-space: pre;
	}

	.error {
		color: var(--error);
	}

	/* Catching up (da-eoq3): the old block stays, faded, so it is never read
	   as the answer to what was just asked. The new one lands at full. */
	.behind {
		opacity: 0.45;
		transition: opacity 120ms;
	}

	.picture :global(svg.svg) {
		display: block;
		/* Shrink to fit, never stretch. A drawing may be scaled down where
		   a table may not — scrolling sideways to a day loses the day —
		   but scaling shrinks the text with it, and at a pane's width that
		   is what made the dayview's axis illegible (da-m0cj). A pane that
		   tells datum its size (`wantsBox` in $lib/panes.ts) gets a band that
		   already fits, and this cap then does nothing; one that does not,
		   like nowview's 520px canvas, still shrinks to the box. */
		max-width: 100%;
		height: auto;
	}
	.picture.stretch :global(svg.svg) {
		width: 100%;
	}

	/* Gridlines and the now-line are drawn over the marks; a pointer
	   passes through them to the mark beneath. */
	.picture :global(line) {
		pointer-events: none;
	}

	/* Fixed, so a band that clips its picture does not clip the label. */
	.picture :global(.tip) {
		position: fixed;
		z-index: 10;
		max-width: calc(100vw - 8px);
		box-sizing: border-box;
		padding: 2px 6px;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 6px;
		color: var(--ink);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		pointer-events: none;
	}
</style>
