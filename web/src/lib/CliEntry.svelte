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

	A FRAME, the time and Undo on its top border (da-hxjrk). vividn,
	2026-10-29: «make the cli time smaller (and use the -1d framing exactly
	like how tail does it for the date if the date is not today), and put it
	right on the frame of the output section, that way the cli command gets
	the full top line within its box. Move the undo button to the top frame
	as well, also slightly smaller like the time text, but with a
	sufficiently large hit box.» Each entry the command echoed is a frame of
	its own inside this one (Doc.svelte).
-->
<script lang="ts">
	import CliOutput from '#lib/CliOutput.svelte';
	import Docs from '#lib/Docs.svelte';
	import RawLines from '#lib/RawLines.svelte';
	import { now, stampLabel } from '#lib/clock.ts';
	import { parseEcho } from '#lib/echo.ts';
	import { marks, type Command } from '#lib/cliStore.ts';
	import { step } from '#lib/layout/engine.ts';

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

	/**
	 * UNDO ON A COMMAND THAT WROTE, REDO ONCE IT IS UNDONE (da-25q3k). vividn,
	 * 2026-10-28: «when something is undone, rather than displaying the
	 * delete/change as a new action, just grey/hash out the command that was
	 * undone … put a redo button where the undo button was», and «disable the
	 * undo button if future writes bury the change. If you tap or hover over
	 * the button, "cannot undo. more recent changes" more recent changes is a
	 * link, which takes you to the latest entry that buries it».
	 */
	const mark = $derived($marks.get(entry));
	const verb = $derived(mark?.undone ? 'redo' : 'undo');
	/** Found at a press, changed since by a write this page never ran (`step`). */
	let elsewhere = $state(false);
	const refused = $derived(!!mark?.buried || elsewhere);
	/** The note on a refused button, shown. */
	let told = $state(false);
	let busy = $state(false);
	const note = $props.id();

	/** A mouse's hover already shows the note, which its click keeps; a tap or a key toggles it. */
	async function press(event: MouseEvent): Promise<void> {
		if (refused) told = (event as PointerEvent).pointerType === 'mouse' || !told;
		else if (mark && !busy) {
			busy = true;
			elsewhere = !(await step(entry, verb).finally(() => (busy = false)));
			told = elsewhere;
		}
	}

	/** To the command that buried this one: the one a button's step was of, when it was one. */
	function follow(event: MouseEvent): void {
		told = false;
		const ran = mark?.buried?.of ?? mark?.buried?.ranAt;
		const list = (event.currentTarget as HTMLElement).closest('[data-ran]')?.parentElement;
		const row = list?.querySelector(`:scope > [data-ran="${ran}"]`) ?? document.querySelector(`[data-ran="${ran}"]`);
		row?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
		row?.animate({ backgroundColor: ['color-mix(in srgb, var(--accent) 30%, transparent)', 'transparent'] }, 1200);
	}
</script>

