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

	The bar is `ButtonRows` — how every button bar is laid out and
	scrolled (da-vu7pi): as many rows as fit, up to three, each packed on
	its own and all of them scrolled left and right.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { Pill } from './bar.js';
	import ButtonRows from './ButtonRows.svelte';
	import { commandsUnder, type Grammar } from './commands.js';

	export let disabled = false;
	export let grammar: Grammar | null = null;
	export let pills: Pill[] = [];

	const dispatch = createEventDispatcher<{ run: string[]; line: { name: string; line: string; keyboard: boolean } }>();

	$: listed = commandsUnder(grammar, pills);
</script>

<ButtonRows class="commands" items={listed} key={(command) => command.name} let:item={command}>
	<button slot="probe" class="command" tabindex="-1">probe</button>
	<button
		class="command"
		class:act={command.press === 'run'}
		{disabled}
		data-command={command.name}
		title={command.press === 'run' ? `run ${command.name}` : `${command.name} …`}
		on:click={() =>
			command.press === 'run'
				? dispatch('run', [command.name])
				: dispatch('line', { name: command.name, line: `${command.name} `, keyboard: command.next !== 'tap' })}
	>
		{command.label}
	</button>
</ButtonRows>

<style>
	.command {
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
</style>
