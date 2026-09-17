<!--
	The fields widget — every field `datum fields` lists, and the primary
	way anything gets logged without typing (da-nz29, da-c7u8).

	Track + comes first: the way into the whole app, so it is there on an
	empty database too. Then every field that is not ignored (`ignored` in
	`datum fields --json`, da-d08c), spec or no spec, in the CLI's
	alphabetical order, each shaped by its kind and painted by what it is
	now (`lookOf` in fields.ts): a point is a filled pill; a block is a
	rectangle, filled while it is on; a switch is a rectangle filled with
	its current state's colour; a keyed record is its name, underlined, and
	a click prefills `add <field> ` in the bottom bar rather than running
	anything. The pane keeps the height its layout gives it and the BAR is
	the scroller (da-mbz2) — one row reached left and right while the pane
	is one or two button-rows high, wrapped and reached up and down at
	three or more (da-ltux) — while its panels, which hang upward out of
	it, are hung against the screen so that scroller cannot clip them.
	Ignored fields get no button: the Track modal lists them, and a press
	runs `track <field>`.

	A PENDING VERB (`verb`, da-ltux) is the commands widget talking to this
	one through the command line they share: the list becomes the fields
	that verb can be spent on, a press MEANS that verb and runs at once,
	and Track + is the `+` at the end of the list instead of the button
	before its start — the one door on the modal that PRESETS a verb, which
	is what `startTracking`'s note below leaves room for. Which fields, and
	what a press then runs, are `fieldsUnder` and `verbPress` in commands.ts.

	Track + opens that modal (da-94q1, $lib/track.ts) — a `<dialog>` over
	the whole page with a pictogram per verb — rather than a form hanging
	off the bar. The widget still runs nothing itself: the modal answers
	with an argv and it goes out the same `run` as a press.

	Press to log. Press and hold (or right-click, or the menu key) to open
	the `-t` / `dur` bubbles first — and `note`, which fills the bar with
	`note <field> ` and runs nothing (da-py5w): the CLI routes the bare
	words that follow into the comment, so the person just types.

	A multi-state field asks WHICH, so its button reads `name:` and a press
	fills the bar with that field's states in place of the fields — `< Back`
	first, `+` last (da-g635). Nothing floats: the list is the widget's own
	content, in the same scroller and under the same height rules, and the
	pane is scrolled back to where it stood when Back returns the fields. A
	press on a state runs `switch <field> <state>` — plainly, with no `-t`,
	because the list carries no bubbles — and comes back; `+` is an input,
	and Enter on it runs the same command with a state the field has never
	been in, which is how one is learned. A LONG press still opens the
	panel, states and all, so the `-t` / `dur` bubbles can be set first.

	The widget never runs anything itself: it composes an argv and
	dispatches it, and the page runs it through `run()` and echoes the
	result's own argv into the REPL. That is the teach loop, and it only
	teaches the truth because the echoed argv IS the executed argv — there
	is no second string built for display that could drift from it.
