<!--
	The nowview widget (da-pcis): datum's nowview picture through a CliPane,
	and a − / + in its corner stepping how far back the picture reaches —
	down to 0, the snapshot of each field's state. The step is the widget's
	setting, handed out with `change`, so the tree keeps it with the layout.

	The controls sit beside the pane, not inside its picture: a redraw
	replaces the picture, never them.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import CliPane from '$lib/CliPane.svelte';
	import { NOWVIEW_HISTORY, NOWVIEW_STEPS, nowviewSpec } from '$lib/panes.js';

	/** How far back the window reaches: one of NOWVIEW_STEPS. */
	export let history = NOWVIEW_HISTORY;
	/** The instant the window is centred on (`--at`); none is now. */
	export let at: string | undefined = undefined;
	export let epoch = 0;

	const dispatch = createEventDispatcher<{ change: string }>();

	// A history off the list (a hand-written tree) steps from the default.
	$: i = NOWVIEW_STEPS.indexOf(NOWVIEW_STEPS.includes(history) ? history : NOWVIEW_HISTORY);
	$: spec = nowviewSpec(history, at);
</script>

<div class="nowview">
	<CliPane {spec} {epoch} />
	<div class="nowview-steps">
		<button
			type="button"
			aria-label="shorter window"
			disabled={i === 0}
			on:click={() => dispatch('change', NOWVIEW_STEPS[i - 1])}>−</button
		>
		<span aria-live="polite">{history === '0' ? 'now' : history}</span>
		<button
			type="button"
			aria-label="longer window"
			disabled={i === NOWVIEW_STEPS.length - 1}
			on:click={() => dispatch('change', NOWVIEW_STEPS[i + 1])}>+</button
		>
	</div>
</div>

<style>
	/* The frame's content box, for the pane inside (`display: contents` in a
	   frame) and for the steps over its top-right corner. */
	.nowview {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	.nowview-steps {
		position: absolute;
		top: 4px;
		right: 4px;
		display: flex;
		align-items: center;
		gap: 2px;
		font-size: 0.75rem;
		color: #7c7cff;
	}

	/* The … badge's size and colours (PaneMenu.svelte). */
	button {
		box-sizing: border-box;
		inline-size: 24px;
		block-size: 24px;
		display: grid;
		place-items: center;
		padding: 0;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
		color: #55558a;
		font: inherit;
		font-size: 0.85rem;
		line-height: 1;
		cursor: pointer;
	}

	button:hover:not(:disabled),
	button:focus-visible {
		color: #adadff;
		border-color: #7c7cff;
	}

	button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	span {
		padding: 0 0.25rem;
		background: #0d0d1a;
		border-radius: 4px;
	}
</style>
