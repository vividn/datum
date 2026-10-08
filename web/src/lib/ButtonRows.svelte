<!--
	A bar of buttons, laid out the one way every button-bearing widget
	lays them out (da-vu7pi) — the commands widget and the fields widget
	today, and the next one by using this rather than copying its CSS.

	THE SCROLL RULE (vividn, 2026-09-22): the bar is always reached along
	one axis — `axis`, scrolled left and right today — and a taller box buys it more rows,
	up to `maxRows` (three, MAX_ROWS, unless the widget lifts it: the fields
	widget takes all its box fits, da-arms4), never a wrap onto the other axis. The rows are counted
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

	SECTIONS (da-0mepr): a widget that passes `section` has its items cut
	into runs of one section each, every run dealt into rows of its own
	and set apart from the next by a divider — the commands widget's kinds
	of action. The runs share the one scroll and the one count of rows.

	A widget hands in its `items`, a `key` for each, and a default slot
	that draws one (`let:item`); its buttons keep its own classes, which
	style them where they are written.
-->
<script lang="ts" generics="T">
	import { onMount } from 'svelte';
	import { MAX_ROWS, lanesOf, watchRows, type Axis } from './commands.js';

	export let items: T[];
	export let key: (item: T) => unknown;
	export let axis: Axis = 'x';
	/** How `items` are laid into so many rows; dealt by index unless the widget says otherwise. */
	export let arrange: (items: T[], rows: number) => T[][] = lanesOf;
	/** Which section an item is in; consecutive items of one section are drawn together. */
	export let section: ((item: T) => unknown) | undefined = undefined;
	/** The most rows drawn, whatever the box fits; `Infinity` for every row it fits. */
	export let maxRows = MAX_ROWS;
	/** The scroller itself, for a widget that has to put a scroll back. */
	export let bar: HTMLElement | undefined = undefined;

	let probe: HTMLElement;
	/** The rows the box fits; the cap is applied after, so it can move on its own. */
	let fit = 1;
	$: rows = Math.min(maxRows, fit);

	onMount(() => watchRows(bar!, probe, (n) => (fit = n), axis));

	/** Consecutive items of one section, as runs; everything one run without `section`. */
	const runsOf = (items: T[]): T[][] =>
		items.reduce<T[][]>((runs, item, i) => {
			if (i && section && section(item) === section(items[i - 1])) runs.at(-1)!.push(item);
			else runs.push([item]);
			return runs;
		}, []);

	$: groups = (section ? runsOf(items) : [items]).map((run) => ({
		name: section?.(run[0]),
		lanes: arrange(run, rows)
	}));
	/** Every group is as many lines as the fullest, so a line runs straight across them. */
	$: lines = Math.max(1, ...groups.map(({ lanes }) => lanes.length));
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
	{#each groups as { name, lanes }}
		<div class="section" data-section={name} style:--lines={lines}>
			{#each lanes as lane}
				<div class="lane">
					{#each lane as item (key(item))}
						<slot {item} />
					{/each}
				</div>
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
		flex-direction: row;
		overflow-x: auto;
		overflow-y: hidden;
	}

	.bar.y {
		flex-direction: column;
		overflow-x: hidden;
		overflow-y: auto;
	}

	/* A section's lines each take an equal share of the box, however few it
	   fills, so every section's first line is the others' first line. */
	.section {
		flex: none;
		display: grid;
		gap: 0.4rem;
	}

	.x > .section {
		grid-template-rows: repeat(var(--lines), minmax(0, 1fr));
	}

	.y > .section {
		grid-template-columns: repeat(var(--lines), minmax(0, 1fr));
	}

	/* The divider between two sections: a rule in the middle of the gap the
	   bar leaves anyway, so it costs no room. One that added its own width
	   put `end` off a 412px phone's one row (5.5px over, da-0mepr). */
	.x > .section + .section {
		border-left: 1px solid var(--rule);
		margin-left: calc(-0.2rem - 0.5px);
		padding-left: calc(0.2rem - 0.5px);
	}

	.y > .section + .section {
		border-top: 1px solid var(--rule);
		margin-top: calc(-0.2rem - 0.5px);
		padding-top: calc(0.2rem - 0.5px);
	}

	/* Each line centres its buttons, so a button taller than the probe
	   cannot push the next line out of it; along the axis it is
	   as long as its own buttons. */
	.lane {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.y .lane {
		flex-direction: column;
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
