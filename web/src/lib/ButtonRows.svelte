<!--
	A bar of buttons, laid out the one way every button-bearing widget
	lays them out (da-vu7pi) — the commands widget and the fields widget
	today, and the next one by using this rather than copying its CSS.

	THE SCROLL RULE (vividn, 2026-09-22): the bar is always reached along
	one axis — `axis`, left and right in a pane, up and down over the bottom bar's output — and a taller box buys it more rows,
	up to `maxRows` (three, MAX_ROWS, unless the widget lifts it: the fields
	widget takes all its box fits, da-arms4), never a wrap onto the other axis. The rows are counted
	from the box the layout gave the bar and the size of the `probe` snippet
	— one of the widget's own buttons, drawn and hidden — so the bar's own
	content never enters the question (`watchRows` in #lib/commands.ts).

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

	CENTERED (da-syu4f, vividn, 2026-10-27: «fields should appear more in
	the center of the fields widget when there are few, to give space
	around them»): a widget that passes `center` has a bar its buttons do
	not fill hold them in the middle of its box — along the axis while they
	do not reach its end, and across it while there are fewer lines than
	the box fits. A full bar is laid out as without it.

	A widget hands in its `items`, a `key` for each, and `children`, a
	snippet that draws one (`{ item }`); its buttons keep its own classes, which
	style them where they are written.
-->
<script lang="ts" generics="T">
	import { onMount, type Snippet } from 'svelte';
	import { MAX_ROWS, lanesOf, runsOf, watchRows, type Axis } from './commands.js';

	interface Props {
		items: T[];
		key: (item: T) => unknown;
		axis?: Axis;
		/** How `items` are laid into so many rows; dealt by index unless the widget says otherwise. */
		arrange?: (items: T[], rows: number) => T[][];
		/** Which section an item is in; consecutive items of one section are drawn together. */
		section?: (item: T) => unknown;
		/** The most rows drawn, whatever the box fits; `Infinity` for every row it fits. */
		maxRows?: number;
		/** Hold buttons that do not fill the box in its middle. */
		center?: boolean;
		/** The scroller itself, for a widget that has to put a scroll back. */
		bar?: HTMLElement;
		class?: string;
		onpointerenter?: (e: PointerEvent) => void;
		onpointerleave?: (e: PointerEvent) => void;
		/** One of the widget's own buttons, drawn hidden: what the rows are counted by. */
		probe?: Snippet;
		children?: Snippet<[{ item: T }]>;
	}

	let {
		items,
		key,
		axis = 'x',
		arrange = lanesOf,
		section = undefined,
		maxRows = MAX_ROWS,
		center = false,
		bar = $bindable(),
		class: classes = '',
		onpointerenter,
		onpointerleave,
		probe,
		children
	}: Props = $props();

	let stick: HTMLElement;
	/** The rows the box fits; the cap is applied after, so it can move on its own. */
	let fit = $state(1);
	const rows = $derived(Math.min(maxRows, fit));

	onMount(() => watchRows(bar!, stick, (n) => (fit = n), axis));

	const groups = $derived(
		(section ? runsOf(items, section) : [items]).map((run) => ({
			name: section?.(run[0]),
			lanes: arrange(run, rows)
		}))
	);
	/** Every group is as many lines as the fullest, so a line runs straight across them. */
	const lines = $derived(Math.max(1, ...groups.map(({ lanes }) => lanes.length)));
</script>

<!-- The measuring stick: the widget's own button, laid out and invisible,
     outside the bar and out of flow, so nothing that reads the bar's
     buttons finds it. -->
<span class="probe" aria-hidden="true" bind:this={stick}>{@render probe?.()}</span>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="bar {classes} {axis}"
	bind:this={bar}
	class:center
	class:short={center && lines < rows}
	data-rows={rows}
	data-axis={axis}
	{onpointerenter}
	{onpointerleave}
>
	{#each groups as { name, lanes }}
		<div class="section" data-section={name} style:--lines={lines}>
			{#each lanes as lane}
				<div class="lane">
					{#each lane as item (key(item))}
						{@render children?.({ item })}
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

	/* Auto margins, not `justify-content: center`: they come to nothing
	   once the buttons overrun the box, so the scroll still starts at its
	   first button. */
	.center.x > .section:first-child {
		margin-inline-start: auto;
	}

	.center.x > .section:last-child {
		margin-inline-end: auto;
	}

	.center.y > .section:first-child {
		margin-block-start: auto;
	}

	.center.y > .section:last-child {
		margin-block-end: auto;
	}

	/* Across the axis the lines are their own height, not shares of the box. */
	.short {
		align-items: center;
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
