<!--
	What an empty tab shows: the registered kinds, by title, click to fill.

	The same picker for a new tab and for the new pane a split makes.
	There is no drag-and-drop (vividn: "No drag and drop, rather split and
	select widget from the … menu for that pane"); moving a widget is a
	split or an add here and a close there.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import Pane from '$lib/Pane.svelte';
	import { KINDS, PICKABLE } from './registry.js';

	const dispatch = createEventDispatcher<{ pick: string }>();
</script>

<Pane title="empty">
	<div class="picker" role="group" aria-label="choose a widget">
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

	button:hover,
	button:focus-visible {
		border-color: #7c7cff;
		color: #e0e0ff;
	}
</style>
