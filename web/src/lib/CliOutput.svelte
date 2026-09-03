<!--
	One block of CLI output, rendered the way a terminal would.

	Monospace, whitespace preserved, and ANSI translated into spans rather
	than dumped as escape bytes (see $lib/ansi.ts). Lines are NOT wrapped:
	datum sizes its tables to a column count, and a pane that reflowed them
	on every resize would shuffle the columns under the reader. The block
	scrolls sideways inside its pane instead.

	`datum dayview` is the exception that proves the pane is rendering CLI
	output faithfully: its stdout is an SVG document, so it is shown as an
	image. Via a data-URL `<img>`, not inline markup — an image-loaded SVG
	cannot run script, which keeps the runner's stdout from becoming an
	injection point the day it carries synced data.
-->
<script lang="ts">
	import { ansiSpans } from './ansi.js';
	import { PANE_COLS } from './panes.js';

	export let text: string;
	/** Paint the block as an error (stderr). */
	export let error = false;

	const SVG_START = /^\s*(?:<\?xml[\s?]|<svg[\s>])/;

	$: svg = SVG_START.test(text) ? 'data:image/svg+xml,' + encodeURIComponent(text) : null;
	$: spans = svg ? [] : ansiSpans(text);
</script>

{#if svg}
	<img class="svg" src={svg} alt="dayview" />
{:else}
	<pre class:error style:min-width="{PANE_COLS}ch">{#each spans as span}<span
				style:color={span.fg}
				style:background={span.bg}
				style:font-weight={span.bold ? 'bold' : null}
				style:opacity={span.dim ? '0.6' : null}
				style:font-style={span.italic ? 'italic' : null}
				style:text-decoration={span.underline ? 'underline' : null}>{span.text}</span
			>{/each}</pre>
{/if}

<style>
	pre {
		margin: 0;
		font: inherit;
		/* `pre`, not `pre-wrap`: see the note above about reflow. The
		   min-width is the claim itself — the block is always PANE_COLS
		   wide, so a table datum sized for 80 columns keeps its shape and
		   the pane scrolls to it. */
		white-space: pre;
	}

	.error {
		color: #f4645a;
	}

	.svg {
		display: block;
		max-width: none;
	}
</style>
