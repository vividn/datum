<!--
	The fields widget as a widget kind.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: this body scrolls, so the pane keeps whatever height its split
	ratios give it and a bar with more buttons than fit is reached by
	scrolling rather than by the pane growing (da-mbz2). The widget's
	panels hang UPWARD out of the bar and a scroller would clip them, so
	they are hung against the screen instead (`hang` in
	FieldsWidget.svelte). A press goes, with the bottom bar's pills after
	it, to the engine's `press`, which routes it through whichever REPL is
	on screen; a record's line goes to the bottom bar's prompt, unrun.
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
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 0.5rem 0.6rem;
	}
</style>
