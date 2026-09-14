<!--
	The frame — the shell's single screen (`/`).

	It used to be a CSS grid with four fixed areas, laid out as the
	wireframe (da-hrz0) drew it. It is a layout tree now (da-jdop): panes
	holding widgets, split and tabbed and resized from each pane's `…`
	menu, with today's arrangement as the default (`$lib/layout/defaults`)
	and the reader's own kept as a draft (`$lib/layout/current`). Its root
	pane's tabs are the page's top bar (da-0a5o, `$lib/layout/RootBar`),
	each one a layout, and the shell (`$lib/layout/shell`) fills the tree
	on boot — from `+layout.svelte`, so `loaded` is already on its way by
	the time this mounts.

	Every widget still talks to datum through `run(argv)` in $lib/run.ts
	and nothing else. What this page owns is the one refresh beat: the
	engine's epoch, bumped after every command here and after every write
	another tab makes, flows down the tree as a prop to every pane.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import Layout from '$lib/layout/Layout.svelte';
	import { layout, loaded } from '$lib/layout/current.js';
	import { epoch, ran } from '$lib/layout/engine.js';
	import { onExternalChange } from '$lib/changes.js';

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
		position: relative;
		display: flex;
		flex-direction: column;
		height: 100%;
	}

</style>
