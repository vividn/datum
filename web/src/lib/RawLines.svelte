<!--
	A command's own output, cut to the last few lines.

	What a command with NO action echo said — a `tail` table, `stats`, an
	error — on a surface that is a fixed height and cannot scroll. vividn, on
	the bar: "If it fits in 3-4 lines, then just display, otherwise … show
	the last 4 lines." The LAST, because the end of a table is the newest
	rows and the end of an error is what went wrong.

	It lives on its own so that the two fixed-height surfaces share it
	rather than each having a copy: the bar's row (`CliEntry.svelte`, where
	the history's first line is drawn) and the input widget's receipt
	(`BarResult.svelte`). The CLI widget's own rows do not use it — there the
	output is whole and the pane scrolls, which is the difference between a
	history and a status line.

	Nothing here composes a line of its own; the one exception is named
	where it is made.
-->
<script lang="ts">
	import { stripAnsi } from '#lib/ansi.ts';

	interface Props {
		/** What the command printed: stdout while there is any, else stderr. */
		text?: string;
		/** Whether to paint it as a failure. */
		error?: boolean;
		/** How many lines of it the surface has room for. */
		lines?: number;
	}

	let { text = '', error = false, lines = 4 }: Props = $props();

	const SVG_START = /^\s*(?:<\?xml[\s?]|<svg[\s>])/;

	const drawing = $derived(SVG_START.test(text));
	const all = $derived(stripAnsi(text).replace(/\n+$/, '').split('\n'));
	const shown = $derived(drawing ? [] : all.slice(-lines));
</script>

<div class="raw" class:error class:single={shown.length === 1}>
	{#if drawing}
		<!-- The one string on this surface that is not datum's: its stdout is
		     an SVG document, and four lines of XML is not what "show the
		     output" means. The picture itself is in the CLI widget, one drag away. -->
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
		/* Centered by the two struts below while the lines fit; past that there
		   is nothing to share, and a column on its foot keeps the last of them. */
		justify-content: flex-end;
		min-width: 0;
		width: 100%;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		/* Five lines inside a phone's section, which is what `check`'s one line
		   wraps to at 390 wide; see the bar's own height note. */
		font-size: 0.68rem;
		line-height: 1.15;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   CLI widget's echoed line uses (CliScrollback.svelte). */
		color: #8888aa;
	}

	.raw::before,
	.raw::after {
		content: '';
		flex: 1;
	}

	/* One line that wrapped past the room is read from its start: `check`'s
	   verdict comes first, and what it cuts is the end of one sentence. */
	.raw.single {
		justify-content: flex-start;
	}

	.raw.error {
		color: var(--error);
	}

	/* A line too long for the width wraps (da-3d1ro): `pre-wrap`, not `normal`,
	   because datum lays its lines out with runs of spaces. Only what holds the
	   lines cuts them, so no line is cut with room left under it. */
	.raw p {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.aside {
		font-style: italic;
		color: var(--quiet);
	}
</style>
