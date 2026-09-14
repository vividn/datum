<!--
	The one modal that fills a tab (da-uyx1): the layouts a tab could hold,
	then the widget kinds. An empty pane's «Choose Widget/Layout» button
	opens it, and so does the root bar's `+` — one component, one ordering,
	wherever it is opened. Its caller is `chooser.ts`, which is what the
	pane menu and the root menu call.

	The order: the layouts `chooser.ts` handed it (`day`, then the saved
	ones, less what is open above the tab), and then every `PICKABLE` kind
	by its title, in the registry's order.

	The `<dialog>` pattern is Confirm.svelte's: `showModal` puts focus
	inside and Escape — or a click on the backdrop, which lands on the
	dialog itself — closes it and returns focus to whatever opened it,
	changing nothing.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { PICKABLE, titleOf } from './registry.js';
	import type { Choice } from './chooser.js';

	/** The layouts this tab could hold, in order (chooser.ts). */
	export let layouts: string[] = [];

	const dispatch = createEventDispatcher<{ close: Choice | null }>();

	let dialog: HTMLDialogElement;
	let chosen: Choice | null = null;

	onMount(() => dialog.showModal());

	function pick(choice: Choice) {
		chosen = choice;
		dialog.close();
	}
</script>

<!-- The backdrop is the dialog's own box outside its content, so a click on
     it lands on the dialog: that is what closes it, and it is the pointer's
     shorthand for the Escape the keyboard already has. -->
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
<dialog
	class="chooser"
	aria-label="choose a widget or a layout"
	bind:this={dialog}
	on:close={() => dispatch('close', chosen)}
	on:click={(e) => e.target === dialog && dialog.close()}
>
	<h2>Choose Widget/Layout</h2>
	<!-- Nothing to offer, nothing to head: inside `day`, with nothing else
	     saved, the layouts section is empty and is not drawn. -->
	{#if layouts.length}
		<h3>Layouts</h3>
		<div class="group" role="group" aria-label="Layouts">
			{#each layouts as name (name)}
				<button type="button" class="layout" on:click={() => pick({ layout: name })}>{name}</button>
			{/each}
		</div>
	{/if}
	<h3>Widgets</h3>
	<div class="group" role="group" aria-label="Widgets">
		{#each PICKABLE as kind (kind)}
			<button type="button" class="widget" on:click={() => pick({ kind })}>{titleOf(kind)}</button>
		{/each}
	</div>
</dialog>

<style>
	/* Narrow enough for a 320px phone with the backdrop showing either
	   side, and never taller than the screen: the lists scroll. */
	.chooser {
		inline-size: min(26rem, calc(100vw - 2rem));
		max-block-size: calc(100dvh - 2rem);
		overflow: auto;
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
	}

	.chooser::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	h2 {
		margin: 0 0 0.75rem;
		font-size: 0.9rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: var(--accent);
	}

	h3 {
		margin: 0.75rem 0 0.4rem;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.08em;
		color: var(--muted);
	}
	h3:first-of-type {
		margin-top: 0;
	}

	.group {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	/* A fingertip's 24px, at every width (reachable.test.mjs). */
	button {
		box-sizing: border-box;
		min-block-size: 24px;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 999px;
		padding: 0.2rem 0.65rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}

	/* A layout reads as a layout: the root bar's own colour. */
	.layout {
		color: var(--accent);
	}

	button:hover,
	button:focus-visible {
		border-color: var(--accent);
		color: var(--ink-bright);
	}
</style>
