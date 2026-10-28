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
	import { act } from '$lib/act.js';
	import type { EchoDoc, InfoSegment } from '$lib/echo.js';
	import Sheet from '$lib/Sheet.svelte';
	import { select, selected } from '$lib/bar.js';
	import { isSelected } from '$lib/selection.js';
	import { tapped } from '$lib/tap.js';

	interface Props {
		doc: EchoDoc;
		/** The section is too narrow for a row to read: show the count alone. */
		tight?: boolean;
		/** No band to fit — the REPL, the sheet: every row, no `+N`. */
		all?: boolean;
	}

	let { doc, tight = false, all = false }: Props = $props();

	/** Rows of data the brace holds before it goes to a `+N`. */
	const ROWS = 4;

	let shown = $state(false);
	let open = $state(false);

	/** The line's runs, or the unpainted line as its one run. */
	const segments: InfoSegment[] = $derived(
		doc.info_segments?.length ? doc.info_segments : [{ text: doc.info, color: null }]
	);

	/** The CLI's `key: value` strings — `data`, not the terminal's join. */
	const rows: string[] = $derived(doc.data ?? []);
	const visible = $derived(tight ? [] : all ? rows : rows.slice(0, ROWS));
	const hidden = $derived(rows.length - visible.length);

	/**
	 * A tap on the document selects its entry (da-r13j); the hid and the
	 * `+N` are buttons of their own and keep their taps. A range runs over
	 * the documents drawn around it — the pane's, the bar's, the sheet's —
	 * in the order they are drawn.
	 */
	function tap(e: MouseEvent): void {
		const el = e.currentTarget as HTMLElement;
		if ((e.target as HTMLElement).closest('button') || !tapped(e)) return;
		const scope = el.closest('dialog, section, .prompt-bar') ?? document.body;
		const order = [...scope.querySelectorAll<HTMLElement>('.doc[data-hid]')].map((d) => d.dataset.hid ?? '');
		select(doc.hid, e, order);
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
	class="doc"
	class:all
	class:selected={isSelected($selected, doc.hid)}
	data-hid={doc.hid}
	onmousedown={(e) => e.shiftKey && e.preventDefault()}
	onclick={tap}
>
	<div class="lines">
		<div class="receipt">
			<span class="action" style:color={doc.action_color}>{doc.action}</span>
			<button
				type="button"
				class="hid"
				title={doc.id}
				aria-label="full id {doc.id}"
				aria-expanded={shown}
				onclick={() => (shown = !shown)}>{shown ? doc.id : doc.hid}</button
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
				<button type="button" class="more" use:act={'open'} title="{hidden} more" onclick={() => (open = true)}
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
		cursor: pointer;
	}

	/* A selected entry (da-r13j): the accent the hid pill is filled with. */
	.doc.selected {
		background: color-mix(in srgb, var(--accent) 22%, transparent);
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

	/* A SURFACE WITH NO BAND TO FIT CUTS NOTHING (da-5iyo). The REPL and the
	   sheet have the vertical room the bar's fixed band has not, so a line
	   too long for its column wraps there instead of ellipsizing, and the
	   brace drops under the lines rather than squeezing them. That is what
	   lets an entry stay whole in a column narrow enough for two of them on
	   a phone; the bar's row still clips, because a fixed-height band that
	   wrapped would push the dashboard.

	   `pre-wrap` and not plain wrapping: datum lays the info line out with
	   runs of spaces, and a browser that collapses them has changed what
	   the CLI printed. At a width the line already fits — the single column
	   it has had until now — nothing about it moves. */
	.doc.all .receipt,
	.doc.all .info,
	.doc.all .row {
		overflow: visible;
		text-overflow: clip;
		white-space: pre-wrap;
	}

	.doc.all {
		flex-wrap: wrap;
	}

	/* No 40% cap once it is on a line of its own: the cap is there to keep
	   the band's info line ahead of the data, and under the lines there is
	   nothing left for it to take. */
	.doc.all .brace {
		flex: 0 1 auto;
		max-width: 100%;
	}

	.receipt {
		/* Not a token: the same step between `--quiet` and `--ink` the
		   REPL's echoed line uses (Repl.svelte). */
		color: #8888aa;
	}

	/* datum's own colour for the tag when the module carries one (da-ckw3);
	   the page's accent when it does not, which is what it always was. */
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

	/* Cut at the line's end, not dropped: a button is an atomic inline, and
	   one that overflows an ellipsized line is hidden whole, so the full id
	   tapped open in a document one column of the bar's strip wide
	   (da-qua5g) left only the `…`. 8ch is the action word and its space. */
	.doc:not(.all) .hid {
		max-width: calc(100% - 8ch);
		overflow: hidden;
		text-overflow: ellipsis;
		vertical-align: top;
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
