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
	anything. The bar wraps and grows; its pane keeps the height its
	layout gives it and the bar scrolls inside it (da-mbz2), while its
	panels, which hang upward out of it, are hung against the screen so
	that scroller cannot clip them. Ignored fields get no button: the
	Track + form lists them, and a press runs `track <field>`.
	`startTracking` is the one place that form opens from, for the set-up
	walkthrough to take over.

	Press to log. Press and hold (or right-click, or the menu key) to open
	the `-t` / `dur` bubbles first.

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
	import { createEventDispatcher, tick } from 'svelte';
	import {
		declareArgv,
		lookOf,
		pressArgv,
		stateLook,
		type Adjust,
		type Look,
		type SwitchLabel,
		type FieldButton,
		type FieldShape
	} from './fields.js';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';

	export let fields: FieldButton[] = [];
	export let disabled = false;
	/** How a switch in a state is labelled; see `SwitchLabel`. */
	export let switchLabel: SwitchLabel = 'field';

	const dispatch = createEventDispatcher<{ run: string[]; line: string }>();

	/** How long a press has to last before it counts as a long press. */
	const HOLD_MS = 450;

	/** The `-t` bubbles: minutes back from now. */
	const WHEN = [0, 5, 15, 30, 60];
	/** The `dur` bubbles, as typed. The empty string is "no duration". */
	const FOR = ['', '5m', '15m', '30m', '1h'];

	let bar: HTMLElement;
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
	let scrolledTo = 0;

	let holdTimer: ReturnType<typeof setTimeout> | null = null;
	let held = false;

	let adding = false;
	let newField = '';
	// 'log' is «decide from the log», and the default: a person who came
	// here to name a field has not necessarily decided what a press of it
	// means, and a verb chosen for them is a decision they did not make
	// (da-emze). The spec is still created — a colour hangs on it later.
	let newKind: FieldShape['kind'] | 'log' = 'log';
	let newStates = '';

	$: buttons = fields.filter((w) => !w.ignored);
	$: ignored = fields.filter((w) => w.ignored);
	$: picking = picked === null ? null : (buttons.find((b) => b.field === picked) ?? null);

	/** A look as the custom properties every button below is painted from. */
	function vars(look: Look) {
		return `--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}`;
	}

	function statesOf(shape: FieldShape): string[] {
		return shape.kind === 'states' ? shape.states : [];
	}

	// Reactive, not a function call: Svelte tracks the variables an
	// expression *names*, so `declareArgv(field, newShape())` in the markup
	// would never see a change to `newKind` or `newStates` — the preview
	// would sit at the first option's argv while the form said otherwise,
	// and a preview that disagrees with the command teaches a lie.
	$: newShape =
		newKind === 'log'
			? null
			: newKind === 'states'
				? ({
						kind: 'states',
						states: newStates
							.split(',')
							.map((s) => s.trim())
							.filter(Boolean)
					} satisfies FieldShape)
				: ({ kind: newKind } as FieldShape);

	$: addArgv = declareArgv(newField.trim() || '<field>', newShape);

	// What the name would do if it were submitted. A field name is one argv
	// token, and this form used to accept anything: `my coffee` made a chip
	// that logged but could not be typed, `cups=2` and `-weird` made chips
	// that errored when pressed (da-b049). `spec set` refuses all three now,
	// so the form says so first — and offers the nearest name that works,
	// because "my coffee" is what a person naturally types.
	$: hint = newField.trim() === '' ? null : fieldNameHint(newField.trim());
	$: fixed = hint === null ? null : suggestFieldName(newField.trim());

	/**
	 * The panel a LONG press opens: the `-t` / `dur` bubbles, and for a
	 * multi-state field the states to spend them on. A plain press never
	 * opens it — that fills the bar with the states instead (`pick`) — so
	 * the bubbles are the whole reason it exists and are always in it.
	 */
	function panel(button: FieldButton) {
		adding = false;
		adjust = {};
		open = button.field;
	}

	function close() {
		open = null;
		adding = false;
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
		const top = scrolledTo;
		scrolled = null;
		if (scroller) void tick().then(() => (scroller.scrollTop = top));
	}

	function emit(argv: string[]) {
		close();
		back();
		dispatch('run', argv);
	}

	/** Open the form that tracks a field, with `field` in it. */
	function startTracking(field = '') {
		close();
		newField = field;
		adding = true;
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
		scrolledTo = scrolled?.scrollTop ?? 0;
		picked = button.field;
	}

	/** The scrolling ancestor the bar sits in — FieldsPane's body, in the app. */
	function scrollerOf(): HTMLElement | null {
		for (let node = bar?.parentElement; node; node = node.parentElement) {
			if (node.scrollHeight > node.clientHeight) return node;
		}
		return null;
	}

	function press(button: FieldButton) {
		// A multi-state field can't be logged without saying which, so a
		// plain press asks — the bar fills with its states.
		if (button.shape.kind === 'states') pick(button);
		else emit(pressArgv(button));
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

	function declare() {
		const field = newField.trim();
		if (!field || hint !== null) return;
		const argv = declareArgv(field, newShape);
		newField = '';
		newStates = '';
		emit(argv);
	}

	function onWindowPointerDown(event: PointerEvent) {
		if (bar && !bar.contains(event.target as Node)) close();
	}

	function onWindowKeyDown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		// Innermost first: the panel a long press opened, then the input the
		// `+` became, then the state list itself — Escape is its Back.
		if (open || adding) close();
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

<div class="bar" bind:this={bar}>
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
		<div class="slot">
			<button class="track add-button" {disabled} title="track a new field" on:click={() => (adding ? close() : startTracking())}>
				Track +
			</button>
			{#if adding}
				<form class="panel add" use:hang on:submit|preventDefault={declare}>
					<!-- svelte-ignore a11y-autofocus -->
					<input bind:value={newField} placeholder="field" spellcheck="false" autofocus />
					<select bind:value={newKind} aria-label="what a press logs">
						<option value="log">decide from the log</option>
						<option value="point">occur — a thing that happened</option>
						<option value="state">start — a block that begins and ends</option>
						<option value="states">switch — one of several states</option>
						<option value="record">add — a record you type</option>
					</select>
					{#if newKind === 'states'}
						<input bind:value={newStates} placeholder="states, comma-separated" spellcheck="false" />
					{/if}
					<button type="submit" disabled={hint !== null}>track it</button>
					{#if hint === null}
						<code class="preview">{addArgv.join(' ')}</code>
					{:else}
						<p class="hint">
							{hint}{#if fixed}
								<button type="button" class="fix" on:click={() => (newField = fixed ?? '')}
									>use {fixed}</button
								>{/if}
						</p>
					{/if}
					{#if ignored.length > 0}
						<!-- Ignored fields get no button; this is the way back (da-d08c). -->
						<div class="states ignored">
							<span class="bubble-label">ignored</span>
							{#each ignored as { field } (field)}
								<button type="button" data-ignored={field} title="track {field}" on:click={() => emit(['track', field])}
									>{field}</button
								>
							{/each}
						</div>
					{/if}
				</form>
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
						on:click={() => dispatch('line', `add ${button.field} `)}>{button.field}</button
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
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
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

	.commit {
		align-self: flex-start;
	}

	.panel button.fix {
		margin-left: 0.4rem;
		font-size: 0.75rem;
	}

	.add button[disabled] {
		opacity: 0.4;
		cursor: default;
	}

	.preview {
		color: var(--muted);
		font-size: 0.75rem;
		white-space: pre;
	}

	.add input,
	.add select,
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
