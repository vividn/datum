<!--
	The commands widget as a widget kind (da-ltux), `command`.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CommandsWidget.svelte), because the
	scroll rule is about the bar's rows and the axis it scrolls on.

	`Undo` and `Redo` go through `pressWithPills`, which hands a command
	only the pills its grammar takes: a selected hid (`undo <hid>`), never
	the time cursor. Everything else only writes a line into the bottom
	bar's prompt, unrun, for a person to finish. The loaded pills decide
	which buttons are active and in what order (da-r13j). A line whose next
	step is a tap is focused without a phone's keyboard (`next` in
	$lib/commands.ts, da-k34p).

	`retime` with BOTH an hid and a time pill loaded is complete, so the tap
	runs it (vividn, 09-17: «retime should just fire when you tap it»). Only
	retime: `delete` keeps its Enter, and `get` / `history` were not asked.
-->
<script lang="ts">
	import CommandsWidget from '$lib/CommandsWidget.svelte';
	import { pills, prefill, text } from '$lib/bar.js';
	import { grammar, pressWithPills, ready } from './engine.js';

	function fill({ name, line, keyboard }: { name: string; line: string; keyboard: boolean }): void {
		const loaded = (kind: string) => $pills.some((p) => p.kind === kind);
		if (name === 'retime' && loaded('hid') && loaded('time')) {
			// A thumb that tapped retime before its second pill has no Enter:
			// the tap that fires takes back the `retime ` the first one put there.
			if ($text === line) $text = '';
			void pressWithPills(['retime']);
		} else prefill.set({ line, keyboard });
	}
</script>

<div class="body">
	<CommandsWidget
		disabled={!$ready}
		grammar={$grammar}
		pills={$pills}
		on:run={(e) => void pressWithPills(e.detail)}
		on:line={(e) => fill(e.detail)}
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
