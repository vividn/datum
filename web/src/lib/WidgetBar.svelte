<!--
	The widget bar — one button per tracked field, and the primary way
	anything gets logged without typing (da-nz29).

	Press to log. Press and hold (or right-click, or the menu key) to open
	the `-t` / `dur` bubbles first. A multi-state field opens its state menu
	either way.

	The bar never runs anything itself: it composes an argv and dispatches
	it, and the page runs it through `run()` and echoes the result's own
	argv into the REPL. That is the teach loop, and it only teaches the
	truth because the echoed argv IS the executed argv — there is no second
	string built for display that could drift from it.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { pressArgv, declareArgv, type Adjust, type Widget, type WidgetShape } from './widgets.js';

	export let widgets: Widget[] = [];
	export let disabled = false;

	const dispatch = createEventDispatcher<{ run: string[] }>();

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
			: ({ kind: newKind } satisfies WidgetShape);

	$: addArgv = declareArgv(newField.trim() || '<field>', newShape);

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
		if (!field) return;
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
	{#each widgets as widget (widget.field)}
		<div class="slot">
			<button
				class="widget"
				class:active={open === widget}
				style={widget.color ? `--field-color: ${widget.color}` : undefined}
				{disabled}
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
				{widget.field}
			</button>

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

	<div class="slot">
		{#if adding}
			<form class="panel add" on:submit|preventDefault={declare}>
				<!-- svelte-ignore a11y-autofocus -->
				<input bind:value={newField} placeholder="field" spellcheck="false" autofocus />
				<select bind:value={newKind} aria-label="what a press logs">
					<option value="point">occur — a thing that happened</option>
					<option value="state">start — a state that begins</option>
					<option value="states">switch — one of several states</option>
				</select>
				{#if newKind === 'states'}
					<input bind:value={newStates} placeholder="states, comma-separated" spellcheck="false" />
				{/if}
				<button type="submit">track it</button>
				<code class="preview">{addArgv.join(' ')}</code>
			</form>
		{:else}
			<button class="widget add-button" {disabled} title="track a new field" on:click={() => { close(); adding = true; }}>
				+
			</button>
		{/if}
	</div>

	{#if widgets.length === 0}
		<p class="hint empty">nothing tracked yet — log something below, or add a field with (+)</p>
	{/if}
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

	.widget {
		/* A held press must not also select the label or raise the platform's
		   text callout, on touch least of all. */
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
		touch-action: manipulation;
		background: #0d0d1a;
		border: 1px solid var(--field-color, #2a2a4a);
		border-radius: 999px;
		padding: 0.35rem 0.85rem;
		color: var(--field-color, #c8c8e8);
		font: inherit;
		font-size: 0.85rem;
		cursor: pointer;
	}

	.widget:hover:not(:disabled),
	.widget.active {
		border-color: #7c7cff;
		color: #e0e0ff;
	}

	.widget:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.add-button {
		color: #7c7cff;
	}

	.panel {
		position: absolute;
		z-index: 10;
		top: calc(100% + 0.3rem);
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

	.empty {
		align-self: center;
	}
</style>
