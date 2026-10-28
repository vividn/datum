<!--
	The bottom bar's result area (da-xfa4) — the top 60% of the status
	line: what the last command answered, whatever the command was.

	Two shapes, and the first is the one the bar is for:

	- A command with an action echo (`--show json`, #lib/echo.ts) is drawn
	  as SECTIONS, one per document (#lib/Docs.svelte).
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
	import Docs from '#lib/Docs.svelte';
	import RawLines from '#lib/RawLines.svelte';
	import { parseEcho } from '#lib/echo.ts';
	import type { RunResult } from '#lib/run.ts';

	let { result = null }: { result?: RunResult | null } = $props();

	/** How many lines of raw output the band holds. */
	const LINES = 4;

	const docs = $derived(result ? parseEcho(result.stdout) : null);
	const error = $derived(!!result && result.code !== 0);
	// stdout while there is any: a warning on stderr beside a good answer is
	// not the answer. With nothing on stdout — an error, a command that only
	// reports progress — stderr is all the CLI said, so it is what is shown.
	// Both are datum's; neither is rewritten.
	const raw = $derived(result ? (result.stdout.trim() ? result.stdout : result.stderr) : '');
</script>

<div class="result" class:error role="status">
	{#if docs}
		<div class="stack">
			<Docs {docs} />
			<!-- A run that wrote and then failed (da-0uvyr): its documents AND
			     the last line of its error, or it reads as a success. -->
			{#if error && result?.stderr.trim()}
				<RawLines text={result.stderr} {error} lines={1} />
			{/if}
		</div>
	{:else if result}
		<RawLines text={raw} {error} lines={LINES} />
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

	.stack {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}

	.stack > :global(.docs) {
		flex: 1;
		min-height: 0;
	}
</style>
