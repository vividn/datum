<!--
	The commands widget — every command datum has, as a button (da-ltux).

	vividn: «Commands widget, which lists all commands. First one is Undo.
	Then let's have occur, start, end, switch, add, then other commands.»
	The order and what each press means are `$lib/commands.ts`; this is
	the bar that draws them.

	Three kinds of press, and the widget itself runs none of them — it
	composes and dispatches, and the page runs it through `run()` and
	echoes the executed argv, exactly as the fields widget does:

	- `Undo` / `Redo` RUN, at once. Undo never refuses on age any more
	  (da-s6no: with a way forward the fifteen-minute gate cost more than
	  it saved), so there is no second clock here — the widget runs `undo`
	  and the CLI's own warning past ten minutes is shown with the echo.
	- a VERB puts `<verb> ` on the command line and leaves the caret in
	  it. That is the whole of pending a verb: the fields widget reads the
	  line's first word, so tapping `occur` and typing `occur` reach the
	  same state through the same buffer.
	- anything else puts `<name> ` on the line too — a command with an
	  argument to finish, and nothing to filter.

	The loaded PILLS filter it (da-r13j): a command whose grammar — `datum
	help --json` — cannot take every pill on the bar is not drawn
	(`commandsUnder`). A selected hid leaves the id commands; a `-t` the
	ones that take a time.

	The SCROLL RULE (vividn): one or two button-rows high and the bar is one
	row that scrolls left and right; three or more and it wraps and
	scrolls up and down. «One row» is the scrolling, not the buttons: two
	rows high draws the buttons on two rows (da-hs6c). The rows are counted from the box the layout
	gave the bar and the height of `probe` — a real button, drawn and
	hidden — so the bar's own height never enters the question.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import type { Pill } from './bar.js';
	import { commandsUnder, scrollsSideways, watchRows, type Grammar } from './commands.js';

	export let disabled = false;
	export let grammar: Grammar | null = null;
	export let pills: Pill[] = [];

	const dispatch = createEventDispatcher<{ run: string[]; line: string }>();

	let bar: HTMLElement;
	let probe: HTMLElement;
	let rows = 3;

	onMount(() => watchRows(bar, probe, (n) => (rows = n)));

	$: sideways = scrollsSideways(rows);
	$: listed = commandsUnder(grammar, pills);
</script>

<!-- The measuring stick: a button of the ordinary kind, laid out and
     invisible, so the row height is the buttons' own. Outside the bar and
     out of flow, so nothing that reads the bar's buttons finds it. -->
<span class="probe" aria-hidden="true"><button class="command" bind:this={probe} tabindex="-1">probe</button></span>

<div class="bar commands" class:sideways bind:this={bar} data-rows={rows} style:--rows={rows}>
	{#each listed as command (command.name)}
		<button
			class="command"
			class:act={command.press === 'run'}
			{disabled}
			data-command={command.name}
			title={command.press === 'run' ? `run ${command.name}` : `${command.name} …`}
			on:click={() =>
				command.press === 'run'
					? dispatch('run', [command.name])
					: dispatch('line', `${command.name} `)}
		>
			{command.label}
		</button>
	{/each}
</div>

<style>
	.bar {
		flex: 1;
		min-height: 0;
		/* A flex item will not shrink under its content unless told to, and
		   a row of buttons that cannot shrink spills out of the pane instead
		   of scrolling inside it. */
		min-width: 0;
		display: flex;
		flex-wrap: wrap;
		align-content: flex-start;
		align-items: center;
		gap: 0.4rem;
		overflow-x: hidden;
		overflow-y: auto;
	}

	/* Reached sideways: at one or two rows high the buttons fill as many
	   rows as fit and the bar scrolls left and right (da-hs6c). A grid
	   flowing by COLUMN, not a column-wrapping flex box: it keeps the DOM
	   order (tab order, reading order) while drawing Undo over Redo, occur
	   over start, so the commands used most stay together at the left
	   edge. `--rows` is the measured count, never the content's height;
	   each row takes an equal share of the box and centres its buttons, as
	   the one-row flex line did. A column is as wide as its widest button,
	   and the narrower one keeps its label's width, ragged left. */
	.bar.sideways {
		display: grid;
		grid-auto-flow: column;
		grid-template-rows: repeat(var(--rows), minmax(0, 1fr));
		grid-auto-columns: max-content;
		justify-content: start;
		justify-items: start;
		overflow-x: auto;
		overflow-y: hidden;
	}

	.command {
		flex: none;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 3px;
		padding: 0.35rem 0.85rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.85rem;
		min-height: 24px;
		white-space: nowrap;
		cursor: pointer;
		user-select: none;
		-webkit-user-select: none;
		touch-action: manipulation;
	}

	/* Undo and Redo do something the moment they are pressed, where every
	   other button only writes a line — so they are drawn in the accent
	   that the rest are not. */
	.command.act {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.command:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.command:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.probe {
		position: absolute;
		visibility: hidden;
		pointer-events: none;
	}
</style>
