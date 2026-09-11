<!--
	What an empty tab shows: the registered kinds, by title, click to fill —
	and, first, the layouts it could hold instead (da-0a5o): `day` and the
	saved ones, inlined as a tab's content the way a root tab holds one.

	The same picker for a new tab and for the new pane a split makes.
	There is no drag-and-drop (vividn: "No drag and drop, rather split and
	select widget from the … menu for that pane"); moving a widget is a
	split or an add here and a close there.

	A layout open above this tab is not offered: `day` inside `day` would
	be `day` inside `day` inside `day`. The names above come down the tree
	as a context (Layout.svelte), and the shell refuses the same names if
	they arrive some other way.
-->
<script lang="ts">
	import { createEventDispatcher, getContext, onMount } from 'svelte';
	import { readable, type Readable } from 'svelte/store';
	import Pane from '$lib/Pane.svelte';
	import { KINDS, PICKABLE } from './registry.js';
	import { listLayouts } from './store.js';

	const dispatch = createEventDispatcher<{ pick: string; layout: string }>();

	const ancestors = getContext<Readable<string[]> | undefined>('layout-ancestors') ?? readable<string[]>([]);

	let saved: string[] = [];
	onMount(async () => {
		saved = (await listLayouts().catch(() => [])).map((l) => l.name);
	});

	// `day` and the saved layouts; a layout that is one widget is the widget.
	$: layouts = ['day', ...saved.filter((n) => n !== 'day')].filter((n) => !$ancestors.includes(n));
</script>

<Pane title="empty">
	<div class="picker" role="group" aria-label="choose a widget">
		{#each layouts as name (name)}
			<button type="button" class="layout" on:click={() => dispatch('layout', name)}>{name}</button>
		{/each}
		{#each PICKABLE as kind (kind)}
			<button type="button" on:click={() => dispatch('pick', kind)}>{KINDS[kind].meta.title}</button>
		{/each}
	</div>
</Pane>

<style>
	.picker {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	button {
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 999px;
		padding: 0.2rem 0.65rem;
		color: #c8c8e8;
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}

	/* A layout reads as a layout: the root bar's own colour. */
	.layout {
		color: #7c7cff;
	}

	button:hover,
	button:focus-visible {
		border-color: #7c7cff;
		color: #e0e0ff;
	}
</style>
