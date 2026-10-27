<!--
	A bar of buttons as SECTIONS THAT SCROLL (da-u79wl) — a PROTOTYPE of
	vividn's sketch, 2026-10-09, beside `ButtonRows` and not instead of it:
	«maybe each section has vertical scroll, and then we can have a different
	number of columns per section and horizontal scrolling to go between the
	sections, that might let us be more space saving, while prioritizing
	certain buttons in each section, and prioritizing some sections by where
	left/right it appear and how many columns it gets.»

	So a section is a grid of `columns[section]` columns, its items in the
	caller's order — the first row is the section's most wanted — and it
	scrolls up and down on its own. The sections stand side by side in the
	caller's order and are paged left and right, each one stopping at the
	left edge.

	The box shows WHOLE rows: as many as fit (`watchRows`, ButtonRows' own
	count from its own probe) share its height (`--row`), and a section
	stops on a row. So nothing peeks out to say there is more, and a phone draws no
	scrollbar: a section with rows out of sight wears a thumb on its right
	edge, `--of` of the edge long and `--at` down it (`mark`).

	`pinned` items are drawn before the scroller and outside it, so nothing
	scrolls them away: the commands widget's red «conflicts», which is what
	the sketch came from — last in one scrolling row, it was off a phone's
	screen.

	Without `section` (a drilled bar) the items are one run that wraps to the
	box and scrolls up and down.

	`bar` and the slots are ButtonRows', so a widget chooses between the two
	with one `svelte:component`.
-->
<script lang="ts" generics="T">
	import { onMount, type Snippet } from 'svelte';
	import { ROW_GAP, runsOf, watchRows } from './commands.js';

	interface Props {
		items: T[];
		key: (item: T) => unknown;
		/** Which section an item is in; consecutive items of one section are drawn together. */
		section?: (item: T) => unknown;
		/** How many columns each section stands in; one, for a section not named here. */
		columns?: Record<string, number>;
		/** The items that must not scroll away. */
		pinned?: (item: T) => boolean;
		/** The whole bar, as ButtonRows hands it out. */
		bar?: HTMLElement;
		class?: string;
		/** The kept `<slot>`s, filled by a runes caller's snippets (`<slot>` reads them from `$$props`, so they are
		    typed here and not destructured: `probe` is also the probe span's binding). */
		probe?: Snippet;
		children?: Snippet<[{ item: T }]>;
	}

	let {
		items,
		key,
		section = undefined,
		columns = {},
		pinned = () => false,
		bar = $bindable(),
		class: classes = ''
	}: Props = $props();

	let probe: HTMLElement;
	let rows = $state(1);
	/** A row's height, in px: the rows that fit share the box between them. */
	let row = $state(0);

	const held = $derived(items.filter(pinned));
	const rest = $derived(items.filter((item) => !pinned(item)));
	const groups = $derived((section ? runsOf(rest, section) : [rest]).map((run) => ({ name: section?.(run[0]) as string | undefined, run })));

	/**
	 * Size and place every section's thumb; no length where every row shows.
	 * Both are shares of the grid's whole height, so they sum to 1 at the end
	 * of the scroll. Called only once mounted: after an update, on a scroll,
	 * and when the box changes — which rewraps a drilled run and changes
	 * nothing an update would follow.
	 */
	function mark(): void {
		for (const grid of bar!.querySelectorAll('.grid')) {
			const { clientHeight, scrollHeight, scrollTop } = grid;
			const section = grid.parentElement!;
			section.style.setProperty('--of', `${scrollHeight > clientHeight + 1 ? clientHeight / scrollHeight : 0}`);
			section.style.setProperty('--at', `${scrollTop / scrollHeight}`);
		}
	}

	onMount(() =>
		watchRows(bar!, probe, (n) => {
			rows = n;
			row = (bar!.clientHeight + ROW_GAP) / n - ROW_GAP;
			mark();
		})
	);
	// After an update: a change to what the grids hold moves their thumbs.
	$effect(() => {
		void [held, groups, columns, row];
		mark();
	});
</script>

<!-- The slots stay `<slot>`s until the callers pass snippets (the sweep,
     da-ul23i): a legacy caller's `let:item` and named slot never reach a
     `{@render}`. -->
<!-- svelte-ignore slot_element_deprecated -->
<span class="probe" aria-hidden="true" bind:this={probe}><slot name="probe" /></span>

<div class="pages {classes}" style:--row="{row}px" data-rows={rows} bind:this={bar} onscrollcapture={mark}>
	{#if held.length}
		<div class="pinned">
			{#each held as item (key(item))}
				<!-- svelte-ignore slot_element_deprecated -->
				<div class="cell"><slot {item} /></div>
			{/each}
		</div>
	{/if}
	<div class="scroller">
		{#each groups as { name, run } (name)}
			<div class="section" data-section={name} style:--columns={columns[name ?? ''] ?? 1}>
				<div class="grid">
					{#each run as item (key(item))}
						<!-- svelte-ignore slot_element_deprecated -->
						<div class="cell"><slot {item} /></div>
					{/each}
				</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.pages,
	.scroller {
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		gap: 0.4rem;
	}

	.scroller {
		overflow-x: auto;
		overflow-y: hidden;
		scroll-snap-type: x mandatory;
	}

	/* What the thumb is placed in; the scroller is the grid inside it. */
	.section {
		position: relative;
		flex: none;
		display: flex;
		scroll-snap-align: start;
	}

	/* The divider, in the middle of the gap the bar leaves anyway: ButtonRows'. */
	.section + .section {
		border-left: 1px solid var(--rule);
		margin-left: calc(-0.2rem - 0.5px);
		padding-left: calc(0.2rem - 0.5px);
	}

	/* The rows that fit share the box, so every section's rows are level
	   and a scroll stops with none of them cut. */
	.pinned,
	.grid {
		flex: none;
		display: grid;
		grid-template-columns: repeat(var(--columns, 1), auto);
		grid-auto-rows: var(--row);
		gap: 0.4rem;
	}

	/* A row's whole height, which is what a scroll stops on; the button is
	   centred in it at its own height and fills its column. */
	.cell {
		display: flex;
		align-items: center;
		scroll-snap-align: start;
	}

	.cell > :global(*) {
		flex: 1;
	}

	.grid {
		overflow: hidden auto;
		scrollbar-width: none;
		scroll-snap-type: y mandatory;
	}

	/* One run, no sections: it has the box to itself and wraps to it. */
	.section:not([data-section]),
	.section:not([data-section]) > .grid {
		flex: 1;
		min-width: 0;
	}

	.section:not([data-section]) > .grid {
		display: flex;
		flex-wrap: wrap;
		align-content: start;
	}

	.section::after {
		content: '';
		position: absolute;
		right: calc(-0.2rem - 1.5px);
		top: calc(var(--at, 0) * 100%);
		width: 3px;
		height: calc(var(--of, 0) * 100%);
		border-radius: 1.5px;
		background: var(--accent);
		pointer-events: none;
	}

	.probe {
		position: absolute;
		display: flex;
		visibility: hidden;
		pointer-events: none;
	}
</style>
