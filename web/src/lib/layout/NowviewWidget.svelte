<!--
	The nowview widget (da-pcis): datum's nowview picture through a CliPane,
	and a + / − over its top-left corner stepping how far back the picture
	reaches — down to 0, the snapshot of each field's state. The window's size
	is written nowhere: each button's title names the step it goes to
	(da-rex1). The step is the widget's setting, handed out with `change`, so
	the tree keeps it with the layout.

	While the time cursor is set ($lib/cursor.ts) the window is centred on
	it, over the `at` a hand-written tree may carry; its dotted line is the
	picture's own (the `picture` action draws it wherever the cursor falls).

	The controls sit beside the pane, not inside its picture: a redraw
	replaces the picture, never them.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import CliPane from '$lib/CliPane.svelte';
	import { cursor } from '$lib/cursor.js';
	import { NOWVIEW_HISTORY, NOWVIEW_STEPS, nowviewSpec } from '$lib/panes.js';

	/** How far back the window reaches: one of NOWVIEW_STEPS. */
	export let history = NOWVIEW_HISTORY;
	/** The instant the window is centred on (`--at`); none is now. */
	export let at: string | undefined = undefined;
	export let epoch = 0;

	const dispatch = createEventDispatcher<{ change: string }>();

	// A history off the list (a hand-written tree) steps from the default.
	$: i = NOWVIEW_STEPS.indexOf(NOWVIEW_STEPS.includes(history) ? history : NOWVIEW_HISTORY);
	$: up = NOWVIEW_STEPS[i + 1];
	$: down = NOWVIEW_STEPS[i - 1];
	$: spec = nowviewSpec(history, $cursor?.at.toISOString() ?? at);
</script>

<div class="nowview">
	<CliPane {spec} {epoch} />
	<div class="nowview-steps">
		<button
			type="button"
			class="control badge step"
			aria-label="longer window"
			title={up ? `longer window (${up})` : 'longer window'}
			disabled={!up}
			on:click={() => dispatch('change', up)}>+</button
		>
		<button
			type="button"
			class="control badge step"
			aria-label="shorter window"
			title={down ? `shorter window (${down === '0' ? 'now' : down})` : 'shorter window'}
			disabled={!down}
			on:click={() => dispatch('change', down)}>−</button
		>
	</div>
</div>

<style>
	/* The frame's content box, for the pane inside (`display: contents` in a
	   frame) and for the steps over its top-left corner. */
	.nowview {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	/* Clear of a title lying over that corner (PaneFrame.svelte). */
	.nowview-steps {
		position: absolute;
		top: calc(4px + var(--under-top, 0px));
		left: calc(4px + var(--under-left, 0px));
		display: flex;
		gap: 2px;
		/* `.control.badge` (app.css) draws the steps, and its box is
		   `--badge` — which a pane sets to its title row and these would
		   inherit, being inside one. A step is a fingertip's 24px wherever
		   it is drawn, so the row says so once, for both of them. */
		--badge: 24px;
	}

	.step:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
