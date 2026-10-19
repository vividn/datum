<!--
	The one modal that fills a tab (da-uyx1): the layouts a tab could hold,
	then the widget kinds. An empty pane's «Change Widget» button opens it,
	and so does the root bar's `+` — one component, one ordering, wherever
	it is opened. Its caller is `chooser.ts`, which is what the pane menu
	and the root menu call.

	«Change Widget» is vividn's word for it (da-bzhq), and it is the same
	word on the menu item, on the empty pane's button and on the heading
	here — `CHANGE` in `menu.ts`, so there is one string. ⚑ It is NARROWER
	than this dialog, which offers the Layouts group below as well; said
	rather than silently widened.

	The order: the layouts `chooser.ts` handed it (`main`, then the rest by
	device, less what is open above the tab), and then every kind this
	DATABASE offers by its title, in the registry's order — `pickableIn`,
	which is every kind but the ones another database gates (da-o2wm8:
	"The DEBUG widget is only available from the DEBUG db"). After them,
	the widgets this database's kits ship (da-gh37k), each wearing its kit's
	name as a kit's layout does — none from the root bar's `+`, which opens
	a named tab, and a kit's widget is named there by its kit's layout.

	A layout says where it came from (da-gg0v): `data-source` is
	`builtin`, `user` or `kit:<name>`, and a kit's layout wears the kit's
	name beside its own, so a list that joins two stores does not read as
	one flat pile of names.

	The `<dialog>` pattern is Confirm.svelte's: `showModal` puts focus
	inside and Escape — or a click on the backdrop, which lands on the
	dialog itself — closes it and returns focus to whatever opened it,
	changing nothing.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { CHANGE } from './menu.js';
	import { currentDb } from '$lib/database.js';
	import { pickableIn, titleOf } from './registry.js';
	import type { Choice, Offer } from './chooser.js';
	import type { KitWidget } from './store.js';

	/** The layouts this tab could hold, in order (chooser.ts). */
	export let layouts: Offer[] = [];
	/** The widgets this database's kits ship (da-gh37k), each the `kit` kind. */
	export let widgets: (KitWidget & { kit: string })[] = [];

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
	aria-label={CHANGE}
	bind:this={dialog}
	on:close={() => dispatch('close', chosen)}
	on:click={(e) => e.target === dialog && dialog.close()}
>
	<h2>{CHANGE}</h2>
	<!-- Nothing to offer, nothing to head: inside `main`, with nothing else
	     saved, the layouts section is empty and is not drawn. -->
	{#if layouts.length}
		<h3>Layouts</h3>
		<div class="group" role="group" aria-label="Layouts">
			{#each layouts as { name, source } (name)}
				<button
					type="button"
					class="layout"
					data-source={source}
					on:click={() => pick({ layout: name })}
				>
					<span class="name">{name}</span>{#if source.startsWith('kit:')}<span class="from"
							>{source.slice(4)}</span
						>{/if}
				</button>
			{/each}
		</div>
	{/if}
	<h3>Widgets</h3>
	<div class="group" role="group" aria-label="Widgets">
		{#each pickableIn(currentDb()) as kind (kind)}
			<button type="button" class="widget" on:click={() => pick({ kind })}>{titleOf(kind)}</button>
		{/each}
		{#each widgets as { kit, name, title } (`${kit}/${name}`)}
			<button
				type="button"
				class="widget"
				data-source="kit:{kit}"
				on:click={() => pick({ kind: 'kit', settings: { kit, widget: name, ...(title ? { title } : {}) } })}
				><span class="name">{title ?? name}</span><span class="from">{kit}</span></button
			>
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

	/* The kit a layout came in with, quieter than the name it goes by. */
	.from {
		margin-inline-start: 0.35rem;
		color: var(--muted);
	}

	button:hover,
	button:focus-visible {
		border-color: var(--accent);
		color: var(--ink-bright);
	}
</style>
