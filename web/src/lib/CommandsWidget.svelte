<!--
	The commands widget — every command datum has, as a button (da-ltux).

	vividn: «Commands widget, which lists all commands. First one is Undo.»
	The order and what each press means are `#lib/commands.ts`; this is
	the bar that draws them.

	THE BASE ROW is eight (da-fg4w4, `BASE` in commands.ts): undo, write,
	read, fields, kit, db, sync, settings. Write, read, fields and settings
	are SUBMENUS (`MENUS`): a tap shows the submenu in the row's place, back
	first, and a command pressed there takes the bar back to the base row.

	The widget itself runs none of its presses — it composes and dispatches,
	and the page runs it through `run()` and echoes the executed argv,
	exactly as the fields widget does:

	- `undo` RUNS, at once. Undo never refuses on age any more (da-s6no:
	  with a way forward the fifteen-minute gate cost more than it saved),
	  so there is no second clock here — the widget runs `undo` and the
	  CLI's own warning past ten minutes is shown with the echo. Its REDO
	  is a small icon joined to it (da-zq91a), always drawn and lit only
	  when `redoable` says so — which is the CLI's answer and not this
	  widget's guess (`redo --dry-run`, asked in CommandPane.svelte).
	- a command the loaded pills COMPLETE runs at once too (da-3lxol):
	  `delete` with an hid selected deletes it. `fires` in commands.ts
	  decides, and the same answer (`kindOf`) draws the button with a DOUBLE
	  outline (`use:act`, #lib/act.ts), so what looks as if it fires is
	  what fires. So does `moment`, `start` or `end` under a FIELD pill
	  (da-lxqqz, `fieldVerb`) — unless the field has a key with no
	  default and the verb is not `end` (da-o4jaa), when the tap writes `moment <field> ` with the first key's
	  `key=` (da-31d1d) and opens the keyboard for the keys, and the pill
	  is spent onto the line.
	- `help`, in settings (da-vjwid), ARMS the page — `#lib/help.ts`: the
	  next tap on a command, a widget or a pill is explained instead of
	  acted on, and a second press on Help is `datum help`, and the answer
	  opens in a modal. Armed, it is filled and its name asks «on what?».
	- a VERB puts `<verb> ` on the command line and leaves the caret in
	  it. That is the whole of pending a verb: the fields widget reads the
	  line's first word, so tapping `moment` and typing `moment` reach the
	  same state through the same buffer.
	- anything else puts `<name> ` on the line too — a command with an
	  argument to finish, and nothing to filter. A field or hid pill it
	  takes goes into the line with it, in its slot, and is spent, when
	  words are TYPED next (da-8ru0m): `note coffee `, `update <hid> `,
	  the caret after them. `track` too (da-fg4w4): it writes its word, and
	  only `setup` opens the setup widget.
	- a command with SUBCOMMANDS (`db`, `kit`, …) opens them
	  instead (da-803mk): the bar is back and the subcommands, and a
	  tap on one fills `db migrate `. Back and Escape return a layer.
	  `db backup` and `db restore` open the «Backups» place instead
	  (da-9ow8m, da-ea5q2, #lib/backups.ts).
	- `sync` opens the sync PLACE (da-6du3o, `openPlace`): a modal on a
	  desktop, a tab beside this one on a phone (da-jozgv), rather than
	  drilling into its subcommands, which live in that widget's Advanced
	  section. While `datum conflicts --json` lists any (`conflicts`, from
	  the pane), it wears their count as a red badge and opens a submenu
	  instead: itself and `conflicts` (vividn: «The conflicts command should
	  not be top level, it should be under sync»), which opens its place
	  (da-6cutb) and is red with the badge. It blocks nothing, and goes
	  plain with the last one.
	- HOLDING a command with flags (da-st2xl, `#lib/hold.ts`) fills it as a
	  tap does, and the bar becomes back, Help and its flags
	  (`flagsOf`). A flag's tap appends it to the line; one that takes a
	  value (`-t`) opens the keyboard for it, one that does not (`-q`)
	  keeps it shut. Armed Help on a flag shows its help line.

	The loaded PILLS filter it (da-r13j): with any, the bar is every command
	whose grammar — `datum help --json` — takes every pill on it, flat, with
	no submenus (`commandsUnder`, vividn: «When a pill is loaded, there is
	still a filtering action which then only shows the available commands»).
	A selected hid leaves the id commands; a `-t` the ones that take a time;
	a field the verbs that field can take (da-57j14).

	The bar is `ButtonRows` — how every button bar is laid out and
	scrolled (da-vu7pi): as many rows as fit, up to three, each packed on
	its own and all of them scrolled along the bar's axis.

	Every command is a CIRCLE with its mark inside and its name beneath
	(da-fg4w4, vividn: «to visually distinguish them from other types of
	buttons»; `iconOf` in commands.ts). The circle IS the button, so its
	outline is the one that says what a tap does (#lib/act.ts), and the name
	hangs under it in room the button keeps below itself. A glyph is drawn by
	CSS, so the button's text is still its name. `track` keeps a color of
	its own (da-y6jfb). A flag is not a command, and stays a plain button.

	What the bar is SHOWING — the submenu, the command drilled into, the one
	held for its flags — outlives the widget (da-ria1a): on a phone a place
	opens as a root tab and unmounts the tab it left, and coming back must
	find the bar where the reader left it. So it is kept by `key`, the
	widget's own, in the session's `inProgress` (#lib/layout/session.ts),
	which goes with the tab or the widget as a session setting does; Back,
	Escape and pills that hide the command clear it as they always did, and
	a reload starts afresh.
-->

<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import type { Pill } from './bar.js';
	import type { FieldButton } from './fields.js';
	import ButtonRows from './ButtonRows.svelte';
	import { advance } from '#lib/keyframes.ts';
	import {
		argumentsOf,
		BACK,
		BASE,
		commandsUnder,
		drills,
		opensField,
		fieldVerb,
		fires,
		flagsOf,
		hasFlags,
		iconOf,
		MAX_ROWS,
		MENUS,
		subcommandsOf,
		type Axis,
		type CommandButton,
		type Grammar,
		type Menu,
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

	let {
		disabled = false,
		grammar = null,
		pills = [],
		/** The fields widget's buttons: what a field pill's verbs are read off. */
		fields = [],
		/** Would `redo` move anything? The CLI's own answer, from the pane. */
		redoable = false,
		/** How many entries have sync conflicts: the CLI's count, from the pane. */
		conflicts = 0,
		/** Whose bar this is: what it was showing is kept under it across a remount. */
		key = '',
		/** The axis the bar is reached along (ButtonRows). */
		axis = 'x',
		/** A press that runs: the argv. */
		onrun,
		/** A press that writes the line instead: what to write, and what it spends. */
		online,
		/** A flag from a held command's list (da-st2xl). */
		onflag
	}: {
		disabled?: boolean;
		grammar?: Grammar | null;
		pills?: Pill[];
		fields?: FieldButton[];
		redoable?: boolean;
		conflicts?: number;
		key?: string;
		axis?: Axis;
		onrun?: (argv: string[]) => void;
		online?: (line: { name: string; line: string; keyboard: boolean; spend?: Pill[]; held?: boolean }) => void;
		onflag?: (flag: { command: string; flag: string; keyboard: boolean }) => void;
	} = $props();

	/** The presses whose kind is theirs whatever is loaded. */
	const FIXED: Partial<Record<Press, ActKind>> = { run: 'run', help: 'arm', back: 'show', flag: 'fill', backups: 'open', menu: 'show' };

	const kept = untrack(() => inProgress.get(key));
	/** The submenu the bar shows in the base row's place, if any. */
	let menu = $state((kept?.menu as Menu | undefined) ?? null);
	/** The command whose subcommands the bar shows, if any. Gone with the command, if its pills hide it. */
	let open = $state((kept?.open as string | undefined) ?? null);
	/** The command whose flags the bar shows, if any (da-st2xl). The same. */
	let flagged = $state((kept?.flagged as string | undefined) ?? null);
	$effect.pre(() => {
		inProgress.set(key, { menu, open, flagged });
	});
	let bar: HTMLElement | undefined = $state();
	const listed = $derived(commandsUnder(grammar, pills, fields));
	$effect.pre(() => {
		if (open && !listed.some((command) => command.name === open)) open = null;
	});
	$effect.pre(() => {
		if (flagged && !listed.some((command) => command.name === flagged)) flagged = null;
	});
	const shown = $derived(
		flagged
			? flagsOf(grammar, flagged)
			: open
				? subcommandsOf(grammar, open)
				: argumentsOf(pills).length
					? listed
					: menu
						? [BACK, ...MENUS[menu]]
						: BASE
	);
	/**
	 * The lines the bar is dealt into, at most: the base row is ONE row in a
	 * pane, and over the bottom bar's output, where a line is a column,
	 * every column that fits; a flag is no circle, and keeps the three
	 * columns wide enough for it.
	 */
	const lines = $derived(flagged ? MAX_ROWS : axis === 'y' ? Infinity : shown === BASE ? 1 : MAX_ROWS);
	/** The base row's sync while there are conflicts: it opens the submenu that holds them. */
	const syncMenu = (command: CommandButton) => command.name === 'sync' && conflicts > 0 && shown === BASE;
	/** What a tap on `command` does: the press below and the outline are both this. */
	const kindOf = $derived((command: CommandButton): ActKind =>
		FIXED[command.press] ??
		(syncMenu(command)
			? 'show'
			: command.press === 'place' || opensField(command.name)
				? 'open'
				: !open && drills(grammar, command.name)
					? 'show'
					: fires(grammar?.get(command.name), pills) || fieldVerb(command.name, pills, fields)?.runs
						? 'run'
						: 'fill'));

	/**
	 * A hold fills the bare command and opens its flags — even where a tap
	 * under a field pill would run it or write the field too (da-lxqqz): the
	 * pill stays loaded and joins whatever the flags compose.
	 */
	function hold(el: HTMLElement): void {
		const name = el.dataset.command!;
		flagged = name;
		online?.({ name, line: `${name} `, keyboard: false, held: true });
	}

	onMount(() => holds(bar!, { within: 'button[data-holds]', run: hold }).destroy);

	/** Back and Escape peel one layer: the flags, else the subcommands, else the submenu. */
	function back(): void {
		if (flagged) flagged = null;
		else if (open) open = null;
		else menu = null;
	}

	function press(command: CommandButton): void {
		if (command.press === 'back') return back();
		if (command.press === 'menu') return void (menu = command.name as Menu);
		if (syncMenu(command)) return void (menu = 'sync');
		if (command.press === 'flag') return onflag?.({ command: flagged!, flag: command.name, keyboard: command.next === 'type' });
		if (kindOf(command) === 'show') return void (open = command.name);
		// A command chosen from a submenu takes the bar back to the base row, as a menu closes:
		// kept at once, since the choice can close the widget (the ❯'s commands) before an effect runs.
		if (!open && menu) {
			menu = null;
			inProgress.set(key, { menu, open, flagged });
		}
		if (command.press === 'help') arm();
		else if (command.press === 'backups') void openBackups(command.name === 'db restore' ? 'restore' : 'plans');
		else if (command.press === 'place') void openPlace(command.name);
		else if (kindOf(command) === 'run') onrun?.([command.name]);
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
			if (keyed) online?.({ name: command.name, line: advance(line, fields) ?? `${line} `, keyboard: true, spend: used });
			else online?.({ name: command.name, line: `${line} `, keyboard: command.next !== 'tap', spend: used });
		}
	}

	/** Escape is Back, and marks the key taken so the line does not peel as well. */
	function onKeyDown(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || !(menu || open || flagged) || event.defaultPrevented || document.querySelector('dialog[open]'))
			return;
		back();
		event.preventDefault();
	}
</script>

<svelte:window onkeydown={onKeyDown} />

<!-- What a circle holds: its line drawing, if its mark is one (a glyph is the button's `data-glyph`), and its name. -->
{#snippet face(icon: string, name: string)}
	{#if icon.length > 1}<svg viewBox="0 0 16 16" aria-hidden="true"><path d={icon} /></svg>{/if}<span class="name">{name}</span>
{/snippet}

<ButtonRows class="commands" items={shown} key={(command) => command.name} {axis} maxRows={lines} bind:bar>
	{#snippet probe()}
		<button class="command round" tabindex="-1">{@render face('', 'probe')}</button>
	{/snippet}
	{#snippet children({ item: command }: { item: CommandButton })}
		{@const icon = iconOf(command)}
		{@const glyph = icon.length === 1 ? icon : undefined}
		{#if command.name === 'undo'}
			<span class="undo">
				<button class="command round" use:act={'run'} {disabled} data-command="undo" data-glyph={glyph} aria-label="undo" title="run undo" onclick={() => press(command)}>
					{@render face(icon, command.label)}
				</button>
				<button
					class="command redo"
					use:act={'run'}
					disabled={disabled || !redoable}
					data-command="redo"
					aria-label="redo"
					title={redoable ? 'run redo' : 'nothing to redo'}
					onclick={() => onrun?.(['redo'])}
				>
					↷
				</button>
			</span>
		{:else if command.press === 'help'}
			<!-- Never disabled: it runs nothing until it is asked about something. -->
			<button
				class="command round help"
				use:act={'arm'}
				class:armed={$helping}
				data-command="help"
				data-glyph={glyph}
				aria-pressed={$helping}
				title={$helping ? 'tap anything to have it explained; Help again for general help' : 'explain the next thing you tap'}
				onclick={() => press(command)}
			>
				{@render face(icon, $helping ? 'on what?' : command.label)}
			</button>
		{:else if command.press === 'back'}
			<button class="command round" use:act={'show'} {disabled} data-back data-passes data-glyph={glyph} title="back" onclick={() => press(command)}>
				{@render face(icon, command.label)}
			</button>
		{:else if command.press === 'menu'}
			<button
				class="command round"
				use:act={'show'}
				{disabled}
				data-menu={command.name}
				data-passes
				title={`open ${command.name}`}
				onclick={() => press(command)}
			>
				{@render face(icon, command.label)}
			</button>
		{:else if command.press === 'flag'}
			<button
				class="command"
				use:act={'fill'}
				{disabled}
				data-flag={`${flagged} ${command.name}`}
				data-help={command.help}
				title={command.help}
				onclick={() => press(command)}
			>
				{command.label}
			</button>
		{:else}
			{@const clashing = command.name === 'conflicts' ? conflicts : 0}
			{@const badge = clashing || (syncMenu(command) ? conflicts : 0)}
			<button
				class="command round"
				use:act={kindOf(command)}
				class:opens={kindOf(command) === 'open'}
				class:clashing
				{disabled}
				data-command={command.name}
				data-glyph={glyph}
				data-holds={!open && hasFlags(grammar, command.name) ? '' : undefined}
				title={kindOf(command) === 'run' ? `run ${command.name}` : command.press === 'backups' ? 'open Backups' : kindOf(command) === 'open' ? `open ${command.label}` : `${command.name} …`}
				onclick={() => press(command)}
			>
				{@render face(icon, command.label)}{#if badge}<span class="badge">{badge}</span>{/if}
			</button>
		{/if}
	{/snippet}
</ButtonRows>

<style>
	/* The look of a plain one is `button.command`, shared with the Sync
	   widget (da-yoef7): app.css. The circle is the bar's own.

	   The button IS the circle (da-fg4w4), so its border is the outline
	   `use:act` decides, and its name hangs beneath it in the margin it
	   keeps below itself, which is what the rows are counted by (the
	   probe's margin box). Eight, with the gap between them, fit a 360px
	   phone's row; a long name spills into the gap rather than widen it. */
	.command.round {
		box-sizing: border-box;
		position: relative;
		inline-size: 2.1rem;
		block-size: 2.1rem;
		margin-block-end: 0.85rem;
		padding: 0;
		border-radius: 50%;
		display: grid;
		place-items: center;
		font-size: 0.95rem;
		line-height: 1;
	}

	/* Room at either end for a name wider than its circle (`conflicts`), so
	   one at an end spills into the bar's padding and nothing scrolls. */
	:global(.bar.commands) {
		padding-inline: 0.5rem;
	}

	/* In a pane the rows stand from the top, so a submenu takes the base
	   row's place rather than the one row sitting in the middle of the box. */
	:global(.bar.commands.x > .section) {
		align-self: start;
	}

	.command.round[data-glyph]::before {
		content: attr(data-glyph) / '';
	}

	.name {
		position: absolute;
		inset-block-start: calc(100% + 0.15rem);
		inset-inline-start: 50%;
		transform: translateX(-50%);
		font-size: 0.55rem;
		white-space: nowrap;
	}

	/* Placed from the padding box, which a 3px border starts 2px lower than a 1px one. */
	.command.round:global([data-act='run']) .name {
		inset-block-start: calc(100% + 0.15rem + 2px);
	}

	/* A tap that opens a modal wears its `›` (app.css) beside the circle,
	   in the gap, so the circle's mark stays alone in it. */
	.command.round::after {
		position: absolute;
		inset-inline-start: 100%;
		inset-block-start: 50%;
		transform: translateY(-50%);
		margin: 0;
		font-size: 0.6rem;
	}

	.command svg {
		inline-size: 1.05rem;
		block-size: 1.05rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

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

	/* Their count, on sync's circle and on conflicts': on its rim, not above
	   it, where a bar's top edge would cut it. */
	.badge {
		position: absolute;
		inset-block-start: -0.1rem;
		inset-inline-end: -0.35rem;
		min-inline-size: 0.85rem;
		padding: 0.1rem 0.15rem;
		box-sizing: border-box;
		border-radius: 0.5rem;
		background: var(--error);
		color: var(--page);
		font-size: 0.55rem;
		line-height: 1;
	}

	/* Track is where a field begins, so it is the one command in a colour
	   of its own (da-y6jfb): easy to find where it stands. */
	.command.command[data-command='track'] {
		border-color: var(--track);
		color: var(--track);
	}

	/* Help is outlined as Undo is but once, not twice (da-35f23): the
	   double outline says a press runs something at once, and this one
	   only waits for the next tap. */
	.command.help {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	/* Armed, Help is the one lit thing on the bar: filled with the accent,
	   so the page plainly is in a different state. Its name stays legible
	   under it, on the page. */
	.command.help.armed {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--page);
	}

	.command.help.armed .name {
		color: var(--accent);
	}

	/* Undo's redo is a small circle on undo's shoulder, so undo takes no
	   more of the row than any other command, and nothing above it. */
	.undo {
		position: relative;
		display: flex;
	}

	.command.redo {
		position: absolute;
		inset-block-start: -0.1rem;
		inset-inline-end: -0.45rem;
		box-sizing: border-box;
		inline-size: 1.15rem;
		block-size: 1.15rem;
		min-height: 0;
		padding: 0;
		border-radius: 50%;
		display: grid;
		place-items: center;
		font-size: 0.65rem;
		line-height: 1;
		z-index: 1;
	}
</style>