-->
<script lang="ts">
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import {
		lookOf,
		pressArgv,
		stateLook,
		type Adjust,
		type Look,
		type SwitchLabel,
		type FieldButton,
		type FieldShape
	} from './fields.js';
	import { openTrack } from './trackModal.js';
	import {
		declaredVerb,
		fieldsUnder,
		scrollsSideways,
		verbPress,
		watchRows,
		type Verb
	} from './commands.js';

	export let fields: FieldButton[] = [];
	export let disabled = false;
	/** How a switch in a state is labelled; see `SwitchLabel`. */
	export let switchLabel: SwitchLabel = 'field';
	/**
	 * The verb the command line is pending (da-ltux), or null.
	 *
	 * It filters the list to the fields that verb can be spent on and
	 * makes a press MEAN that verb — so `start` on a switch field with
	 * `true` among its states runs `start <field>`, not the state list a
	 * plain press would open. The `+` at the end is then the way to a
	 * field that does not exist yet, with the verb already chosen.
	 */
	export let verb: Verb | null = null;

	const dispatch = createEventDispatcher<{ run: string[]; line: string }>();

	/** How long a press has to last before it counts as a long press. */
	const HOLD_MS = 450;

	/** The `-t` bubbles: minutes back from now. */
	const WHEN = [0, 5, 15, 30, 60];
	/** The `dur` bubbles, as typed. The empty string is "no duration". */
	const FOR = ['', '5m', '15m', '30m', '1h'];

	let bar: HTMLElement;
	/** The measuring stick the scroll rule counts rows with (da-ltux). */
	let probe: HTMLElement;
	let rows = 3;

	onMount(() => watchRows(bar, probe, (n) => (rows = n)));

	$: sideways = scrollsSideways(rows);
	/**
	 * The field whose long-press PANEL is open, by name — never the button
	 * object, for the same reason `picked` is not (da-0n3k): `buttons` is
	 * rebuilt from the fields store on every beat, so an `open === button`
	 * identity check goes false the next time anything runs, and the panel
	 * a reader is still setting bubbles in vanishes under them.
	 */
	let open: string | null = null;
	let adjust: Adjust = {};

	/**
	 * The field whose STATES have taken the bar over, by name (da-g635).
	 *
	 * By name and not by the button, because the entries are rebuilt on
	 * every beat: held by name, the list redraws from the field as it is
	 * now — the state just logged marked — rather than from a copy that
	 * went stale the moment it was picked.
	 */
	let picked: string | null = null;
	/** Whether the `+` at the end of that list has become its input. */
	let addingState = false;
	let newState = '';
	/** The pane's scroller and where it stood when the states took over. */
	let scrolled: HTMLElement | null = null;
	/**
	 * Where the scroller stood when the states took over — BOTH axes, since
	 * the scroll rule decides which one the bar is on (da-ltux) and a short
	 * pane is scrolled sideways.
	 */
	let scrolledTo = { left: 0, top: 0 };

	let holdTimer: ReturnType<typeof setTimeout> | null = null;
	let held = false;

	$: buttons = fieldsUnder(
		fields.filter((w) => !w.ignored),
		verb
	);
	$: ignored = fields.filter((w) => w.ignored);
	$: picking = picked === null ? null : (buttons.find((b) => b.field === picked) ?? null);

	/** A look as the custom properties every button below is painted from. */
	function vars(look: Look) {
		return `--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}`;
	}

	function statesOf(shape: FieldShape): string[] {
		return shape.kind === 'states' ? shape.states : [];
	}

	/**
	 * The panel a LONG press opens: the `-t` / `dur` bubbles, and for a
	 * multi-state field the states to spend them on. A plain press never
	 * opens it — that fills the bar with the states instead (`pick`) — so
	 * the bubbles are the whole reason it exists and are always in it.
	 */
	function panel(button: FieldButton) {
		adjust = {};
		open = button.field;
	}

	function close() {
		open = null;
	}

	/**
	 * Leave the state list for the field list, with the pane where it was.
	 *
	 * The states are fewer than the fields, so the shorter list scrolls the
	 * pane's own scroller back up under the reader — where they were is
	 * remembered by `pick` and given back here, after the list is drawn.
	 */
	function back() {
		if (picked === null) return;
		picked = null;
		addingState = false;
		newState = '';
		const scroller = scrolled;
		const at = scrolledTo;
		scrolled = null;
		if (scroller)
			void tick().then(() => {
				scroller.scrollLeft = at.left;
				scroller.scrollTop = at.top;
			});
	}

	function emit(argv: string[]) {
		close();
		back();
		dispatch('run', argv);
	}

	/**
	 * `note` in the panel: the bar gets `note <field> `, focused, and
	 * nothing runs (da-py5w). The comment is prose — only the person can
	 * type it — and the CLI's `note` joins the bare words that follow into
	 * `extra_data.comment` on its own, so no flag goes in the line.
	 */
	function note(button: FieldButton) {
		line(`note ${button.field} `);
	}

	/** Put a line in the command line, unrun, and leave the lists as they were. */
	function line(text: string) {
		close();
		back();
		dispatch('line', text);
	}

	/**
	 * Ask «which verb?» and run the answer (da-94q1).
	 *
	 * The one door the widget has on the Track modal, so the set-up
	 * walkthrough has a single place to take it over. Nothing is preset
	 * from here: the doors that DO preset one — a filtered list's `+`, the
	 * CLI's own refusal — call `openTrack` themselves. A dismissed modal
	 * answers null and nothing runs.
	 */
	async function startTracking() {
		close();
		const argv = await openTrack({ ignored: ignored.map((w) => w.field) });
		if (argv) emit(argv);
	}

	/**
	 * Show `button`'s states in the bar, in place of the fields (da-g635).
	 *
	 * Nothing floats: the list is REPLACED inside the widget's own box, so
	 * the pane keeps the height its layout gives it (da-mbz2) and the
	 * states are reached by the same scroll the fields are.
	 */
	function pick(button: FieldButton) {
		close();
		// The in-place list shows no bubbles, so it must carry none: a `-t`
		// set in the panel and then dismissed would otherwise still be on
		// the next plain press, and the state would be logged fifteen
		// minutes ago with nothing on screen saying so (da-vlc0). `panel()`
		// resets it for its own opening; this is the other door in.
		adjust = {};
		scrolled = scrollerOf();
		scrolledTo = { left: scrolled?.scrollLeft ?? 0, top: scrolled?.scrollTop ?? 0 };
		picked = button.field;
	}

	/** What scrolls, either way: the bar itself, or an ancestor that took the overflow. */
	function scrollerOf(): HTMLElement | null {
		for (let node: HTMLElement | null = bar; node; node = node.parentElement) {
			if (node.scrollHeight > node.clientHeight || node.scrollWidth > node.clientWidth) return node;
		}
		return null;
	}

	function press(button: FieldButton) {
		// With a verb pending, the press IS that verb (C1: a field tap runs
		// at once) — except where `<verb> <field>` is not a command, and
		// `verbPress` hands back null so the two courtesies below apply.
		const asked = verb === null ? null : verbPress(verb, button);
		if (asked) return emit(asked);
		// A record is typed, never pressed: its line goes to the prompt.
		if (button.shape.kind === 'record') return line(`add ${button.field} `);
		// A multi-state field can't be logged without saying which, so a
		// plain press asks — the bar fills with its states.
		if (button.shape.kind === 'states') pick(button);
		else emit(pressArgv(button));
	}

	/**
	 * The `+` at the end of a filtered list: the same modal, with the
	 * pending verb already chosen (da-ltux).
	 *
	 * One of the doors `startTracking` above leaves the presetting to. The
	 * verb is mapped to one a field can be DECLARED with — `end` is not, so
	 * it declares the block it would have ended (`declaredVerb`).
	 */
	async function trackUnder(pending: Verb) {
		close();
		const argv = await openTrack({
			verb: declaredVerb(pending),
			ignored: ignored.map((w) => w.field)
		});
		if (argv) emit(argv);
	}

	/**
	 * Enter in the `+`'s input. A state is learned from the log, so
	 * `switch` is the whole of adding one: it is in the list next time.
	 * Empty cancels, as Escape does.
	 */
	function addState(button: FieldButton) {
		const state = newState.trim();
		if (state === '') {
			addingState = false;
			newState = '';
			return;
		}
		emit(pressArgv(button, adjust, state));
	}

	function clearHold() {
		if (holdTimer !== null) clearTimeout(holdTimer);
		holdTimer = null;
	}

	// Pointer events rather than touch/mouse pairs: one code path reaches
	// a finger, a stylus and a held mouse button alike.
	function down(button: FieldButton) {
		held = false;
		clearHold();
		holdTimer = setTimeout(() => {
			holdTimer = null;
			held = true;
			panel(button);
		}, HOLD_MS);
	}

	// The click still fires after a long press (and after the keyboard's
	// Enter/Space, which never sends a pointerdown at all) — swallow the one
	// that follows a hold, and treat everything else as a short press.
	function click(button: FieldButton) {
		if (held) {
			held = false;
			return;
		}
		if (open === button.field) close();
		else press(button);
	}

	function onWindowPointerDown(event: PointerEvent) {
		if (bar && !bar.contains(event.target as Node)) close();
	}

	function onWindowKeyDown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		// Innermost first: the panel a long press opened, then the input the
		// `+` became, then the state list itself — Escape is its Back. The
		// modal has its own Escape, and the dialog swallows it.
		if (open) close();
		else if (addingState) {
			addingState = false;
			newState = '';
		} else back();
	}

	/**
	 * Hang an open panel above the slot it belongs to, against the screen.
	 *
	 * The bar scrolls inside its pane (da-mbz2), and a scroller clips
	 * whatever its children hang outside it — so a panel is `fixed` and
	 * placed from its slot's box rather than laid out against it. It still
	 * opens upward (da-hkpd: below the bar is off the bottom of the
	 * screen), and never past the top edge: a panel taller than the room
	 * above its slot scrolls inside itself.
	 */
	function hang(node: HTMLElement) {
		const slot = node.parentElement;
		const place = () => {
			if (!slot) return;
			const box = slot.getBoundingClientRect();
			node.style.setProperty('--left', `${box.left}px`);
			node.style.setProperty('--above', `${box.top}px`);
			node.style.setProperty('--screen', `${document.documentElement.clientHeight}px`);
		};
		place();
		// Capturing: the scroll that moves the slot is the PANE's, not the
		// window's, and a scroll event does not bubble.
		window.addEventListener('scroll', place, true);
		window.addEventListener('resize', place);
		return {
			destroy() {
				window.removeEventListener('scroll', place, true);
				window.removeEventListener('resize', place);
			}
		};
	}
