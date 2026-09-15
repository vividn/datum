<!--
	The bottom bar's result area (da-xfa4) — the top 60% of the status
	line: what the last command answered, whatever the command was.

	Two shapes, and the first is the one the bar is for:

	- A command with an action echo (`--show json`, $lib/echo.js) is drawn
	  as SECTIONS, one per document ($lib/Docs.svelte).
	- A command with none — a `tail` table, `stats`, an error — shows its
	  own output: whole while it fits in four lines, otherwise the LAST
	  four. vividn: "If it fits in 3-4 lines, then just display, otherwise …
	  show the last 4 lines". What was cut is one tap away, in the REPL the
	  bar's own `^` opens over this area (da-bhvy) — the control is the
	  bar's, at its top edge, and no longer also this corner's.

	The second shape is also the FALLBACK (the bead's trap 1): a stdout
	with no document in it — an older wasm module, a process cut off
	mid-write — still shows the lines datum printed. Nothing here composes
	a line of its own; the one exception is named where it is made.

	The area's height never changes with what is in it. Every line is one
	line, cut with an ellipsis, and the count is capped: the bar is a fixed
	height and the dashboard above it must not reflow because a command
	printed a long answer.
-->
<script lang="ts">
	import { stripAnsi } from '$lib/ansi.js';
	import Docs from '$lib/Docs.svelte';
	import { parseEcho } from '$lib/echo.js';
	import type { RunResult } from '$lib/run.js';

	export let result: RunResult | null = null;

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

</style>
