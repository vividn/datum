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
	  decides, and the same answer (`kindOf`) draws the button with a DOUBLE
	  outline (`use:act`, $lib/act.ts), so what looks as if it fires is
	  what fires. So does `occur`, `start` or `end` under a FIELD pill
	  (da-lxqqz, `fieldVerb`) — unless the field has a key with no
	  default and the verb is not `end` (da-o4jaa), when the tap writes `occur <field> ` with the first key's
	  `key=` (da-31d1d) and opens the keyboard for the keys, and the pill
	  is spent onto the line.
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
	  argument to finish, and nothing to filter. A field or hid pill it
	  takes goes into the line with it, in its slot, and is spent, when
	  words are TYPED next (da-8ru0m): `note coffee `, `update <hid> `,
	  the caret after them.
	- a command with SUBCOMMANDS (`db`, `sync`, …) opens them
	  instead (da-803mk): the bar is `< Back` and the subcommands, and a
	  tap on one fills `db migrate `. `< Back` and Escape return to the list.
	  `db backup` and `db restore` open the «Backups» place instead
	  (da-9ow8m, da-ea5q2, $lib/backups.ts) — drawn, as Help is, in a
	  SINGLE outline of the accent: they open something, and run nothing.
	- `Sync` opens the sync PLACE (da-6du3o, `openPlace`): a modal on a
	  desktop, a tab beside this one on a phone (da-jozgv), rather than drilling into its nineteen subcommands,
	  which live in that widget's Advanced section. Outlined once, too.
	- `conflicts` opens its place the same way (da-6cutb), and is RED, with
	  their count, while `datum conflicts --json` lists any (`conflicts`,
	  from the pane): it blocks nothing, and goes plain with the last one.
	- HOLDING a command with flags (da-st2xl, `$lib/hold.ts`) fills it as a
	  tap does, and the bar becomes `< Back`, Help and its flags
	  (`flagsOf`). A flag's tap appends it to the line; one that takes a
	  value (`-t`) opens the keyboard for it, one that does not (`-q`)
	  keeps it shut. Armed Help on a flag shows its help line.

	The loaded PILLS filter it (da-r13j): a command whose grammar — `datum
	help --json` — cannot take every pill on the bar is not drawn
	(`commandsUnder`). A selected hid leaves the id commands; a `-t` the
	ones that take a time; a field the verbs that field can take
	(da-57j14: an occur field hides start, end and switch).

	The bar is `ButtonRows` — how every button bar is laid out and
	scrolled (da-vu7pi): as many rows as fit, up to three, each packed on
	its own and all of them scrolled left and right.
	The commands stand in SECTIONS (da-0mepr, `Section`): undo and help,
	logging, setup, edits, analysis, db — a divider between each, and a
	section the pills empty is not drawn at all. A drilled bar is drawn
	without them — its Help carries a section — so it is the one run it
	always was.

	A common command wears an ICON before its name (da-y6jfb, `ICONS` in
	commands.ts), no two alike, and `track` is drawn in a colour of its own
	— vividn: «I wouldn't put track first in the commands widget, it can stay
	more or less where it is, maybe if it has a nice color, it would be
	easy to find.» The icon is paid for out of the button's own padding, so
	a button is as wide as its word alone made it, and a glyph is drawn by
	CSS, so the button's text is still its word.

	What the bar is SHOWING — the command drilled into, the one held for its
	flags — outlives the widget (da-ria1a): on a phone a place opens as a
	root tab and unmounts the tab it left, and coming back must find the bar
	where the reader left it. So it is kept by `key`, the widget's own, in
	the session's `inProgress` ($lib/layout/session.ts), which goes with the
	tab or the widget as a session setting does; Back, Escape and pills that
	hide the command clear it as they always did, and a reload starts afresh.
-->

