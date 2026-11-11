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
	exactly as the collect widget does:

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
	  what fires. So does `moment`, `on` or `off` under a FIELD pill
	  (da-lxqqz, `fieldCommand`) — unless the field has a key with no
	  default and the command is not `off` (da-o4jaa), when the tap writes `moment <field> ` with the first key's
	  `key=` (da-31d1d) and opens the keyboard for the keys, and the pill
	  is spent onto the line.
	- `help`, in settings (da-vjwid), ARMS the page — `#lib/help.ts`: the
	  next tap on a command, a widget or a pill is explained instead of
	  acted on, and a second press on Help is `datum help`, and the answer
	  opens in a modal. Armed, it is filled and its name reads «(click on something)», as the
	  header's (?) does (da-okj8i).
	- a LOG COMMAND puts `<command> ` on the command line and leaves the caret in
	  it. That is the whole of pending a command: the collect widget reads the
	  line's first word, so tapping `moment` and typing `moment` reach the
	  same state through the same buffer.
	- anything else puts `<name> ` on the line too — a command with an
	  argument to finish, and nothing to filter. A field or hid pill it
	  takes goes into the line with it, in its slot, and is spent, when
	  words are TYPED next (da-8ru0m): `remark coffee `, `update <hid> `,
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
	a field the log commands that field can take (da-57j14).

	A command the pills make READY leads the bar (da-h3jcu, `suggestionOf`
	in commands.ts): a larger circle before the rows, filled in its field's
	color (its state's, for a change) and named for what it runs, «moment
	coffee», «change → reading». A tap runs it. While something it needs is
	missing, a change's state or a key to type, it is dashed and a tap writes
	the line, the field and the pill's state on it, for the rest to be typed.

	The bar is `ButtonRows` — how every button bar is laid out and
	scrolled (da-vu7pi): as many rows as fit, up to three, each packed on
	its own and all of them scrolled along the bar's axis.

	ON A TAB WITH A COMMAND (da-zx826, `fixed` in #lib/bar.ts), where a line
	runs as `setup <line>`, the bar is `fixedCommands`, whatever pills are
	loaded or the bar kept: undo, Help, the command's own subcommands and
	flags, and the commands that open a page. No lead circle, no hold: nothing
	that would run as `setup <another command>`.

	WITH THE SETUP WIDGET ON SCREEN holding a field (da-r674m,
	`setupCommands` in #lib/fieldWidget.ts), its commands lead: the first,
	save or start tracking, is the larger circle, filled in the accent once it
	can run, the rest the first of the row, and undo is the form's own, its
	reason in its title while it is grayed. vividn, 2026-11-11: «the command
	pallete is how you should save stuff from the setup screen! save should be
	the primary action».

	THE PALETTE, the ❯'s commands over the bottom bar's output (da-qzzox,
	vividn, 2026-11-04: «increase the size of the buttons in the command
	pallette. It should be one row, horizontally scrollable»): `palette`
	draws larger circles in ONE row that never wraps, scrolled sideways,
	and the row declares `data-pans="x"`, so a sideways scroll in it is
	never a tab swipe.

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
		fieldCommand,
		fires,
		fixedCommands,
		flagsOf,
		hasFlags,
		ICONS,
		iconOf,
		MAX_ROWS,
		MENUS,
		subcommandsOf,
		suggestionOf,
		type Suggestion,
		type CommandButton,
		type Grammar,
		type Menu,
		type Press
	} from './commands.js';
	import { act, type ActKind } from './act.js';
	import type { SetupCommand } from './fieldWidget.js';
	import { openBackups } from './backups.js';
	import { openPlace } from './layout/shell.js';
	import { ARMED, arm, helping } from './help.js';
	import { holds } from './hold.js';
	import { inProgress } from './layout/session.js';
	import { withPills } from './selection.js';
	import { shellJoin } from './shellSplit.js';

	let {
		disabled = false,
		grammar = null,
		pills = [],
		/** The collect widget's buttons: what a field pill's log commands are read off. */
		fields = [],
		/** Would `redo` move anything? The CLI's own answer, from the pane. */
		redoable = false,
		/** How many entries have sync conflicts: the CLI's count, from the pane. */
		conflicts = 0,
		/** Whose bar this is: what it was showing is kept under it across a remount. */
		key = '',
		/** The ❯'s palette: larger circles in one row scrolled sideways (da-qzzox). */
		palette = false,
		/** The tab's command (da-zx826), or null: the bar is then `fixedCommands`, whatever is loaded or kept. */
		fixed = null,
		/** A press that runs: the argv. */
		onrun,
		/** A press that writes the line instead: what to write, and what it spends. */
		online,
		/** A flag from a held command's list (da-st2xl). */
		onflag,
		/** The ready command's tap (da-h3jcu): it runs. */
		onsuggest,
		/** The setup widget's commands, its primary first, and its undo (da-r674m); null with none on screen. */
		setup = null,
		/** One of them pressed. */
		onsetup,
		/** Those commands and undo alone: the setup place's modal, over the bar (da-r674m). */
		only = false
	}: {
		disabled?: boolean;
		grammar?: Grammar | null;
		pills?: Pill[];
		fields?: FieldButton[];
		redoable?: boolean;
		conflicts?: number;
		key?: string;
		palette?: boolean;
		fixed?: string | null;
		onrun?: (argv: string[]) => void;
		online?: (line: { name: string; line: string; keyboard: boolean; spend?: Pill[]; held?: boolean }) => void;
		onflag?: (flag: { command: string; flag: string; keyboard: boolean }) => void;
		onsuggest?: (suggestion: Suggestion) => void;
		setup?: { commands: SetupCommand[]; undo: SetupCommand } | null;
		onsetup?: (command: SetupCommand) => void;
		only?: boolean;
	} = $props();

	const suggested = $derived(fixed || setup ? null : suggestionOf(pills, fields));
	/** The setup widget's commands after its primary, first in the row, as buttons. */
	const setupRow = $derived((setup?.commands.slice(1) ?? []).map((c): CommandButton => ({ name: `setup: ${c.name}`, label: c.name, press: 'setup' })));
	const setupOf = (command: CommandButton) => setup?.commands.find((c) => c.name === command.label);

	/** The lead circle's tap: run what is ready, else write it on the line for the rest to be typed. */
	function lead(s: Suggestion): void {
		if (s.ready) return onsuggest?.(s);
		const { argv, used } = withPills(grammar, s.argv, pills.filter((p) => p.kind === 'field' || p.kind === 'state'));
		const line = [argv[0], shellJoin(argv.slice(1))].join(' ');
		online?.({ name: s.command, line: advance(line, fields) ?? `${line} `, keyboard: true, spend: used });
	}

	/** The presses whose kind is theirs whatever is loaded. */
	const FIXED: Partial<Record<Press, ActKind>> = { run: 'run', help: 'arm', back: 'show', flag: 'fill', backups: 'open', menu: 'show', setup: 'run' };

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
		fixed
			? fixedCommands(grammar, fixed)
			: flagged
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
	 * The lines the bar is dealt into, at most: the palette is ONE row
	 * whatever it shows, and in a pane the base row is one and the rest as
	 * many as fit, up to three.
	 */
	const lines = $derived(palette || (shown === BASE && !flagged) ? 1 : MAX_ROWS);
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
					: fires(grammar?.get(command.name), pills) || fieldCommand(command.name, pills, fields)?.runs
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
		if (command.press === 'flag') return onflag?.({ command: fixed ?? flagged!, flag: command.name, keyboard: command.next === 'type' });
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
			// (`retime`, a log command) keeps them loaded for that tap; a log command under
			// a field it will not run is typed (da-lxqqz).
			const keyed = fieldCommand(command.name, pills, fields);
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

{#if setup}
	{@const primary = setup.commands[0]}
	<button
		class="lead"
		class:palette
		class:ready={primary.enabled}
		use:act={'run'}
		disabled={disabled || !primary.enabled}
		style:--lead="var(--accent)"
		data-setup={primary.name}
		data-primary
		data-glyph={primary.glyph}
		title={primary.enabled ? primary.name : primary.why}
		onclick={() => onsetup?.(primary)}
	>
		{@render face(primary.glyph, primary.name)}
	</button>
{:else if suggested}
	{@const s = suggested}
	{@const icon = ICONS[s.command]}
	<button
		class="lead"
		class:palette
		class:ready={s.ready}
		use:act={s.ready ? 'run' : 'fill'}
		{disabled}
		style:--lead={s.color}
		data-suggested={s.command}
		data-ready={s.ready}
		data-glyph={icon.length > 1 ? undefined : icon}
		title={`${s.ready ? 'run ' : ''}${[s.command, s.field, s.state].filter(Boolean).join(' ')}${s.ready ? '' : ' …'}`}
		onclick={() => lead(s)}
	>
		{@render face(icon, s.label)}
	</button>
{/if}

<ButtonRows
	class="commands{palette ? ' palette' : ''}"
	items={setup ? [...setupRow, ...(only ? [BASE[0]] : shown)] : shown}
	key={(command) => command.name}
	maxRows={lines}
	pans={palette ? 'x' : undefined}
	bind:bar
>
	{#snippet probe()}
		<button class="command round" tabindex="-1">{@render face('', 'probe')}</button>
	{/snippet}
	{#snippet children({ item: command }: { item: CommandButton })}
		{@const icon = iconOf(command)}
		{@const glyph = icon.length === 1 ? icon : undefined}
		{#if command.press === 'setup'}
			{@const c = setupOf(command)}
			<!-- Gone with the widget while its row still stands, it draws nothing. -->
			{#if c}<button
				class="command round"
				use:act={'run'}
				disabled={disabled || !c.enabled}
				data-setup={c.name}
				data-glyph={c.glyph}
				title={c.enabled ? c.name : c.why}
				onclick={() => onsetup?.(c)}
			>
				{@render face(c.glyph, c.name)}
			</button>{/if}
		{:else if command.name === 'undo'}
			{@const local = setup?.undo}
			<span class="undo">
				<button
					class="command round"
					use:act={'run'}
					disabled={disabled || (local && !local.enabled)}
					data-command="undo"
					data-setup={local ? 'undo' : undefined}
					data-glyph={glyph}
					aria-label="undo"
					title={!local ? 'run undo' : local.enabled ? (local.does === 'local' ? 'undo the last edit in the form' : 'undo the save') : local.why}
					onclick={() => (local ? onsetup?.(local) : press(command))}
				>
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
				{@render face(icon, $helping ? ARMED : command.label)}
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
				data-flag={`${fixed ?? flagged} ${command.name}`}
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
				data-holds={!fixed && !open && hasFlags(grammar, command.name) ? '' : undefined}
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

	/* The palette's are larger (da-qzzox): the output's height holds one
	   row of them, names included, and the row scrolls sideways. */
	:global(.bar.commands.palette) .command.round {
		inline-size: 2.9rem;
		block-size: 2.9rem;
		margin-block-end: 1rem;
		font-size: 1.25rem;
	}

	:global(.bar.commands.palette) .command svg {
		inline-size: 1.4rem;
		block-size: 1.4rem;
	}

	:global(.bar.commands.palette) .name {
		font-size: 0.68rem;
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

	/* The ready command (da-h3jcu): a larger circle before the rows, its
	   name beneath, filled in its field's color once it can run and dashed
	   in it while it cannot. The color is data, so it is set inline. */
	.lead {
		box-sizing: border-box;
		position: relative;
		flex: none;
		align-self: start;
		inline-size: 2.6rem;
		block-size: 2.6rem;
		margin: 0 0.9rem 0.95rem 0.5rem;
		padding: 0;
		border-radius: 50%;
		border: 2px dashed var(--lead);
		background: transparent;
		color: var(--lead);
		display: grid;
		place-items: center;
		font-size: 1.3rem;
		line-height: 1;
		cursor: pointer;
	}

	.lead.palette {
		inline-size: 3.3rem;
		block-size: 3.3rem;
		font-size: 1.6rem;
	}

	.lead.palette .name {
		font-size: 0.7rem;
	}

	/* Ready, it runs at a tap, so it wears the double outline every such
	   button does (#lib/act.ts): a ring of the page, then one of its color. */
	.lead.ready {
		border-style: solid;
		background: var(--lead);
		color: var(--page);
		box-shadow:
			0 0 0 2px var(--page),
			0 0 0 3.5px var(--lead);
	}

	.lead[data-glyph]::before {
		content: attr(data-glyph) / '';
	}

	.lead svg {
		inline-size: 1.2rem;
		block-size: 1.2rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.lead .name {
		color: var(--ink-bright);
		font-size: 0.6rem;
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
