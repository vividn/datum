<!--
	A command's own output, cut to the last few lines.

	What a command with NO action echo said — a `tail` table, `stats`, an
	error — on a surface that is a fixed height and cannot scroll. vividn, on
	the bar: "If it fits in 3-4 lines, then just display, otherwise … show
	the last 4 lines." The LAST, because the end of a table is the newest
	rows and the end of an error is what went wrong.

	It lives on its own so that the two fixed-height surfaces share it
	rather than each having a copy: the bar's row (`ReplEntry.svelte`, where
	the history's first line is drawn) and the input widget's receipt
	(`BarResult.svelte`). The REPL's own rows do not use it — there the
	output is whole and the pane scrolls, which is the difference between a
	history and a status line.

	Nothing here composes a line of its own; the one exception is named
	where it is made.
-->
<script lang="ts">
	import { stripAnsi } from '$lib/ansi.js';

	/** What the command printed: stdout while there is any, else stderr. */
	export let text = '';
	/** Whether to paint it as a failure. */
	export let error = false;
	/** How many lines of it the surface has room for. */
	export let lines = 4;
	/** The FIRST lines instead: an explanation's point is at its top (da-vjwid). */
	export let head = false;

	const SVG_START = /^\s*(?:<\?xml[\s?]|<svg[\s>])/;

	$: drawing = SVG_START.test(text);
	$: all = stripAnsi(text).replace(/\n+$/, '').split('\n');
	$: shown = drawing ? [] : head ? all.slice(0, lines) : all.slice(-lines);
</script>

<div class="raw" class:error>
	{#if drawing}
		<!-- The one string on this surface that is not datum's: its stdout is
		     an SVG document, and four lines of XML is not what "show the
		     output" means. The picture itself is in the REPL, one drag away. -->
		<p class="aside">a drawing</p>
	{:else}
		{#each shown as line, i (i)}
			<p>{line}</p>
		{/each}
	{/if}
</div>

<style>
	.raw {
		display: flex;
		flex-direction: column;
		justify-content: center;
		min-width: 0;
		width: 100%;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		/* Four lines inside the section; see the bar's own height note. */
		font-size: 0.68rem;
		line-height: 1.25;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   REPL's echoed line uses (Repl.svelte). */
		color: #8888aa;
	}

	.raw.error {
		color: var(--error);
	}

	.raw p {
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: pre;
	}

	.aside {
		font-style: italic;
		color: var(--quiet);
	}
</style>