<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import type { Pill } from './bar.js';
	import type { FieldButton } from './fields.js';
	import ButtonRows from './ButtonRows.svelte';
	import SectionPages from './SectionPages.svelte';
	import { advance } from '$lib/keyframes.js';
	import {
		COLUMNS,
		commandsUnder,
		drills,
		opensField,
		fieldVerb,
		fires,
		flagsOf,
		hasFlags,
		ICONS,
		subcommandsOf,
		type CommandButton,
		type Grammar,
		type Press
	} from './commands.js';
	import { act, type ActKind } from './act.js';
	import { openBackups } from './backups.js';
	import { openPlace } from './layout/shell.js';
	import { arm, helping } from './help.js';
	import { holds } from './hold.js';
	import { inProgress } from './layout/session.js';
	import { withPills } from './selection.js';
	import { shellJoin } from './shellSplit.js';

	export let disabled = false;
	export let grammar: Grammar | null = null;
	export let pills: Pill[] = [];
	/** The fields widget's buttons: what a field pill's verbs are read off. */
	export let fields: FieldButton[] = [];
	/** Would `redo` move anything? The CLI's own answer, from the pane. */
	export let redoable = false;
	/** How many entries have sync conflicts: the CLI's count, from the pane. */
	export let conflicts = 0;
	/** Whose bar this is: what it was showing is kept under it across a remount. */
	export let key = '';
	/** The prototype (da-u79wl): sections that scroll, and «conflicts» pinned while there are any. */
	export let sections = false;

	/** The presses whose kind is theirs whatever is loaded. */
	const FIXED: Partial<Record<Press, ActKind>> = { run: 'run', help: 'arm', back: 'show', flag: 'fill', backups: 'open', place: 'open' };

	const dispatch = createEventDispatcher<{
		run: string[];
		line: { name: string; line: string; keyboard: boolean; spend?: Pill[]; held?: boolean };
		flag: { command: string; flag: string; keyboard: boolean };
	}>();

	/** The command whose subcommands the bar shows, if any. Gone with the command, if its pills hide it. */
	let open = (inProgress.get(key)?.open as string | undefined) ?? null;
	/** The command whose flags the bar shows, if any (da-st2xl). The same. */
	let flagged = (inProgress.get(key)?.flagged as string | undefined) ?? null;
	$: inProgress.set(key, { open, flagged });
	let bar: HTMLElement | undefined;
	/** Either bar: SectionPages takes ButtonRows' items, key, section and slot. */
	$: Bar = (sections ? SectionPages : ButtonRows) as typeof ButtonRows<CommandButton>;
	/** What SectionPages takes beyond them: its columns, and «conflicts» kept in sight while there are any. */
	$: paged = sections ? { columns: COLUMNS, pinned: (command: CommandButton) => conflicts > 0 && command.name === 'conflicts' } : {};
	$: listed = commandsUnder(grammar, pills, fields);
	$: if (open && !listed.some((command) => command.name === open)) open = null;
	$: if (flagged && !listed.some((command) => command.name === flagged)) flagged = null;
	$: shown = flagged ? flagsOf(grammar, flagged) : open ? subcommandsOf(grammar, open) : listed;
	/** What a tap on `command` does: the press below and the outline are both this. */
	$: kindOf = (command: CommandButton): ActKind =>
		FIXED[command.press] ??
		(opensField(command.name, pills)
			? 'open'
			: !open && drills(grammar, command.name, pills)
				? 'show'
				: fires(grammar?.get(command.name), pills) || fieldVerb(command.name, pills, fields)?.runs
					? 'run'
					: 'fill');

	/**
	 * A hold fills the bare command and opens its flags — even where a tap
	 * under a field pill would run it or write the field too (da-lxqqz): the
	 * pill stays loaded and joins whatever the flags compose.
	 */
	function hold(el: HTMLElement): void {
		const name = el.dataset.command!;
		flagged = name;
		dispatch('line', { name, line: `${name} `, keyboard: false, held: true });
	}

	onMount(() => holds(bar!, { within: 'button[data-holds]', run: hold }).destroy);

	/** Back and Escape peel one layer: the flags, else the subcommands. */
	function back(): void {
		if (flagged) flagged = null;
		else open = null;
	}

	function press(command: CommandButton): void {
		if (command.press === 'back') back();
		else if (command.press === 'backups') void openBackups(command.name === 'db restore' ? 'restore' : 'plans');
		else if (command.press === 'place') void openPlace(command.name);
		else if (command.press === 'flag') dispatch('flag', { command: flagged!, flag: command.name, keyboard: command.next === 'type' });
		else if (kindOf(command) === 'show') open = command.name;
		else if (kindOf(command) === 'run') dispatch('run', [command.name]);
		else {
			// Words TYPED next go after the field and hid pills, so those are
			// written into their slots first and spent (da-8ru0m): a pill left
			// loaded would lose its slot to the words. What a TAP completes
			// (`retime`, a verb) keeps them loaded for that tap; a verb under
			// a field it will not run is typed (da-lxqqz).
			const keyed = fieldVerb(command.name, pills, fields);
			const slotted = command.next === 'type' || keyed ? pills.filter((p) => p.kind === 'field' || p.kind === 'hid') : [];
			const { argv, used } = withPills(grammar, [command.name], slotted);
			// The command's own words stay bare (`db migrate` is two); what follows is quoted.
			const line = [argv[0], shellJoin(argv.slice(1))].join(' ').trimEnd();
			// Straight into the first key's frame, where a bare word fills it (da-31d1d, da-p3hxn).
			if (keyed) dispatch('line', { name: command.name, line: advance(line, fields) ?? `${line} `, keyboard: true, spend: used });
			else dispatch('line', { name: command.name, line: `${line} `, keyboard: command.next !== 'tap', spend: used });
		}
	}

	/** Escape is Back, and marks the key taken so the line does not peel as well. */
	function onKeyDown(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || !(open || flagged) || event.defaultPrevented || document.querySelector('dialog[open]'))
			return;
		back();
		event.preventDefault();
	}
