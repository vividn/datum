<!--
	The bottom bar's result area (da-xfa4) — the top 60% of the status
	line: what the last command answered, whatever the command was.

	Two shapes, and the first is the one the bar is for:

	- A command with an action echo (`--show json`, $lib/echo.js) is drawn
	  as SECTIONS, one per document ($lib/Docs.svelte).
	- A command with none — a `tail` table, `stats`, an error — shows its
	  own output: whole while it fits in four lines, otherwise the LAST
	  four with a «(tap to expand)» that opens the REPL, exactly as the
	  bar's `^` does. vividn: "If it fits in 3-4 lines, then just display,
	  otherwise … show the last 4 lines with a (tap to expand), which when
	  tapped opens the REPL like hitting the ^".

	The second shape is also the FALLBACK (the bead's trap 1): a stdout
	with no document in it — an older wasm module, a process cut off
	mid-write — still shows the lines datum printed. Nothing here composes
	a line of its own; the one exception is named where it is made.

	The area's height never changes with what is in it. Every line is one
	line, cut with an ellipsis, and the count is capped: the bar is a fixed
	height and the dashboard above it must not reflow because a command
	printed a long answer. The expander costs no line of its own — it sits
	in the area's bottom right corner, over the end of the last line — so
	four lines is four lines and not four plus one.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { stripAnsi } from '$lib/ansi.js';
	import Docs from '$lib/Docs.svelte';
	import { parseEcho } from '$lib/echo.js';
	import type { RunResult } from '$lib/run.js';

	export let result: RunResult | null = null;

	const dispatch = createEventDispatcher<{ expand: void }>();

	/** How many lines of raw output the band holds. */
	const LINES = 4;

	const SVG_START = /^\s*(?:<\?xml[\s?]|<svg[\s>])/;

	$: docs = result ? parseEcho(result.stdout) : null;
	$: error = !!result && result.code !== 0;
	// stdout while there is any: a warning on stderr beside a good answer is
	// not the answer. With nothing on stdout — an error, a command that only
	// reports progress — stderr is all the CLI said, so it is what is shown.
	// Both are datum's; neither is rewritten.
	$: raw = result ? (result.stdout.trim() ? result.stdout : result.stderr) : '';
	$: drawing = SVG_START.test(raw);
	$: all = stripAnsi(raw).replace(/\n+$/, '').split('\n');
	$: lines = drawing ? [] : all.slice(-LINES);
	$: over = drawing || all.length > LINES;
</script>

<div class="result" class:error role="status">
	{#if docs}
		<Docs {docs} />
	{:else if result}
		<div class="raw">
			{#if drawing}
				<!-- The one string on this surface that is not datum's: its
				     stdout is an SVG document, and four lines of XML is not
				     what "show the output" means. The picture itself is in
				     the REPL, one tap away. -->
				<p class="aside">a drawing</p>
			{:else}
				{#each lines as line, i (i)}
					<p>{line}</p>
				{/each}
			{/if}
		</div>
		{#if over}
			<button type="button" class="expand" on:click={() => dispatch('expand')}
				>(tap to expand)</button
			>
		{/if}
	{/if}
</div>

<style>
	.result {
		position: relative;
		display: flex;
		align-items: stretch;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
	}

	.raw {
		display: flex;
		flex-direction: column;
		justify-content: center;
		min-width: 0;
		width: 100%;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		/* Four lines inside the tier; see the bar's own height note. */
		font-size: 0.68rem;
		line-height: 1.25;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   REPL's echoed line uses (Repl.svelte). */
		color: #8888aa;
	}

	.result.error .raw {
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

	/* Over the end of the last line, on the bar's own ground so the text
	   under it does not read through: the area is a fixed height, and a
	   fifth line would be a line the box has not got. */
	.expand {
		position: absolute;
		right: 0;
		bottom: 0;
		padding: 0 0 0 0.5rem;
		background: linear-gradient(to right, transparent, var(--ground) 0.5rem);
		border: none;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.68rem;
		line-height: 1.3;
		color: var(--accent);
		cursor: pointer;
	}

	.expand:hover,
	.expand:focus-visible {
		color: var(--accent-bright);
	}
</style>
