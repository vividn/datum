<!--
	The debug widget (da-o2wm8): instruments for the things a synthetic
	drive cannot judge.

	vividn, 2026-09-21: "A db named DEBUG, which has a layout preloaded into
	it this DEBUG widget. It only gets loaded for the DEBUG db. how about
	that. That seems safer and also easier to access for me. The DEBUG
	widget is only available from the DEBUG db, and is one of the premade
	widgets. Or we could even have different DEBUG widgets eventually if
	needed."

	So the gate is WHICH DATABASE YOU ARE IN and not what you typed, and it
	stands in two places for two different doors:

	  - the CHOOSER does not offer the kind outside `DEBUG` (`db` on the
	    kind, `pickableIn` in layout/registry.ts), which is the door a
	    reader comes through;
	  - and THIS component says so in words when it is drawn there anyway,
	    which is the other door: a saved layout lives in `_internal` and is
	    the DEVICE's, not the database's, so a layout holding a debug pane
	    opens in every database this device visits. ⚑ An empty pane and a
	    crash were the two other possible answers and neither is one: a
	    pane that says where its contents live is the only one a reader can
	    act on.

	Each tool is a `debugKind` line in the registry naming one of the
	components below: `debug`, the bottom bar's two knobs, and `seed`, a
	sample week (da-6ecm5). The gate is written once, here.
-->
<script lang="ts">
	import Pane from './Pane.svelte';
	import DebugBar from './DebugBar.svelte';
	import DebugSeed from './DebugSeed.svelte';
	import { currentDb } from './database.js';
	import { DEBUG_DB } from './layout/defaults.js';

	/**
	 * The database this page runs against. Read once, at mount: `?db=` is
	 * the whole of that state (da-vk1x) and changing it is a navigation,
	 * which builds the tree again.
	 */
	const here = currentDb();

	/** Which debug widget this is: the kind, and the heading and component it draws. */
	export let kind: 'debug' | 'seed' = 'debug';
	const TOOLS = { debug: ['bar', DebugBar], seed: ['sample data', DebugSeed] } as const;
	$: [heading, tool] = TOOLS[kind];
</script>

<!-- `.debug-widget` is how this widget is FOUND, here and in the tests: a
     one-widget layout is lifted into its root tab (`inline`, shell.ts), so
     the pane drawing it is `shell` and its `data-title` is whichever tab is
     in front — neither of which names the widget. -->
<Pane title={kind}>
	<div class="debug-widget" data-kind={kind}>
		{#if here === DEBUG_DB}
			<h3>{heading}</h3>
			<svelte:component this={tool} />
		{:else}
			<p class="elsewhere">
				the debug widgets live in the <code>{DEBUG_DB}</code> database — this page is on
				<code>{here}</code>. Open <code>?db={DEBUG_DB}</code> to use them.
			</p>
		{/if}
	</div>
</Pane>

<style>
	h3 {
		margin: 0 0 0.35rem;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.08em;
		color: var(--accent);
	}
	.elsewhere {
		margin: 0;
		color: var(--quiet);
	}
</style>
