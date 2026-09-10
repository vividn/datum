<!--
	The day frame — the shell's single screen.

	It used to be a CSS grid with four fixed areas, laid out as the
	wireframe (da-hrz0) drew it. It is a layout tree now (da-jdop): panes
	holding widgets, split and tabbed and resized from each pane's `…`
	menu, with today's arrangement as the default (`$lib/layout/defaults`)
	and the reader's own kept as a draft (`$lib/layout/current`).

	Every widget still talks to datum through `run(argv)` in $lib/run.ts
	and nothing else. What this page owns is the boot — open the database —
	and the one refresh beat: the engine's epoch,
	bumped after every command here and after every write another tab
	makes, flows down the tree as a prop to every pane.
-->
<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Layout from '$lib/layout/Layout.svelte';
	import { layout, loaded } from '$lib/layout/current.js';
	import { boot, epoch, ran } from '$lib/layout/engine.js';
	import { onExternalChange } from '$lib/changes.js';

	// The tree itself is filled by the header's layout controls
	// (LayoutControls.svelte, da-wpgy): this device's draft, else the saved
	// layout, else today's page. `loaded` is their word that it has been.
	onMount(boot);

	// Another tab wrote. Same treatment as a local command finishing — bump
	// the epoch the panes hang off and rebuild the widget bar — so there is
	// one refresh path rather than two that can disagree.
	//
	// Subscribed here rather than from `onMount`: Svelte takes a teardown from
	// a SYNCHRONOUS onMount only, and an async one returns a promise it cannot
	// call. Safe during init because `onExternalChange` opens nothing outside
	// a browser (see $lib/changes.ts) and returns an empty teardown there.
	onDestroy(onExternalChange(() => void ran()));
</script>

<div class="frame">
	{#if $loaded}
		<Layout pane={$layout.root} epoch={$epoch} rootId={$layout.root.id} />
	{/if}
</div>

<style>
	.frame {
		display: flex;
		flex-direction: column;
		height: 100%;
	}
</style>
