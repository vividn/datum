<!--
	The fields widget — every field `datum fields` lists, and the primary
	way anything gets logged without typing (da-nz29, da-c7u8).

	Track + comes first: the way into the whole app, so it is there on an
	empty database too. Then the TRACKED fields (a spec exists), each shaped
	by its kind and painted by what it is now (`lookOf` in widgets.ts): a
	point is a filled pill; a block is a rectangle, filled while it is on;
	a switch is a rectangle filled with its current state's colour; a keyed
	record is its name, underlined, and a click prefills `add <field> ` in
	the bottom bar rather than running anything. Last, dim, the fields
	logged without a spec, each with a `track` that opens the form with the
	name in it — `startTracking` is the one place that form opens from, for
	the set-up walkthrough to take over.

	Press to log. Press and hold (or right-click, or the menu key) to open
	the `-t` / `dur` bubbles first. A multi-state field opens its state menu
	either way.

	The widget never runs anything itself: it composes an argv and
	dispatches it, and the page runs it through `run()` and echoes the
	result's own argv into the REPL. That is the teach loop, and it only
	teaches the truth because the echoed argv IS the executed argv — there
	is no second string built for display that could drift from it.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import {
		declareArgv,
		lookOf,
		pressArgv,
		NEUTRAL,
		type Adjust,
		type SwitchLabel,
		type Widget,
		type WidgetShape
	} from './widgets.js';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';

	export let widgets: Widget[] = [];
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
	let open: Widget | null = null;
	/** Whether the open panel shows the adjustment bubbles (long press did). */
	let adjustable = false;
	let adjust: Adjust = {};

	let holdTimer: ReturnType<typeof setTimeout> | null = null;
	let held = false;

	let adding = false;
	let newField = '';
	let newKind: WidgetShape['kind'] = 'point';
	let newStates = '';

	$: tracked = widgets.filter((w) => w.tracked);
	$: untracked = widgets.filter((w) => !w.tracked);

	// A spec's colour is spelled however it was typed (`orange`, `#ffa500`);
	// the ink rule needs `#rrggbb`, which the browser's own parser gives.
	let ctx: CanvasRenderingContext2D | null = null;
	function hex(color: string): string {
		ctx ??= document.createElement('canvas').getContext('2d');
		if (!ctx) return NEUTRAL;
		ctx.fillStyle = NEUTRAL;
		ctx.fillStyle = color;
		return ctx.fillStyle.startsWith('#') ? ctx.fillStyle : NEUTRAL;
	}

	function paint(widget: Widget, variant: SwitchLabel) {
		const look = lookOf(widget, variant, hex);
		return { look, style: `--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}` };
	}

	function statesOf(shape: WidgetShape): string[] {
		return shape.kind === 'states' ? shape.states : [];
	}

	// Reactive, not a function call: Svelte tracks the variables an
	// expression *names*, so `declareArgv(field, newShape())` in the markup
	// would never see a change to `newKind` or `newStates` — the preview
	// would sit at `--default-verb occur` while the form said otherwise,
	// and a preview that disagrees with the command teaches a lie.
	$: newShape =
		newKind === 'states'
			? ({
					kind: 'states',
					states: newStates
						.split(',')
						.map((s) => s.trim())
						.filter(Boolean)
				} satisfies WidgetShape)
			: ({ kind: newKind } as WidgetShape);

	$: addArgv = declareArgv(newField.trim() || '<field>', newShape);

	// What the name would do if it were submitted. A field name is one argv
	// token, and this form used to accept anything: `my coffee` made a chip
	// that logged but could not be typed, `cups=2` and `-weird` made chips
	// that errored when pressed (da-b049). `spec set` refuses all three now,
	// so the form says so first — and offers the nearest name that works,
	// because "my coffee" is what a person naturally types.
	$: hint = newField.trim() === '' ? null : fieldNameHint(newField.trim());
	$: fixed = hint === null ? null : suggestFieldName(newField.trim());

	function panel(widget: Widget, withAdjust: boolean) {
		adding = false;
		adjust = {};
		adjustable = withAdjust;
		open = widget;
	}

	function close() {
		open = null;
		adding = false;
	}

	function emit(argv: string[]) {
		close();
		dispatch('run', argv);
	}

	/** Open the form that tracks a field, with `field` in it. */
	function startTracking(field = '') {
		close();
		newField = field;
		adding = true;
	}

	function press(widget: Widget) {
		// A multi-state field can't be logged without picking a state, so a
		// plain press opens the menu instead of committing (bead item 2).
		if (widget.shape.kind === 'states') panel(widget, false);
		else emit(pressArgv(widget));
	}

	function clearHold() {
		if (holdTimer !== null) clearTimeout(holdTimer);
		holdTimer = null;
	}

	// Pointer events rather than touch/mouse pairs: one code path reaches
	// a finger, a stylus and a held mouse button alike.
	function down(widget: Widget) {
		held = false;
		clearHold();
		holdTimer = setTimeout(() => {
			holdTimer = null;
			held = true;
			panel(widget, true);
		}, HOLD_MS);
	}

	// The click still fires after a long press (and after the keyboard's
	// Enter/Space, which never sends a pointerdown at all) — swallow the one
	// that follows a hold, and treat everything else as a short press.
	function click(widget: Widget) {
		if (held) {
			held = false;
			return;
		}
		if (open === widget) close();
		else press(widget);
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
		if (event.key === 'Escape') close();
	}
</script>

<svelte:window on:pointerdown={onWindowPointerDown} on:keydown={onWindowKeyDown} />

