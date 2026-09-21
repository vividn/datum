<!--
	The fields widget as a widget kind.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (FieldsWidget.svelte), so the pane keeps
	whatever height its split ratios give it and a bar with more buttons
	than fit is reached by scrolling rather than by the pane growing
	(da-mbz2) — sideways at one or two rows high, up and down at three or
	more (da-ltux). The widget's panels hang UPWARD out of the bar and a
	scroller would clip them, so they are hung against the screen instead
	(`hang` in FieldsWidget.svelte). A press goes, with the bottom bar's
	pills after it, to the engine's `press`, which routes it through
	whichever REPL is on screen; a record's line goes to the bottom bar's
	prompt, unrun.

	`pending` is the verb the command line holds (da-ltux): it filters the
	list, and a press under it runs `<verb> <field>` at once and spends the
	verb — the box empties, and the list is whole again.

	The SELECTION is the other thing the widget answers (da-qu16): a `dur=`
	pill among the bar's pills leaves the fields a length can be spent on,
	and the lanes a press on a picture landed in bring those fields to the
	front. Both arrive here as stores and go in as plain props, so the
	widget itself asks nothing of the page.

	`showIgnored` (da-xhio) arrives as a prop too, though its value is the
	SESSION's rather than the layout's: the renderer merges the two before
	it draws (Layout.svelte, $lib/layout/session.ts), so neither this pane
	nor the widget below it knows the difference.
-->
<script lang="ts">
	import FieldsWidget from '$lib/FieldsWidget.svelte';
	import { clearPending, pending, pills, prefill } from '$lib/bar.js';
	import { hasDuration } from '$lib/commands.js';
	import type { SwitchLabel } from '$lib/fields.js';
	import { lanes } from '$lib/lanes.js';
	import { fields, pressWithPills, ready } from './engine.js';

	export let switchLabel: SwitchLabel = 'field';
	/**
	 * Whether the ignored fields get a button too (da-xhio) — this
	 * session's option, merged over the tree's settings by the renderer,
	 * so from here it is an ordinary prop like any other.
	 */
	export let showIgnored = false;

	function run(argv: string[]) {
		clearPending();
		void pressWithPills(argv);
	}
</script>

<div class="body">
	<FieldsWidget
		fields={$fields}
		disabled={!$ready}
		verb={$pending}
		duration={hasDuration($pills)}
		rank={$lanes}
		{switchLabel}
		{showIgnored}
		on:run={(e) => run(e.detail)}
		on:line={(e) => prefill.set({ line: e.detail, keyboard: true })}
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
