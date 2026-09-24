<!--
	The commands widget as a widget kind (da-ltux), `command`.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (CommandsWidget.svelte), because the
	scroll rule is about the bar's rows and the axis it scrolls on.

	A tap that RUNS — `Undo`, `Redo`, and whatever the loaded pills
	complete (`fires` in $lib/commands.ts, da-3lxol: `delete` with an hid
	selected) — goes through `pressWithPills`, which hands the command only
	the pills its grammar takes: a selected hid (`undo <hid>`), a time for
	`retime`. Everything else only writes a line into the bottom bar's
	prompt, unrun, for a person to finish. The loaded pills decide which
	buttons are active and in what order (da-r13j). A line whose next step
	is a tap is focused without a phone's keyboard (`next` in
	$lib/commands.ts, da-k34p).

	`spec` with a FIELD pill loaded opens the spec modal on that field at
	once (da-ybai9, vividn: «no need for sub commands or pressing enter. It
	should just work»): the pill named the subject, so it is spent at the
	tap, like any command's. Without one, `spec ` goes on the line as ever.
-->
<script lang="ts">
	import CommandsWidget from '$lib/CommandsWidget.svelte';
	import { clearUnlockedPills, pills, prefill, text } from '$lib/bar.js';
	import { openSpec } from '$lib/specModal.js';
	import { grammar, press, pressWithPills, ready } from './engine.js';

	function runNow(argv: string[]): void {
		// A thumb that tapped the command before its pills were all loaded
		// has no Enter: the tap that fires takes back the `<name> ` the
		// first one put there.
		if ($text === `${argv[0]} `) $text = '';
		void pressWithPills(argv);
	}

	function fill({ name, line, keyboard }: { name: string; line: string; keyboard: boolean }): void {
		const field = $pills.find((p) => p.kind === 'field');
		if (name === 'spec' && field) {
			clearUnlockedPills([field]);
			void openSpec(field.text).then((argv) => argv && press(argv));
		} else prefill.set({ line, keyboard });
	}
</script>

<div class="body">
	<CommandsWidget
		disabled={!$ready}
		grammar={$grammar}
		pills={$pills}
		on:run={(e) => runNow(e.detail)}
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