</script>

<svelte:window on:keydown={onKeyDown} />

<svelte:component
	this={Bar}
	class="commands"
	items={shown}
	key={(command) => command.name}
	section={open || flagged ? undefined : (command) => command.section}
	{...paged}
	bind:bar
	let:item={command}
>
	<button slot="probe" class="command" tabindex="-1">probe</button>
	{#if command.name === 'undo'}
		<span class="undo">
			<button class="command" use:act={'run'} {disabled} data-command="undo" data-glyph={ICONS.undo} aria-label="undo" title="run undo" on:click={() => press(command)}>
				{command.label}
			</button>
			<button
				class="command redo"
				use:act={'run'}
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
			use:act={'arm'}
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
	{:else if command.press === 'back'}
		<button class="command" use:act={'show'} {disabled} data-back title="back to every command" on:click={() => press(command)}>
			{command.label}
		</button>
	{:else if command.press === 'flag'}
		<button
			class="command"
			use:act={'fill'}
			{disabled}
			data-flag={`${flagged} ${command.name}`}
			data-help={command.help}
			title={command.help}
			on:click={() => press(command)}
		>
			{command.label}
		</button>
	{:else}
		{@const icon = ICONS[command.name] ?? ''}
		{@const clashing = command.name === 'conflicts' ? conflicts : 0}
		<button
			class="command"
			use:act={kindOf(command)}
			class:opens={kindOf(command) === 'open'}
			class:drawn={icon.length > 1}
			class:clashing
			{disabled}
			data-command={command.name}
			data-glyph={icon.length === 1 ? icon : undefined}
			data-holds={!open && hasFlags(grammar, command.name) ? '' : undefined}
			title={kindOf(command) === 'run' ? `run ${command.name}` : command.press === 'backups' ? 'open Backups' : command.press === 'place' ? `open ${command.label}` : `${command.name} …`}
			on:click={() => press(command)}
		>
			{#if icon.length > 1}<svg viewBox="0 0 16 16" aria-hidden="true"><path d={icon} /></svg>{/if}{command.label}{clashing ? ` ${clashing}` : ''}
		</button>
	{/if}
</svelte:component>

<style>
	/* The look itself is `button.command`, shared with the Sync widget
	   (da-yoef7): app.css. What is the bar's own stays here. */

	/* Opens a place: once outlined, as Help is (below). */
	.command.opens {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	/* Conflicts to deal with (da-6cutb): red until the last is resolved. */
	/* `.command` twice, to outweigh `button.command:hover:not(:disabled)`
	   (app.css): Svelte 4's scope class did, and Svelte 5's adds less. */
	.command.command.clashing {
		border-color: var(--error);
		color: var(--error);
	}

	/* An icon'd button gives the icon's width back out of its padding
	   (`--pad`, app.css): the phone's one row holds the verbs it held
	   (da-shzs). `/ ''` keeps a glyph out of the button's name. */
	.command[data-glyph],
	.command.drawn {
		--pad: 0.45rem;
	}

	.command[data-glyph]::before {
		content: attr(data-glyph) / '';
		margin-right: 0.3em;
	}

	.command svg {
		inline-size: 1em;
		block-size: 1em;
		margin-right: 0.3em;
		vertical-align: -0.125em;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	/* Track is where a field begins, so it is the one command in a colour
	   of its own (da-y6jfb): easy to find where it stands. */
	.command.command[data-command='track'] {
		border-color: var(--track);
		color: var(--track);
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
</style>
