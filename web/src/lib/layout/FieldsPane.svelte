<!--
	The fields widget as a widget kind.

	Its own body rather than `Pane.svelte`'s, because the widget's panels
	hang UPWARD out of it (FieldsWidget.svelte, da-hkpd) and a scrolling body
	would clip them: this body never scrolls, and the pane takes the
	widget's height instead (`fit` in registry.ts). A press goes, with the
	bottom bar's pills after it, to the engine's `press`, which routes it
	through whichever REPL is on screen; a record's line goes to the bottom
	bar's prompt, unrun.
-->
<script lang="ts">
	import FieldsWidget from '$lib/FieldsWidget.svelte';
	import { prefill } from '$lib/bar.js';
	import type { SwitchLabel } from '$lib/fields.js';
	import { fields, pressWithPills, ready } from './engine.js';

	export let switchLabel: SwitchLabel = 'field';
</script>

<div class="body">
	<FieldsWidget
		fields={$fields}
		disabled={!$ready}
		{switchLabel}
		on:run={(e) => void pressWithPills(e.detail)}
		on:line={(e) => prefill.set(e.detail)}
	/>
</div>

<style>
	.body {
		padding: 0.5rem 0.6rem;
		/* Held open so the pane stops above the bottom edge rather than
		   snapping taller once the bar has buttons in it. A floor only. */
		min-height: 2rem;
	}
</style>
