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

	The SCROLL RULE (vividn): one or two button-rows high and the bar is one
	row that scrolls left and right; three or more and it wraps and
	scrolls up and down. The rows are counted from the box the layout
	gave the bar and the height of `probe` — a real button, drawn and
	hidden — so the bar's own height never enters the question.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { COMMANDS, scrollsSideways, watchRows } from './commands.js';

	export let disabled = false;

	const dispatch = createEventDispatcher<{ run: string[]; line: string }>();

	let bar: HTMLElement;
	let probe: HTMLElement;
	let rows = 3;

	onMount(() => watchRows(bar, probe, (n) => (rows = n)));

	$: sideways = scrollsSideways(rows);
</script>

<!-- The measuring stick: a button of the ordinary kind, laid out and
     invisible, so the row height is the buttons' own. Outside the bar and
     out of flow, so nothing that reads the bar's buttons finds it. -->
<span class="probe" aria-hidden="true"><button class="command" bind:this={probe} tabindex="-1">probe</button></span>

<div class="bar commands" class:sideways bind:this={bar} data-rows={rows}>
	{#each COMMANDS as command (command.name)}
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

	/* One row, reached sideways: at one or two rows high there is no room
	   to wrap into, so the bar stops wrapping instead of hiding buttons. */
	.bar.sideways {
		flex-wrap: nowrap;
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
