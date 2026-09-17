<!--
	The commands widget as a widget kind (da-ltux), `command`.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CommandsWidget.svelte), because the
	scroll rule is about the bar's rows and the axis it scrolls on.

	`Undo` and `Redo` go through `pressWithPills`, which hands a command
	only the pills its grammar takes: a selected hid (`undo <hid>`), never
	the time cursor. Everything else only writes a line into the bottom
	bar's prompt, unrun, for a person to finish. The loaded pills decide
	which buttons are active and in what order (da-r13j).
-->
<script lang="ts">
	import CommandsWidget from '$lib/CommandsWidget.svelte';
	import { pills, prefill } from '$lib/bar.js';
	import { grammar, pressWithPills, ready } from './engine.js';
</script>

<div class="body">
	<CommandsWidget
		disabled={!$ready}
		grammar={$grammar}
		pills={$pills}
		on:run={(e) => void pressWithPills(e.detail)}
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
