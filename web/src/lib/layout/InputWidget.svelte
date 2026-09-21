<!--
	The input widget (da-0dhc, back as da-3zon): the page's command line,
	drawn in a pane.

	vividn: «Let's bring back the input widget, I'm thinking about positioning
	the input box somewhere else on desktop, it's too invisible there. But
	keep the fixed input bar on the bottom still for now.» So this is about
	PLACEMENT and nothing else — it is not a second command line. The box is
	`text` in $lib/bar.ts and the Enter is `submitLine`, the very stores and
	function the bottom bar's prompt uses (Prompt.svelte): type here and the
	bar shows it, type there and this shows it, and a line runs once from
	whichever box the reader pressed Enter in.

	What it holds is the bar's two tiers and only those: the RECEIPT of the
	last command run anywhere (BarResult.svelte, `last`) above, and the
	prompt row — caret, box, pills — below. Not the `^`, not the REPL
	overlay and not an output pane: those are the bar's, and a widget
	wanting the scrollback is the `repl` widget, which is a different kind.

	It does NOT register with the engine (`registerRepl`): it has nowhere to
	put a command's output, so a widget-bar press still goes to the REPL
	when one is mounted and runs through the seam when none is. What it does
	claim is the FOCUS — `Prompt`'s `widget` prop — so a line the fields
	widget prefills lands in the box the reader put on the page rather than
	in the bar at the foot of it.

	The receipt takes whatever height the pane has left; the prompt row is
	pinned at the bottom, because a command line you cannot see is not one.
	`reversed` (its option, da-gvdc) turns the two round: the prompt row
	pinned at the top and the receipt under it — the same buffer, pills and
	Enter, only where the rows sit.
-->
<script lang="ts">
	import BarResult from '$lib/BarResult.svelte';
	import { last } from '$lib/bar.js';
	import Prompt from '$lib/Prompt.svelte';

	/** The prompt row on top, the receipt under it. */
	export let reversed = false;
</script>

<section class="input-widget" class:reversed>
	<div class="receipt">
		<BarResult result={$last?.result ?? null} />
	</div>
	<Prompt widget />
</section>

<style>
	.input-widget {
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		min-width: 0;
		min-height: 0;
	}

	.reversed {
		flex-direction: column-reverse;
	}

	/* As the bar's top tier is: it shrinks away before the prompt does. */
	.receipt {
		display: flex;
		align-items: center;
		flex: 1 1 auto;
		min-height: 0;
	}

	.receipt :global(.result) {
		flex: 1;
		height: 100%;
	}

	/* The receipt stands above the prompt row here, so this is the surface
	   that rules it off (da-ckw3 moved the declaration out of Prompt.svelte,
	   which cannot know what is above it).

	   In the default order only. REVERSED the column turns over, the line is
	   at the widget's TOP and its border-top is drawn on that outside edge,
	   with nothing where the two actually meet — measured by vremya on PR
	   #46: the line at y 52–88, the receipt at 88–131. That is what the
	   widget did before this bead too, so nothing here changed it; whether a
	   reversed widget wants a seam of its own is da-gvdc's to say, not
	   this bead's. */
	.input-widget :global(.line) {
		flex: none;
		border-top: 1px solid var(--tint);
	}
</style>
