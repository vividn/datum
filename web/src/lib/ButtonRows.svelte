<!--
	A bar of buttons, laid out the one way every button-bearing widget
	lays them out (da-vu7pi) — the commands widget and the fields widget
	today, and the next one by using this rather than copying its CSS.

	THE SCROLL RULE (vividn, 2026-09-22): the bar is always reached along
	one axis — `axis`, scrolled left and right today — and a taller box buys it more rows,
	up to three, never a wrap onto the other axis. The rows are counted
	from the box the layout gave the bar and the size of the `probe` slot
	— one of the widget's own buttons, drawn and hidden — so the bar's own
	content never enters the question (`watchRows` in $lib/commands.ts).

	THE ROWS PACK ON THEIR OWN (vividn, 2026-09-21: «each row gets
	independent spacing but buttons automatically flow into the left most
	space»). The items are dealt into the rows by index (`lanesOf`), and
	each row is its own flex line: every button at its own width, no button
	padded out to the widest in a column, as the grid this replaced did.
	The first few items still start the rows, so the widget's order is the
	reader's. A widget whose rows MEAN something passes its own `arrange`
	instead — the fields widget's rows are bands of height (da-xtkcp). TAB
	ORDER follows the drawing — along the first row, then the second.

	A widget hands in its `items`, a `key` for each, and a default slot
	that draws one (`let:item`); its buttons keep its own classes, which
	style them where they are written.
-->
<script lang="ts" generics="T">
	import { onMount } from 'svelte';
	import { lanesOf, watchRows, type Axis } from './commands.js';

	export let items: T[];
	export let key: (item: T) => unknown;
	export let axis: Axis = 'x';
	/** How `items` are laid into so many rows; dealt by index unless the widget says otherwise. */
	export let arrange: (items: T[], rows: number) => T[][] = lanesOf;
	/** The scroller itself, for a widget that has to put a scroll back. */
	export let bar: HTMLElement | undefined = undefined;

	let probe: HTMLElement;
	let rows = 1;

	onMount(() => watchRows(bar!, probe, (n) => (rows = n), axis));

	$: lanes = arrange(items, rows);
</script>

<!-- The measuring stick: the widget's own button, laid out and invisible,
     outside the bar and out of flow, so nothing that reads the bar's
     buttons finds it. -->
<span class="probe" aria-hidden="true" bind:this={probe}><slot name="probe" /></span>

<div
	class="bar {$$props.class ?? ''} {axis}"
	bind:this={bar}
	data-rows={rows}
	data-axis={axis}
	on:pointerenter
	on:pointerleave
>
	{#each lanes as lane}
		<div class="lane">
			{#each lane as item (key(item))}
				<slot {item} />
			{/each}
		</div>
	{/each}
</div>

<style>
	.bar {
		/* What an item laid over the whole box is placed in — the fields
		   widget's states (da-7q3ka). */
		position: relative;
		flex: 1;
		/* A flex item will not shrink under its content unless told to, and
		   a row of buttons that cannot shrink spills out of the pane instead
		   of scrolling inside it. */
		min-height: 0;
		min-width: 0;
		display: flex;
		gap: 0.4rem;
	}

	.bar.x {
		flex-direction: column;
		overflow-x: auto;
		overflow-y: hidden;
	}

	.bar.y {
		flex-direction: row;
		overflow-x: hidden;
		overflow-y: auto;
	}

	/* Each line takes an equal share of the box and centres its buttons, so
	   a button taller than the probe (`Track +`) cannot push the next line
	   out of it; along the axis it is as long as its own buttons. */
	.lane {
		flex: 1 1 0;
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.x > .lane {
		min-height: 0;
		width: max-content;
	}

	.y > .lane {
		flex-direction: column;
		min-width: 0;
		height: max-content;
	}

	.lane > :global(*) {
		flex: none;
	}

	.probe {
		position: absolute;
		display: flex;
		visibility: hidden;
		pointer-events: none;
	}
</style>
