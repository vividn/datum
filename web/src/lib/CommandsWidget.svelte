<!--
	The commands widget — every command datum has, as a button (da-ltux).

	vividn: «Commands widget, which lists all commands. First one is Undo.
	Then let's have occur, start, end, switch, add, then other commands.»
	The order and what each press means are `$lib/commands.ts`; this is
	the bar that draws them.

	Three kinds of press, and the widget itself runs none of them — it
	composes and dispatches, and the page runs it through `run()` and
	echoes the executed argv, exactly as the fields widget does:

	- `Undo` RUNS, at once. Undo never refuses on age any more (da-s6no:
	  with a way forward the fifteen-minute gate cost more than it saved),
	  so there is no second clock here — the widget runs `undo` and the
	  CLI's own warning past ten minutes is shown with the echo. Its REDO
	  is a small icon joined to it (da-zq91a), always drawn and lit only
	  when `redoable` says so — which is the CLI's answer and not this
	  widget's guess (`redo --dry-run`, asked in CommandPane.svelte).
	- a command the loaded pills COMPLETE runs at once too (da-3lxol):
	  `delete` with an hid selected deletes it. `fires` in commands.ts
	  decides, and the same answer draws the button with a DOUBLE outline
	  (`.act`), so what looks as if it fires is what fires.
	- `Help` (da-vjwid) ARMS the page — `$lib/help.ts`: the next tap on a
	  command, a widget or a pill is explained instead of acted on, and a
	  second press on Help is `datum help`, and the answer opens in a modal.
	  Armed, its label says so — `? on what`, over how to get the general
	  help — in the same width, which is small: on a phone's one row of
	  commands a wider Help pushes the verbs off the screen. It is drawn
	  in a SINGLE outline of the
	  accent: it does something, but not at once.
	- a VERB puts `<verb> ` on the command line and leaves the caret in
	  it. That is the whole of pending a verb: the fields widget reads the
	  line's first word, so tapping `occur` and typing `occur` reach the
	  same state through the same buffer.
	- anything else puts `<name> ` on the line too — a command with an
	  argument to finish, and nothing to filter.

	The loaded PILLS filter it (da-r13j): a command whose grammar — `datum
	help --json` — cannot take every pill on the bar is not drawn
	(`commandsUnder`). A selected hid leaves the id commands; a `-t` the
	ones that take a time; a field the verbs that field can take
	(da-57j14: an occur field hides start, end and switch).

	The bar is `ButtonRows` — how every button bar is laid out and
	scrolled (da-vu7pi): as many rows as fit, up to three, each packed on
	its own and all of them scrolled left and right.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { Pill } from './bar.js';
	import type { FieldButton } from './fields.js';
	import ButtonRows from './ButtonRows.svelte';
	import { commandsUnder, fires, type CommandButton, type Grammar } from './commands.js';
	import { arm, helping } from './help.js';

	export let disabled = false;
	export let grammar: Grammar | null = null;
	export let pills: Pill[] = [];
	/** The fields widget's buttons: what a field pill's verbs are read off. */
	export let fields: FieldButton[] = [];
	/** Would `redo` move anything? The CLI's own answer, from the pane. */
	export let redoable = false;

	const dispatch = createEventDispatcher<{ run: string[]; line: { name: string; line: string; keyboard: boolean } }>();

	$: listed = commandsUnder(grammar, pills, fields);
	$: runs = (command: CommandButton) => command.press === 'run' || fires(grammar?.get(command.name), pills);

	const press = (command: CommandButton) =>
		runs(command)
			? dispatch('run', [command.name])
			: dispatch('line', { name: command.name, line: `${command.name} `, keyboard: command.next !== 'tap' });
</script>

<ButtonRows class="commands" items={listed} key={(command) => command.name} let:item={command}>
	<button slot="probe" class="command" tabindex="-1">probe</button>
	{#if command.name === 'undo'}
		<span class="undo">
			<button class="command act" {disabled} data-command="undo" aria-label="undo" title="run undo" on:click={() => press(command)}>
				{command.label}
			</button>
			<button
				class="command act redo"
				disabled={disabled || !redoable}
				data-command="redo"
				aria-label="redo"
				title={redoable ? 'run redo' : 'nothing to redo'}
				on:click={() => dispatch('run', ['redo'])}
			>
				↷
			</button>
		</span>
	{:else if command.press === 'help'}
		<!-- Never disabled: it runs nothing until it is asked about something. -->
		<button
			class="command help"
			class:armed={$helping}
			data-command="help"
			aria-pressed={$helping}
			title={$helping ? 'tap anything to have it explained; Help again for general help' : 'explain the next thing you tap'}
			on:click={arm}
		>
			<!-- `Help` alone sizes the button; armed, it is hidden and the
			     armed label is drawn over the same box, so arming it moves
			     nothing on the bar. -->
			<span class="label" class:hidden={$helping}>{command.label}</span>
			{#if $helping}<span class="label armed">? on what<small>(tap: all)</small></span>{/if}
		</button>
	{:else}
		<button
			class="command"
			class:act={runs(command)}
			{disabled}
			data-command={command.name}
			title={runs(command) ? `run ${command.name}` : `${command.name} …`}
			on:click={() => press(command)}
		>
			{command.label}
		</button>
	{/if}
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

	/* A button that does something the moment it is pressed, where the
	   rest only write a line, is drawn in a DOUBLE outline of the accent
	   (vividn, 09-22: «some sort of distinctive design feature, like a double
	   outline»). Inside the box, as a border, so the bar's scroll edge never
	   clips it and focus keeps the outline; the padding gives back the two
	   pixels the border takes, so a button does not grow when it arms. */
	.command.act {
		border: 3px double var(--accent);
		padding: calc(0.35rem - 2px) calc(0.85rem - 2px);
		color: var(--ink-bright);
	}

	.command:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	/* Help is outlined as Undo is but once, not twice (da-35f23): the
	   double outline says a press runs something at once, and this one
	   only waits for the next tap. Its padding is its idle label's, so the
	   armed label — no width of its own — has the whole box to be drawn
	   in, and the button is exactly as big as `Help` makes it. */
	.command.help {
		border-color: var(--accent);
		color: var(--ink-bright);
		display: inline-grid;
		padding: 0 2px;
	}

	.help .label {
		grid-area: 1 / 1;
		padding: 0.35rem calc(0.85rem - 2px);
	}

	.help .label.hidden {
		visibility: hidden;
	}

	.help .armed {
		width: 0;
		min-width: 100%;
		padding: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		font-size: 0.62rem;
		line-height: 1.1;
	}

	.help small {
		font-size: 0.5rem;
		opacity: 0.85;
	}

	/* Armed, Help is the one lit thing on the bar: filled with the accent,
	   so the page plainly is in a different state. */
	.command.help.armed {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--page);
	}

	/* Undo and its redo are one control of two buttons: they share the
	   seam between them, and the redo half is only as wide as its icon. */
	.undo {
		display: flex;
	}

	.undo > .command:first-child {
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
	}

	.undo > .redo {
		border-left: none;
		border-top-left-radius: 0;
		border-bottom-left-radius: 0;
		padding-left: calc(0.45rem - 2px);
		padding-right: calc(0.45rem - 2px);
	}

	.command:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
