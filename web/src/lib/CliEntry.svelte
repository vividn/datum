<!--
	ONE ROW OF THE HISTORY: a command, when it ran, and what it answered.

	There is one list and this is a row of it (da-ckw3). vividn, 2026-09-21:
	"the output section (before you drag up the rest of the repl) should
	actually *be* the first line of the repl. That is, it shouldn't be
	repeated when you drag it up. It's just the last entry of the last
	entries. If that makes sense. It should also have the same format as the
	rest of the repl. So let's actually show (in small characters, the entry
	text and time of the entry text."

	It used to be two renderings of one thing: the bar's output section drew
	`BarResult`, the CLI widget drew its own rows, and dragging the log up met the
	newest entry a second time in a different shape. So the row moved here,
	and both surfaces mount THIS — the bar for the newest entry, the log for
	the ones under it. Neither can drift from the other because there is
	only one of them.

	"The entry text and time of the entry text, in small characters" is the
	`.stamp` and the `.echo` below, at the sizes the CLI widget already drew them:
	he asked for the format the rest of the history has, so a smaller size
	invented here would be a third format rather than the end of the second.
-->
<script lang="ts">
	import CliOutput from '#lib/CliOutput.svelte';
	import Docs from '#lib/Docs.svelte';
	import RawLines from '#lib/RawLines.svelte';
	import { now, stampLabel } from '#lib/clock.ts';
	import { parseEcho } from '#lib/echo.ts';
	import type { Command } from '#lib/cliStore.ts';

	let {
		entry,
		/**
		 * Show every document a multi-document command echoed.
		 *
		 * The history has the room and shows them all; the bar's output section
		 * is a fixed height and shows what fits with a `+N` for the rest
		 * (Docs.svelte). Same component, same row, one number of difference.
		 */
		all = false,
		/**
		 * How many lines of RAW output a surface that cannot scroll has room for,
		 * or 0 for all of it.
		 *
		 * The other number of difference between the two surfaces. A command with
		 * no action echo — a `tail` table, `stats`, an error — is whole in the
		 * history, where the pane scrolls; in the bar's row it is the LAST few
		 * lines, because vividn asked for that before this row existed: "If it fits
		 * in 3-4 lines, then just display, otherwise … show the last 4 lines."
		 * A fixed-height box that simply clipped would show the FIRST lines,
		 * which for a table is the oldest rows and for an error is the preamble.
		 */
		cap = 0
	}: {
		entry: Command;
		all?: boolean;
		cap?: number;
	} = $props();

	/** A command's documents, when it echoed any (#lib/echo.ts). */
	const docs = $derived(parseEcho(entry.result.stdout));
	// stdout while there is any: a warning on stderr beside a good answer is
	// not the answer. With nothing on stdout — an error, a command that only
	// reports progress — stderr is all the CLI said, so it is what is shown.
	// Both are datum's; neither is rewritten.
	const raw = $derived(entry.result.stdout.trim() ? entry.result.stdout : entry.result.stderr);
	const failed = $derived(entry.result.code !== 0 && !!entry.result.stderr.trim());
</script>

<div class="head" class:capped={!!cap} class:all>
	<span class="stamp">{stampLabel(new Date(entry.ranAt), $now)}</span>
	<!-- The stamp is a sibling of `.echo`, not a child of it: the echo's
	     text is the command and nothing else, which is what every reader of
	     it — a test, a copy-paste — already assumes. -->
	<div class="echo"><span class="prompt">❯</span> {entry.line}</div>
</div>
{#if cap}
	<!-- A surface that cannot scroll shows ONE box: four lines is not enough
	     to spend on a stdout and a stderr separately, and what the reader
	     needs there is the last thing said. -->
	{#if docs}
		<Docs {docs} {all} />
		<!-- A run that wrote and then failed echoes its writes AND exits
		     non-zero (da-0uvyr): `del <good>,<bad>` deleted one entry. The
		     documents alone would read as a success, so the error's last
		     line goes under them. -->
		{#if failed}
			<RawLines text={entry.result.stderr} lines={1} error />
		{/if}
	{:else}
		<RawLines text={raw} lines={cap} error={entry.result.code !== 0} />
	{/if}
{:else}
	{#if docs}
		<!-- The same sections both surfaces draw, one per document (da-xfa4):
		     the web asks every command it runs for `--show json`, so a
		     multi-document result is side by side in each.

		     `--show json` carries the CLI's strings with its ANSI stripped,
		     which for a while cost the echo datum's field and state colours
		     while a table, a picture and an error kept theirs — they carry no
		     echo and arrive as the CLI printed them. The document now carries
		     the colours as DATA (`info_segments`, da-yu9s) and `Doc.svelte`
		     paints from those, so the separation vividn asked for ("The whole
		     web interface can take use json as the default show and then
		     render on it's own terms which is nice separation") no longer
		     costs the colour. -->
		<Docs {docs} {all} />
	{:else if entry.result.stdout}
		<CliOutput text={entry.result.stdout} />
	{/if}
	<!-- ALONGSIDE the branch above, not instead of it. A command can both
	     echo a document and warn on stderr — `undo` past ten minutes does
	     exactly that — and nesting this inside that branch dropped the
	     warning silently. -->
	{#if entry.result.stderr}
		<CliOutput
			text={entry.result.stderr}
			stream="stderr"
			error={entry.result.code !== 0}
		/>
	{/if}
{/if}

<style>
	.head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.echo {
		flex: 1;
		min-width: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   bottom bar's receipt uses (PromptBar.svelte). */
		color: #8888aa;
	}

	/* BESIDE THE TIME WHEN IT FITS THERE, UNDER IT WHEN IT DOES NOT
	   (da-5iyo). The history's columns are narrow enough for two on a
	   phone, and a stamp taking 65px off the front of a 169px one left
	   `occur brew bean=ethiopian …` breaking mid-word in the strip that
	   was left. `flex: 1 1 auto` is what asks the question: the echo's
	   basis is the command's own width, so a line that cannot sit beside
	   the stamp wraps under it and takes the whole column. A command
	   short enough is where it always was, at every width. */
	.head.all {
		flex-wrap: wrap;
	}

	.head.all .echo {
		flex: 1 1 auto;
	}

	/* ONE LINE on a surface that cannot scroll. A command long enough to wrap
	   — `occur brew a=1 bean=ethiopian c=3 grind=fine e=5 water=94C` wraps to
	   four lines at 320px — would otherwise take the whole of the bar's row
	   and leave its own answer nowhere to go. The history keeps the wrap,
	   because there the pane scrolls. */
	.head.capped .echo {
		white-space: pre;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Dimmer than the echoed command and much dimmer than the data under
	   it: this is when you typed, and the entry's own occur_time — which
	   the renderer prints two lines down — is what actually matters. */
	.stamp {
		flex: none;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}

	.prompt {
		color: var(--accent);
	}
</style>