<div class="frame" class:capped={!!cap} class:marked={!!mark} class:struck={mark?.undone}>
	<div class="head" class:capped={!!cap} class:all class:marked={!!mark} class:struck={mark?.undone}>
		<span class="stamp">{stampLabel(new Date(entry.ranAt), $now)}</span>
		<!-- The stamp is a sibling of `.echo`, not a child of it: the echo's
		     text is the command and nothing else, which is what every reader of
		     it — a test, a copy-paste — already assumes. -->
		<div class="echo"><span class="prompt">❯</span> {entry.line}</div>
		{#if mark}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<span
				class="step"
				onpointerenter={(e) => e.pointerType === 'mouse' && (told = refused)}
				onpointerleave={(e) => e.pointerType === 'mouse' && (told = false)}
				onfocusout={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && (told = false)}
			>
				<button
					type="button"
					class={verb}
					aria-disabled={refused || busy}
					aria-describedby={refused ? note : undefined}
					onclick={press}>{verb}</button
				>
				<!-- In the DOM while refused, so the button is described as refused
				     before it is pressed; drawn once it is hovered or pressed. -->
				{#if refused}
					<span id={note} class="buried" class:told role="status"
						>cannot {verb}. {#if mark.buried}<button
								type="button"
								class="link"
								tabindex={told ? 0 : -1}
								onclick={follow}>more recent changes</button
							>{:else}a change made elsewhere{/if}</span
					>
				{/if}
			</span>
		{/if}
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
</div>

<style>
	/* THE COMMAND'S FRAME: the time and the step stand on its top border as
	   a fieldset's legend does, the half of them above it the frame's
	   margin, so the command has the whole of the first line inside. */
	.frame {
		position: relative;
		min-width: 0;
		margin-top: calc(var(--label) / 2);
		padding: calc(var(--label) / 2) 0.45em 0.15em;
		border: 1px solid var(--quiet);
		border-radius: 0.45em;
		/* Named, not inherited: the step's hit box is measured in it (below). */
		line-height: 1.35;
		/* The label's height: its type, a step down, on one line. */
		--label: calc(0.75em * 1.2);
		/* The step's hit box, at least a finger's on every surface. */
		--hit: 44px;
	}

	.echo {
		min-width: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   bottom bar's receipt uses (PromptBar.svelte). */
		color: #8888aa;
	}

	/* Clear of the step's hit box, which reaches down past the border to
	   the end of this line. */
	.head.marked .echo {
		padding-right: calc(var(--hit) + 0.5em);
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

	/* ON THE BORDER, the time at its left and the step at its right, a step
	   down from the command; their ground hides the border behind them. */
	.stamp,
	.step {
		position: absolute;
		top: 0;
		transform: translateY(-50%);
		padding: 0 0.25em;
		font-size: 0.75em;
		line-height: 1.2;
		background: var(--ground);
	}

	/* Dimmer than the echoed command and much dimmer than the data under
	   it: this is when you typed, and the entry's own occur_time — which
	   its frame prints under its field — is what actually matters. */
	.stamp {
		left: 0.4em;
		white-space: pre;
		color: var(--quiet);
		font-variant-numeric: tabular-nums;
	}

	.prompt {
		color: var(--accent);
	}

	.step {
		right: 0.4em;
		z-index: 2;
	}

	.step > button {
		position: relative;
		padding: 0 0.4em;
		font: inherit;
		line-height: inherit;
		color: var(--accent-bright);
		background: var(--ground);
		/* An inset ring, not a border, which would make the line 2px taller. */
		border: none;
		box-shadow: inset 0 0 0 1px var(--accent);
		border-radius: 0.3em;
		cursor: pointer;
	}

	/* THE HIT BOX, larger than the label it is behind (da-hxjrk): vividn,
	   «slightly smaller like the time text, but with a sufficiently large
	   hit box». It stands on the foot of the command's first line, whose
	   text keeps clear of it (`.marked`), and reaches up out of the frame:
	   3em of the step's type is its label (1.2) and the line (1.35 of the
	   frame's 0.75 more). */
	.step > button::before {
		content: '';
		position: absolute;
		top: calc(3em - var(--hit));
		right: -0.25em;
		width: max(100% + 0.5em, var(--hit));
		height: var(--hit);
	}

	/* The bar's band has no room above it: there the box reaches down from
	   the band's top edge instead, over the top border of the entry under
	   the command, whose label stands over it: a press on the hid, the full
	   id open or not, is the hid's and never an undo. */
	.frame.capped .step > button::before {
		top: -4px;
	}

	.frame.capped :global(.doc > .receipt) {
		z-index: 3;
	}

	.step > button[aria-disabled='true'] {
		color: var(--quiet);
		box-shadow: inset 0 0 0 1px var(--tint);
		cursor: default;
	}

	.buried:not(.told) {
		clip-path: inset(50%);
		pointer-events: none;
	}

	.buried {
		position: absolute;
		top: 100%;
		right: 0;
		z-index: 3;
		width: max-content;
		max-width: 20ch;
		padding: 0.2em 0.4em;
		color: var(--ink);
		background: var(--ground);
		border: 1px solid var(--tint);
		border-radius: 0.3em;
	}

	.link {
		padding: 0;
		font: inherit;
		color: var(--accent-bright);
		background: none;
		border: none;
		text-decoration: underline;
		cursor: pointer;
	}

	/* AN UNDONE COMMAND, HASHED OUT WHERE IT STANDS: its frame hatched, and
	   all of it but its redo faded. */
	.frame.struck {
		background-image: repeating-linear-gradient(
			135deg,
			transparent 0 5px,
			color-mix(in srgb, var(--quiet) 30%, transparent) 5px 6px
		);
	}

	.frame.struck > :not(.head),
	.struck .stamp,
	.struck .echo {
		opacity: 0.45;
	}

	.struck .echo {
		text-decoration: line-through;
	}
</style>
