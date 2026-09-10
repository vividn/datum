<!--
	The quick-log bar as a widget kind.

	Its own body rather than `Pane.svelte`'s, because the bar's panels hang
	UPWARD out of it (WidgetBar.svelte, da-hkpd) and a scrolling body would
	clip them: this body never scrolls, and the pane takes the bar's height
	instead (`fit` in registry.ts). A press goes to the engine's `press`,
	which routes it through whichever REPL is on screen.
-->
<script lang="ts">
	import WidgetBar from '$lib/WidgetBar.svelte';
	import { fields, press, ready } from './engine.js';
</script>

<div class="body">
	<WidgetBar widgets={$fields} disabled={!$ready} on:run={(e) => void press(e.detail)} />
</div>

<style>
	.body {
		padding: 0.5rem 0.6rem;
		/* Held open so the pane stops above the bottom edge rather than
		   snapping taller once the bar has buttons in it. A floor only. */
		min-height: 2rem;
	}
</style>