</script>

<svelte:window on:pointerdown={onWindowPointerDown} on:keydown={onWindowKeyDown} />

<!-- The measuring stick for the scroll rule: a button of the ordinary
     kind, laid out and invisible, so the row height is the buttons' own.
     Outside the bar and out of flow, so nothing that reads the bar's
     buttons finds it. -->
<span class="probe" aria-hidden="true"><button class="widget" bind:this={probe} tabindex="-1">probe</button></span>

<div class="bar" class:sideways bind:this={bar} data-rows={rows} style:--rows={rows}>
	{#if picking}
		{@const chosen = picking}
		<!-- The states, in place of the fields (da-g635): `< Back` first —
		     it logs nothing — then one button per state in the CLI's order,
		     each painted as that state is, and `+` last, which is how a
		     state that has never been logged gets into the list at all. -->
		<div class="slot">
			<button class="nav back" data-back {disabled} title="back to the fields" on:click={back}>
				&lt; Back
			</button>
		</div>

		{#each statesOf(chosen.shape) as state (state)}
			{@const look = stateLook(chosen, state)}
			<div class="slot">
				<button
					class="widget states"
					class:on={look.fill !== null}
					style={vars(look)}
					{disabled}
					data-state={state}
					title={pressArgv(chosen, adjust, state).join(' ')}
					on:click={() => emit(pressArgv(chosen, adjust, state))}
				>
					{state}
				</button>
			</div>
		{/each}

		<div class="slot">
			{#if addingState}
				<form on:submit|preventDefault={() => addState(chosen)}>
					<!-- svelte-ignore a11y-autofocus -->
					<input
						class="new-state"
						bind:value={newState}
						placeholder="state"
						aria-label="a new state for {chosen.field}"
						spellcheck="false"
						autofocus
					/>
				</form>
			{:else}
				<button
					class="nav add-state"
					data-add-state
					{disabled}
					title="add a state to {chosen.field}"
					on:click={() => (addingState = true)}
				>
					+
				</button>
			{/if}
		</div>
	{:else}
		<!-- Track + leads the whole list — the way into the app on an empty
		     database. Under a pending verb it is the `+` at the END instead
		     (vividn: «There should be a + field at the end of this filtered
		     fields view for adding new fields»), so the filtered list is the
		     fields that verb can be spent on and then the way to one more. -->
		<div class="slot" class:last={verb !== null}>
			{#if verb === null}
				<button
					class="track add-button"
					{disabled}
					title="track a new field"
					on:click={() => void startTracking()}
				>
					Track +
				</button>
			{:else}
				<button
					class="nav add-field"
					data-add-field
					{disabled}
					title="track a new {verb} field"
					on:click={() => void trackUnder(verb)}
				>
					+
				</button>
			{/if}
		</div>

		{#if buttons.length === 0}
			<!-- No direction word here on purpose: this hint said "below" while the
			     bar was at the top of the page (da-nz29), and da-1x72 moved the
			     bar to the bottom without the copy following. Name the thing, not
			     where it is. -->
			<p class="hint empty">nothing tracked yet — or type a command to log something</p>
		{/if}

		{#each buttons as button (button.field)}
			{@const look = lookOf(button, switchLabel)}
			<div class="slot">
				{#if button.shape.kind === 'record'}
					<!-- Not a button: nothing runs. The line goes to the bar's prompt. -->
					<button
						class="record"
						role="link"
						data-field={button.field}
						style={vars(look)}
						{disabled}
						title="add {button.field} … — fills the prompt"
						on:click={() => press(button)}>{button.field}</button
					>
				{:else}
					<button
						class="widget {button.shape.kind}"
						class:on={look.fill !== null}
						class:active={open === button.field}
						style={vars(look)}
						{disabled}
						data-field={button.field}
						aria-expanded={open === button.field}
						title="press to log · hold for when / how long"
						on:pointerdown={() => down(button)}
						on:pointerup={clearHold}
						on:pointerleave={clearHold}
						on:pointercancel={clearHold}
						on:contextmenu|preventDefault={() => panel(button)}
						on:click={() => click(button)}
					>{look.label}{button.shape.kind === 'states' ? ':' : ''}</button>
				{/if}

				{#if open === button.field}
					<div class="panel" use:hang>
						<div class="bubbles">
							<span class="bubble-label">when</span>
							{#each WHEN as minutes (minutes)}
								<button
									class:on={(adjust.minutesAgo ?? 0) === minutes}
									on:click={() => (adjust = { ...adjust, minutesAgo: minutes })}
								>
									{minutes === 0 ? 'now' : `-${minutes}m`}
								</button>
							{/each}
						</div>
						<div class="bubbles">
							<span class="bubble-label">for</span>
							{#each FOR as value (value)}
								<button
									class:on={(adjust.dur ?? '') === value}
									on:click={() => (adjust = { ...adjust, dur: value || undefined })}
								>
									{value || '—'}
								</button>
							{/each}
						</div>

						<button class="note" on:click={() => note(button)}>note</button>

						{#if button.shape.kind === 'states'}
							{#if statesOf(button.shape).length === 0}
								<p class="hint">
									no states yet — <code>switch {button.field} &lt;state&gt;</code> in the REPL
								</p>
							{:else}
								<div class="states">
									{#each statesOf(button.shape) as state (state)}
										<button
											title={pressArgv(button, adjust, state).join(' ')}
											on:click={() => emit(pressArgv(button, adjust, state))}
										>
											{state}
										</button>
									{/each}
								</div>
							{/if}
						{:else}
							<button class="commit" on:click={() => emit(pressArgv(button, adjust))}>log</button>
						{/if}

						<code class="preview"
							>{pressArgv(
								button,
								adjust,
								button.shape.kind === 'states' ? '<state>' : undefined
							).join(' ')}</code
						>
					</div>
				{/if}
			</div>
		{/each}
	{/if}
</div>

<style>
	/*
	 * The bar is the SCROLLER, and the scroll rule decides its axis
	 * (da-ltux, vividn): one or two button-rows high and it is one row
	 * reached left and right; three or more and it wraps and is reached up
	 * and down. Its panels hang against the screen (`hang` above), so
	 * taking the overflow here cannot clip them.
	 */
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

	/* Sideways: as many rows as fit, flowing by column, reached left and
	   right — the commands bar's rule and its grid, for its reasons
	   (CommandsWidget.svelte, da-hs6c). The rows share the box equally
	   rather than sizing to their content, so `Track +`, taller than the
	   chip the rows were counted with, cannot push the second row out of
	   the box. `order` (`.slot.last`) is honoured by the grid as by flex. */
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

	.slot {
		flex: none;
	}

	/* Under a pending verb the Track slot is the `+` at the END of the
	   filtered list. `order` rather than a second block of markup: the
	   button it holds is the same door on the same modal. */
	.slot.last {
		order: 1;
	}

	.probe {
		position: absolute;
		visibility: hidden;
		pointer-events: none;
	}

	.widget,
	.track,
	.record,
	.nav {
		/* A held press must not also select the label or raise the platform's
		   text callout, on touch least of all. */
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
		touch-action: manipulation;
		min-height: 24px;
		font: inherit;
		cursor: pointer;
	}

	/* Shape by kind, fill by state (lookOf in fields.ts): the colours
	   arrive as --fill / --line / --text. */
	.widget {
		background: var(--fill);
		border: 1.5px solid var(--line);
		border-radius: 3px;
		padding: 0.35rem 0.85rem;
		color: var(--text);
		font-size: 0.85rem;
	}

	.widget.point {
		border-radius: 999px;
	}

	.widget:hover:not(:disabled) {
		filter: brightness(1.2);
	}

	.widget.active {
		box-shadow: 0 0 0 2px var(--ink-bright);
	}

	.widget:disabled,
	.track:disabled,
	.record:disabled,
	.nav:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* `< Back` and `+`: the two ends of the state list, and neither of them
	   a state — so they are drawn in the app's own ink rather than in any
	   field's colour, and cannot be mistaken for something to log. */
	.nav {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 3px;
		padding: 0.35rem 0.85rem;
		color: var(--ink);
		font-size: 0.85rem;
	}

	.nav:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.new-state {
		width: 7rem;
	}

	/* The first thing a person meets: larger than the fields, and filled. */
	.track {
		background: var(--accent);
		border: none;
		border-radius: 0.5rem;
		padding: 0.45rem 1.1rem;
		color: var(--page);
		font-size: 1rem;
		font-weight: 700;
		letter-spacing: 0.02em;
		box-shadow: 0 0 12px color-mix(in srgb, var(--accent) 40%, transparent);
	}

	/* Not `--accent-bright`: this is the accent as a FILL under a pointer,
	   which lifts less than the accent as ink does. */
	.track:hover:not(:disabled) {
		background: #9a9aff;
	}

	.record {
		background: none;
		border: none;
		padding: 0.35rem 0.3rem;
		color: var(--ink);
		font-size: 0.85rem;
		text-decoration: underline 2px var(--line);
		text-underline-offset: 0.3em;
	}

	.untracked {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--muted);
		font-size: 0.8rem;
	}

	.track-this {
		background: none;
		/* Not `--rule`: a dashed outline reads lighter than a solid one, so
		   it is drawn a step up to weigh the same. */
		border: 1px dashed #3a3a6a;
		border-radius: 3px;
		padding: 0 0.35rem;
		color: var(--accent);
		font-size: 0.7rem;
	}

	.track-this:hover:not(:disabled) {
		border-color: var(--accent);
	}

	.panel {
		/* Fixed, and placed by `hang` above: the bar's own pane scrolls,
		   and a panel laid out against its slot would be clipped by that
		   scroller. Upward, because the bar is the bottom-most row of the
		   frame at every width, so a panel hung below it opens off the
		   bottom of the screen — reachable only by a scroll the reader has
		   no reason to guess at (da-hkpd). */
		position: fixed;
		z-index: 10;
		left: var(--left);
		bottom: calc(var(--screen) - var(--above) + 0.3rem);
		max-height: calc(var(--above) - 0.6rem);
		overflow: auto;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		min-width: max-content;
		/* Not `--ground`: a panel floats ABOVE the bar, and a step up from
		   the ground it covers is how it says so. */
		background: #12122a;
		border: 1px solid var(--rule);
		border-radius: 6px;
		padding: 0.5rem;
		/* A shadow is black at an opacity, not a colour from the palette. */
		box-shadow: 0 4px 16px #00000080;
	}

	.bubbles,
	.states {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem;
	}

	.bubble-label {
		width: 2.6rem;
		color: var(--muted);
		font-size: 0.7rem;
		letter-spacing: 0.06em;
	}

	.panel button {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 999px;
		padding: 0.2rem 0.55rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.panel button:hover,
	.panel button.on {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.commit,
	.panel button.note {
		align-self: flex-start;
	}

	.preview {
		color: var(--muted);
		font-size: 0.75rem;
		white-space: pre;
	}

	.new-state {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
		font-size: 0.8rem;
	}

	.hint {
		margin: 0;
		color: var(--quiet);
		font-size: 0.8rem;
		font-style: italic;
	}

	.hint code {
		font-style: normal;
		color: var(--accent);
	}

	/* One line beside Track +, cut rather than wrapped. */
	.empty {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
