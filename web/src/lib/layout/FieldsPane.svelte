<!--
	The fields widget as a widget kind.

	Its own body rather than `Pane.svelte`'s, for the padding and nothing
	else: the BAR is the scroller (FieldsWidget.svelte), so the pane keeps
	whatever height its split ratios give it and a bar with more buttons
	than fit is reached by scrolling rather than by the pane growing
	(da-mbz2) — sideways, on as many rows as fit (da-vu7pi, da-arms4,
	`ButtonRows.svelte`). The widget's panels hang UPWARD out of the bar and a
	scroller would clip them, so they are hung against the screen instead
	(`hang` in FieldsWidget.svelte). A press goes, with the bottom bar's
	pills after it, to the engine's `press`, which routes it through
	whichever REPL is on screen; a record's line, and a log with keys to
	type (da-chml5), go to the bottom bar's prompt, unrun.

	`pending` is the verb the command line holds (da-ltux): it filters the
	list, and a press under it runs `<verb> <field>` at once — with the
	flags the line carries between them (da-bbmpx) — and spends the verb:
	the box empties, and the list is whole again.

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
	import { clearPending, pending, pills, prefill, text } from '$lib/bar.js';
	import { flagLine, hasDuration } from '$lib/commands.js';
	import { advance } from '$lib/keyframes.js';
	import type { SwitchLabel } from '$lib/fields.js';
	import { lanes } from '$lib/lanes.js';
	import { shellJoin, shellSplit } from '$lib/shellSplit.js';
	import { fields, grammar, pressWithPills, ready, standings } from './engine.js';

	export let switchLabel: SwitchLabel = 'field';
	/**
	 * Whether the ignored fields get a button too (da-xhio) — this
	 * session's option, merged over the tree's settings by the renderer,
	 * so from here it is an ordinary prop like any other.
	 */
	export let showIgnored = false;

	/**
	 * A press under the pending verb runs the LINE with the field after it
	 * — `occur -q ` and coffee is `occur -q coffee` — and spends it
	 * (da-bbmpx); a line `flagLine` refuses keeps today's `occur coffee`.
	 */
	function run(argv: string[]) {
		const line = pendingFlags(argv);
		if (line) $text = '';
		else clearPending();
		void pressWithPills(line ? [...line, ...argv.slice(1)] : argv);
	}

	/** The pending line's flags, when `argv` spends its verb and it holds only flags. */
	const pendingFlags = (argv: string[]): string[] | null =>
		argv[0] === $pending ? flagLine(shellSplit($text), $grammar?.get(argv[0])) : null;

	/**
	 * The same line, left unrun and straight into its first key's frame
	 * (da-31d1d). The flags go AFTER the field, where the CLI takes them as
	 * well: the frames read the field as the second word.
	 */
	function fill(argv: string[]) {
		const line = shellJoin([...argv, ...(pendingFlags(argv)?.slice(1) ?? [])]);
		prefill.set({ line: advance(line, $fields) ?? `${line} `, keyboard: true });
	}
</script>

<div class="body">
	<FieldsWidget
		fields={$fields}
		disabled={!$ready}
		verb={$pending}
		duration={hasDuration($pills)}
		rank={$lanes}
		standings={$standings}
		{switchLabel}
		{showIgnored}
		on:run={(e) => run(e.detail)}
		on:fill={(e) => fill(e.detail)}
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