<div class="bar" bind:this={bar}>
	<div class="slot">
		<button class="track add-button" {disabled} title="track a new field" on:click={() => (adding ? close() : startTracking())}>
			Track +
		</button>
		{#if adding}
			<form class="panel add" on:submit|preventDefault={declare}>
				<!-- svelte-ignore a11y-autofocus -->
				<input bind:value={newField} placeholder="field" spellcheck="false" autofocus />
				<select bind:value={newKind} aria-label="what a press logs">
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
			</form>
		{/if}
	</div>

	{#if widgets.length === 0}
		<!-- No direction word here on purpose: this hint said "below" while the
		     widget bar was at the top of the page (da-nz29), and da-1x72 moved the
		     bar to the bottom without the copy following. Name the thing, not
		     where it is. -->
		<p class="hint empty">nothing tracked yet — or type a command to log something</p>
	{/if}

	{#each tracked as widget (widget.field)}
		{@const { look, style } = paint(widget, switchLabel)}
		<div class="slot">
			{#if widget.shape.kind === 'record'}
				<!-- Not a button: nothing runs. The line goes to the bar's prompt. -->
				<button
					class="record"
					role="link"
					data-field={widget.field}
					{style}
					{disabled}
					title="add {widget.field} … — fills the prompt"
					on:click={() => dispatch('line', `add ${widget.field} `)}>{widget.field}</button
				>
			{:else}
				<button
					class="widget {widget.shape.kind}"
					class:on={look.fill !== null}
					class:active={open === widget}
					{style}
					{disabled}
					data-field={widget.field}
					aria-haspopup={widget.shape.kind === 'states' ? 'menu' : undefined}
					aria-expanded={open === widget}
					title="press to log · hold for when / how long"
					on:pointerdown={() => down(widget)}
					on:pointerup={clearHold}
					on:pointerleave={clearHold}
					on:pointercancel={clearHold}
					on:contextmenu|preventDefault={() => panel(widget, true)}
					on:click={() => click(widget)}
				>
					{look.label}
				</button>
			{/if}

			{#if open === widget}
				<div class="panel">
					{#if adjustable}
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
					{/if}

					{#if widget.shape.kind === 'states'}
						{#if statesOf(widget.shape).length === 0}
							<p class="hint">
								no states yet — <code>switch {widget.field} &lt;state&gt;</code> in the REPL
							</p>
						{:else}
							<div class="states">
								{#each statesOf(widget.shape) as state (state)}
									<button
										title={pressArgv(widget, adjust, state).join(' ')}
										on:click={() => emit(pressArgv(widget, adjust, state))}
									>
										{state}
									</button>
								{/each}
							</div>
						{/if}
					{:else}
						<button class="commit" on:click={() => emit(pressArgv(widget, adjust))}>log</button>
					{/if}

					<code class="preview"
						>{pressArgv(
							widget,
							adjust,
							widget.shape.kind === 'states' ? '<state>' : undefined
						).join(' ')}</code
					>
				</div>
			{/if}
		</div>
	{/each}

	{#each untracked as widget (widget.field)}
		<span class="untracked" data-field={widget.field}
			>{widget.field}<button
				class="track-this"
				{disabled}
				aria-label="track {widget.field}"
				title="track {widget.field}"
				on:click={() => startTracking(widget.field)}>track</button
			></span
		>
	{/each}
</div>

<style>
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
	}

	.slot {
		position: relative;
	}

	.widget,
	.track,
	.record,
	.track-this {
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

	/* Shape by kind, fill by state (lookOf in widgets.ts): the colours
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
		box-shadow: 0 0 0 2px #e0e0ff;
	}

	.widget:disabled,
	.track:disabled,
	.record:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* The first thing a person meets: larger than the fields, and filled. */
	.track {
		background: #7c7cff;
		border: none;
		border-radius: 0.5rem;
		padding: 0.45rem 1.1rem;
		color: #0a0a1a;
		font-size: 1rem;
		font-weight: 700;
		letter-spacing: 0.02em;
		box-shadow: 0 0 12px #7c7cff66;
	}

	.track:hover:not(:disabled) {
		background: #9a9aff;
	}

	.record {
		background: none;
		border: none;
		padding: 0.35rem 0.3rem;
		color: #c8c8e8;
		font-size: 0.85rem;
		text-decoration: underline 2px var(--line);
		text-underline-offset: 0.3em;
	}

	.untracked {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		color: #55558a;
		font-size: 0.8rem;
	}

	.track-this {
		background: none;
		border: 1px dashed #3a3a6a;
		border-radius: 3px;
		padding: 0 0.35rem;
		color: #7c7cff;
		font-size: 0.7rem;
	}

	.track-this:hover:not(:disabled) {
		border-color: #7c7cff;
	}

	.panel {
		position: absolute;
		z-index: 10;
		/* Upward: the bar is the bottom-most row of the frame at every
		   width, so a panel hung below it opens off the bottom of the
		   screen — reachable only by a scroll the reader has no reason to
		   guess at (da-hkpd). Above the bar it is on screen wherever the
		   bar is. */
		bottom: calc(100% + 0.3rem);
		left: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		min-width: max-content;
		background: #12122a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
		padding: 0.5rem;
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
		color: #55558a;
		font-size: 0.7rem;
		letter-spacing: 0.06em;
	}

	.panel button {
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 999px;
		padding: 0.2rem 0.55rem;
		color: #c8c8e8;
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.panel button:hover,
	.panel button.on {
		border-color: #7c7cff;
		color: #e0e0ff;
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
		color: #55558a;
		font-size: 0.75rem;
		white-space: pre;
	}

	.add input,
	.add select {
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: #e0e0ff;
		font: inherit;
		font-size: 0.8rem;
	}

	.hint {
		margin: 0;
		color: #444466;
		font-size: 0.8rem;
		font-style: italic;
	}

	.hint code {
		font-style: normal;
		color: #7c7cff;
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
