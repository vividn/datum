<!--
	One document of a command's result (da-xfa4), as the bottom bar's
	status line shows it and as the REPL and the `+N` modal show it.

	Two lines when the entry carries no data — the action with its hid,
	then the info line — and three when it does, the data on the third as
	one line, cut with an ellipsis where it overruns. vividn: "If there is no
	extra data, show it in 2 lines … If there is extra data, then show 3
	lines, with all the extra data on one line (or truncated if there is a
	lot of extra data)", and "the font size could be bigger though when
	there are only 2 lines" — which is `small`, set by whoever draws the
	row, because sections standing side by side in one band share a size.

	Every string here is datum's own, out of `--show json` ($lib/echo.ts):
	the web places them and lets CSS truncate them, and composes none of
	them (docs/web-conventions.md #6).

	The colours are datum's own too (da-yu9s). `--show json` strips the
	ANSI, so the info line arrived here unpainted for a while and an echo
	read grey beside a table that was still in datum's field and state
	colours. It now arrives as `info_segments` — the runs the terminal
	would have painted, each with its hex — so the line is PLACED painted
	rather than searched for a field name (`switch mood mood` has it
	twice, and only the CLI knows which is which). A document without
	them falls back to the plain line.

	The full id is behind the hid — vividn: "don't show full id (rather
	showing that on hover over the hid)" — as a `title` for a pointer and,
	because touch has no hover, as a TAP that swaps the hid for the id
	until it is tapped again.
-->
<script lang="ts">
	import type { EchoDoc, InfoSegment } from '$lib/echo.js';

	export let doc: EchoDoc;
	/** The three-line size: set for the whole row when any document has data. */
	export let small = false;

	let shown = false;

	/** The line's runs, or the unpainted line as its one run. */
	let segments: InfoSegment[];
	$: segments = doc.info_segments?.length
		? doc.info_segments
		: [{ text: doc.info, color: null }];
</script>

<div class="doc" class:small>
	<div class="receipt">
		<span class="action">{doc.action}</span>
		<button
			type="button"
			class="hid"
			title={doc.id}
			aria-label="full id {doc.id}"
			aria-expanded={shown}
			on:click={() => (shown = !shown)}>{shown ? doc.id : doc.hid}</button
		>
	</div>
	<!-- Nothing may come between the spans: the line is monospace and
	     laid out with runs of spaces, and a newline in the markup is a
	     space in the DOM — so the hugging here is load-bearing, as it is
	     in AnsiText.svelte. -->
	<div class="info">{#each segments as segment}<span style:color={segment.color}>{segment.text}</span
		>{/each}</div>
	{#if doc.data_inline}
		<div class="data">{doc.data_inline}</div>
	{/if}
</div>

<style>
	.doc {
		display: flex;
		flex-direction: column;
		justify-content: center;
		min-width: 0;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		/* Two lines of room; the third fits because `small` shrinks the type
		   by more than a third and the band's height never changes. */
		font-size: 1rem;
		line-height: 1.35;
	}

	.doc.small {
		font-size: 0.72rem;
		line-height: 1.3;
	}

	/* Each line is one line: the band is a fixed height, so a string too
	   long for it is cut rather than allowed to push the dashboard.
	   `pre`, not `nowrap`: datum lays its info line out with runs of
	   spaces, and a browser that collapses them has changed what the CLI
	   printed. */
	.receipt,
	.info,
	.data {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: pre;
	}

	.receipt {
		/* Not a token: the same step between `--quiet` and `--ink` the
		   REPL's echoed line uses (Repl.svelte). */
		color: #8888aa;
	}

	.action {
		color: var(--accent-bright);
	}

	.hid {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
		text-decoration: underline dotted;
		text-underline-offset: 2px;
	}

	.hid:hover,
	.hid:focus-visible {
		color: var(--ink-bright);
	}

	.info {
		color: var(--ink-bright);
	}

	.data {
		color: var(--quiet);
	}
</style>
