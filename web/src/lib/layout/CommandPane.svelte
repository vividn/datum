<!--
	The commands widget as a widget kind (da-ltux), `command`.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CommandsWidget.svelte), because the
	scroll rule is about the bar's rows and the axis it scrolls on.

	`Undo` and `Redo` go through `press` and not `pressWithPills`: the
	pills are a time cursor for the log verbs and neither of these takes a
	time. Everything else only writes a line into the bottom bar's prompt,
	unrun, for a person to finish.
-->
<script lang="ts">
	import CommandsWidget from '$lib/CommandsWidget.svelte';
	import { prefill } from '$lib/bar.js';
	import { press, ready } from './engine.js';
</script>

<div class="body">
	<CommandsWidget
		disabled={!$ready}
		on:run={(e) => void press(e.detail)}
		on:line={(e) => prefill.set(e.detail)}
	/>
</div>

<style>
	.body {
		flex: 1;
		min-height: 0;
		display: flex;
		padding: 0.5rem 0.6rem;
	}
</style>
