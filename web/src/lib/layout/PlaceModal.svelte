<!--
	A place on a desktop (da-jozgv): the widget kind `openPlace` names,
	drawn in a modal over the page. vividn, 2026-09-29: «I agree that is
	should always be a widget, that can be opened in a new tab etc, but I
	would still have it be a modal on desktop».

	The body is the one widget the place's layout holds — its kind's own
	component with its settings, what the tab would show — so there is one
	widget and one set of tests and only the frame differs. A place is a
	NAME, not a kind: `dayview` is a `daystrip`. The frame is $lib/Sheet.svelte:
	Escape, the backdrop and the × close it, and `Open as tab` hands it to
	the transient tab a phone gets, from where `Keep tab` keeps it.
-->
<script lang="ts">
	import Sheet from '$lib/Sheet.svelte';
	import { act } from '$lib/act.js';
	import { epoch } from './engine.js';
	import { KINDS } from './registry.js';
	import { closeModalPlace, modalPlace, openPlace } from './shell.js';

	const kind = $derived($modalPlace === null ? undefined : KINDS[$modalPlace.widget.kind]);
	// The place's own until the widget moves one (a derived written to), and the place's again once it changes.
	let settings: Record<string, unknown> = $derived($modalPlace?.widget.settings ?? {});
</script>

{#if kind && $modalPlace}
	{@const place = $modalPlace.name}
	<Sheet title={place} widget open data-place={place} onclose={closeModalPlace}>
		{#snippet head()}
			<button type="button" class="as-tab" use:act={'open'} onclick={() => void openPlace(place, true)}
				>Open as tab</button
			>
		{/snippet}
		<kind.component
			{...kind.props ? kind.props(settings) : { settings }}
			epoch={$epoch}
			on:change={(e) => kind?.onChange && (settings = kind.onChange(settings, e.detail))}
		/>
	</Sheet>
{/if}

<style>
	/* The sheet's head names the place already; the widget's own title
	   would say it twice. */
	:global(dialog.sheet[data-place] section.pane > h2) {
		display: none;
	}

	/* The widget fills the body, as a pane fills its frame: a box that
	   followed its content would move the spec form's Done from tab to tab
	   (da-laxpn, da-hbrfg). */
	:global(dialog.sheet[data-place] .sheet-body > section.pane) {
		flex: 1;
	}

	.as-tab {
		background: none;
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.5rem;
		font: inherit;
		color: var(--quiet);
		cursor: pointer;
	}

	.as-tab:hover,
	.as-tab:focus-visible {
		color: var(--ink-bright);
	}
</style>
