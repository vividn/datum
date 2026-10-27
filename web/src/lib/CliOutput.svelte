<!--
	One block of CLI output, rendered the way a terminal would.

	Monospace, whitespace preserved, and ANSI translated into spans rather
	than dumped as escape bytes (see $lib/ansi.ts). Lines are NOT wrapped:
	datum sizes its tables to a column count, and a pane that reflowed them
	on every resize would shuffle the columns under the reader. A block
	wider than its pane scrolls sideways inside it instead — and only then:
	the block is as wide as its longest line and no wider.

	A field pill on the bar outlines that field's own marks in whatever
	picture is on screen (da-8810n): the group datum named is marked
	`data-pilled` and the halo below is the whole of it. A SELECTED entry
	(da-pzzoz) is marked `data-selected` on the mark that names it and
	wears the same halo, wherever the selection was made — a row in the
	tail, a document in the REPL, a double tap on the mark itself.

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

	interface Props {
		text: string;
		/** Paint the block as an error (stderr). */
		error?: boolean;
		/**
		 * Which stream this block is. `error` cannot say: since progress went
		 * to stderr (da-h9vj) a successful command renders two plain blocks,
		 * and a reader asking for "what it printed" got the progress lines
		 * (da-q15y).
		 */
		stream?: 'stdout' | 'stderr';
		/** A newer block has been asked for: this one is dimmed until it lands. */
		behind?: boolean;
		/** A picture's stretch from then to now is dimmed: this copy may not have it all yet (da-g5jjp). */
		unsynced?: number | undefined;
	}

	let {
		text,
		error = false,
		stream = 'stdout',
		behind = false,
		unsynced = undefined
	}: Props = $props();

	const SVG_START = /^\s*(?:<\?xml[\s?]|<svg[\s>])/;

	const svg = $derived(SVG_START.test(text));
	const spans = $derived(svg ? [] : ansiSpans(text));
</script>

{#if svg}
	<!-- While a border is being dragged the picture is not re-asked for
	     (layout/resizing.ts); it follows its box as one scaled image
	     until the drop redraws it — whole, so an axis and its labels
	     move together (da-jgrn). -->
	<div class="picture" class:stretch={$resizing} class:behind use:picture={{ text, unsynced }}></div>
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

	/* A clip's id is its geometry (`flow_clip` in datum-layout), so two
	   pictures of one size share it, and `url(#…)` finds the first on the
	   page: maybe one in a tab behind, which is `visibility: hidden`. A
	   clip's shape that is not visible clips everything away, so the tab in
	   front drew no history at all (da-wvdz2). A pattern (every dayview's
	   `#dead-zone` hatch), a mask and a marker are the same: their content
	   is styled where it sits, so the front picture's hatch went unpainted
	   (da-9lzxv). None is ever painted in place, so each is always visible. */
	.picture :global(:is(clipPath, pattern, mask, marker) *) {
		visibility: visible;
	}

	/* Gridlines and the now-line are drawn over the marks; a pointer
	   passes through them to the mark beneath. */
	.picture :global(line) {
		pointer-events: none;
	}

	/* A FIELD PILL on the bar brings that field's marks out (da-8810n,
	   `highlight` in $lib/hover.ts). vividn: "maybe a highlight outline, so
	   that all the occurences or data of that field is brought out a little
	   bit. nothing too intense, it must still be possible to see all the
	   other data well."

	   A halo, so it works on every shape the CLI draws — a 2px dot, a block
	   half a pixel wide, the now strip's chip — without the browser inventing
	   any geometry of its own. Two passes of one shadow rather than one wide
	   one: a single 2px blur around a 1px dot is a smudge, while two 1px
	   passes read as an edge. Nothing is done to the other fields at all:
	   whatever is not pilled is drawn exactly as datum drew it. */
	/* A SELECTED ENTRY wears the very same halo (da-pzzoz). vividn: "the
	   field outline will highlight all data in that field, the hid will
	   highlight just one block, I suppose they should look the same" — so
	   one rule says both, and a reader who has learned what the glow means
	   for a field has learned it for an entry. The difference is only what
	   it is ON: a field's whole group, or the one mark that names the
	   entry (`bringOut` in $lib/hover.ts). */
	.picture :global(.field[data-pilled]),
	.picture :global([data-selected]) {
		filter: drop-shadow(0 0 1px var(--ink-bright)) drop-shadow(0 0 1px var(--ink-bright));
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
