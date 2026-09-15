<!--
	One document of a command's result (da-xfa4), as the bottom bar's
	status line shows it and as the REPL and the `+N` modal show it.

	TWO LINES, always at the two-line size — the action with its hid, then
	the info line — and the entry's data BESIDE them, in a brace (da-f4mw).
	It used to be a third line, with the whole row shrunk to 0.72rem so
	three would fit the fixed band; vividn, from the train: "The extra data is
	almost invisible in the output part of the input bar. Rather than
	changing the lines to be 3 in this case, let's put the extra data next
	to the 2 lines in a big {}. The font size can be smaller here, up to 4
	lines. If there are more than 4 rows of extra data, maybe show a +N; if
	there is no extra data, no {} is necessary." So the lines keep the size
	vividn asked for them ("the font size could be bigger though when there
	are only 2 lines") whatever the entry carries, and the data is read in
	its own column rather than as a grey line under a shrunken one.

	`all` is the form with no band to fit — the REPL the bar's `^` opens
	over its result area (da-bhvy), and the `+N` sheet: every row is drawn,
	so a document the tier had cut to four and a `+N` is read whole there.

	The brace holds up to `ROWS` of the CLI's `key: value` strings, in the
	CLI's order, at a size DERIVED from the two lines' height rather than
	picked by eye — four of them fit inside it, which is what keeps the
	band's 6rem the same number after every command (PromptBar.svelte).
	Past four: four and a `+N`, whose sheet lists every key. No data: no
	brace, and the two lines take the whole width.

	`tight` is the 320px phone, and it is measured and not guessed: a
	section 239px wide wants 183px for its info line and 71px for the
	brace's own furniture, which leaves the rows FOUR pixels. Four rows of
	one character each are not the data, so a section that narrow shows
	`{ +N }` — the count of every key, opening the same sheet — and the
	info line keeps the width it needs (the bead's trap 2). Which sections
	are that narrow is Docs.svelte's to say: it is the surface that
	measures.

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
	import Sheet from '$lib/Sheet.svelte';

	export let doc: EchoDoc;
	/** The section is too narrow for a row to read: show the count alone. */
	export let tight = false;
	/** No band to fit — the REPL, the sheet: every row, no `+N`. */
	export let all = false;

	/** Rows of data the brace holds before it goes to a `+N`. */
	const ROWS = 4;

	let shown = false;
	let open = false;

	/** The line's runs, or the unpainted line as its one run. */
	let segments: InfoSegment[];
	$: segments = doc.info_segments?.length
		? doc.info_segments
		: [{ text: doc.info, color: null }];

	/** The CLI's `key: value` strings — `data`, not the terminal's join. */
	let rows: string[];
	$: rows = doc.data ?? [];
	$: visible = tight ? [] : all ? rows : rows.slice(0, ROWS);
	$: hidden = rows.length - visible.length;
</script>

<div class="doc">
	<div class="lines">
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
		<div class="info">{#each segments as segment}<span style:color={segment.color}
				>{segment.text}</span
			>{/each}</div>
	</div>
	{#if rows.length}
		<div class="brace" style:--rows={Math.max(visible.length, 1)}>
			<span class="curly" aria-hidden="true">&#123;</span>
			{#if visible.length}
				<div class="rows">
					{#each visible as row, i (i)}
						<div class="row">{row}</div>
					{/each}
				</div>
			{/if}
			{#if hidden > 0}
				<button type="button" class="more" title="{hidden} more" on:click={() => (open = true)}
					>+{hidden}</button
				>
			{/if}
			<span class="curly" aria-hidden="true">&#125;</span>
		</div>
	{/if}
</div>

{#if hidden > 0}
	<Sheet title="{rows.length} keys" bind:open>
		<div class="keys">
			{#each rows as row, i (i)}
				<div class="key">{row}</div>
			{/each}
		</div>
	</Sheet>
{/if}

<style>
	/* The lines and the brace stand side by side, centred in whatever
	   height the surface gives the section. */
	.doc {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		min-width: 0;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		/* The two-line size, now the only one: 2 × 1.35rem = 2.7rem, which
		   is the band's deepest document case (PromptBar.svelte). */
		font-size: 1rem;
		line-height: 1.35;
	}

	/* Only as wide as the lines themselves: the brace then HUGS them —
	   vividn's "next to the 2 lines" — rather than sitting at the far edge
	   of a wide section. */
	.lines {
		flex: 0 1 auto;
		min-width: 0;
	}

	/* Each line is one line: the band is a fixed height, so a string too
	   long for it is cut rather than allowed to push the dashboard.
	   `pre`, not `nowrap`: datum lays its info line out with runs of
	   spaces, and a browser that collapses them has changed what the CLI
	   printed. */
	.receipt,
	.info,
	.row {
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

	.brace {
		/* Shrinks four times as readily as the lines do, and never past
		   this cap: on a 320px phone it is the VALUES that ellipsize, never
		   the info line the section is for (the bead's trap 2). */
		flex: 0 4 auto;
		display: flex;
		align-items: center;
		gap: 0.15rem;
		min-width: 0;
		max-width: 40%;
	}

	/* One glyph pair as tall as the rows it holds — so a brace is never
	   taller than the two lines beside it, whatever `--rows` is. */
	.curly {
		flex: none;
		font-size: calc(var(--rows) * 0.55rem);
		line-height: 1;
		color: var(--muted);
	}

	/* Four rows inside the two lines' height, derived from it and not
	   chosen: 2 × 1.35rem leaves 0.675rem a row, and 0.6rem × 1.1 =
	   0.66rem puts the four just inside that. */
	.rows {
		min-width: 0;
		font-size: 0.6rem;
		line-height: 1.1;
		/* vividn's whole complaint was that this was almost invisible; it is
		   the entry's own data, so it reads as the lines do, a step down. */
		color: var(--ink);
	}

	/* The docs' own chip (Docs.svelte), at the brace's size — beside the
	   rows and not under them, because a fifth row is a row the band has
	   not got. */
	.more {
		flex: none;
		align-self: center;
		padding: 0 0.25rem;
		background: none;
		border: 1px solid var(--accent);
		border-radius: 999px;
		font-family: inherit;
		font-size: 0.6rem;
		line-height: 1.4;
		color: var(--accent-bright);
		cursor: pointer;
	}

	.more:hover,
	.more:focus-visible {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}

	/* In the sheet there is no band to fit, so every key is listed at a
	   size meant for reading. */
	.keys {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
		line-height: 1.5;
		color: var(--ink);
	}

	.key {
		white-space: pre-wrap;
	}
</style>
