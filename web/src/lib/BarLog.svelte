<!--
	The log: the history ABOVE the bar's output section (da-ckw3, da-mwp4m).

	vividn, 2026-10-27: «keep the input bar on the bottom, and put the
	output/return bar above it again / make the CLI chronological order
	again». So it reads as a terminal does, top to bottom: the oldest
	command first and the newest last, nearest the input — the REPL
	COMPONENT, the same widget a layout can hold, drawn in its own order.

	`bare`, because a titled box over a reader's own prompt says "this is a
	widget" where it has to say "this is the rest of what you just saw" —
	vividn, 2026-09-21: "Remove the 'repl' header line."

	`skip={1}`, because the bar's OUTPUT SECTION is the newest row of this
	same list (PromptBar.svelte draws it with the same `ReplEntry`). vividn:
	"the output section … should actually *be* the first line of the repl …
	it shouldn't be repeated when you drag it up." So the log ends at the
	row before it, and what a reader pulls up is the continuation of what
	they were already looking at.

	ITS ROWS SCROLL ON THEIR OWN, pinned to the newest whenever a command
	lands, as a terminal's do. The bar's number opens it and the number no
	longer scrolls it (da-tj6m's phase 2 went with the order it was for):
	the handle and the full screen are da-0b2vy's.

	It mounts on the first gesture at the bar rather than at boot: until a
	finger or a wheel has touched the bar nobody has ever seen this box, and
	a scrollback rendered twice on every command is a cost paid for nothing.

	`close history` is PINNED TO THE BOTTOM OF THE BOX, just over the output
	section — vividn, 16:25, of the name it had then: "yes, return to top is
	pinned at the bottom". A history can be thousands of entries long, and a
	control at the end of one is a control nobody can reach.
-->
<script lang="ts">
	import { tick, untrack } from 'svelte';
	import Repl from '$lib/Repl.svelte';
	import { repl } from '$lib/replStore.js';
	import { armed, closeLog, inLog } from '$lib/log.js';

	let body: HTMLDivElement | undefined = $state();
	// A command that lands is the newest row, at the bottom: the list comes
	// back to it, wherever the reader had scrolled.
	$effect.pre(() => {
		void $repl.scrollback;
		const box = body;
		if (box) untrack(() => tick().then(() => (box.scrollTop = box.scrollHeight)));
	});
</script>

<!-- `inert` is for the REPL inside: `close history` is already out of the
     DOM until the log is reached, and a scrollback nobody can see must not
     be in the Tab order either (canary on PR #46). -->
<div class="log" class:reached={$inLog} inert={!$inLog}>
	{#if $armed}
		<Repl bare skip={1} bind:body />
	{/if}
	{#if $inLog}
		<button type="button" class="top" onclick={closeLog}>close history</button>
	{/if}
</div>

<style>
	.log {
		position: relative;
		display: flex;
		flex: none;
		/* As tall as the bar has been pulled past the notch, and no taller
		   than the page less the bar — vividn: "open the REPL section as far as
		   you want, not to full screen". At its tallest the dashboard is gone
		   and the log's top is the top of the page; its body scrolls. */
		height: var(--log-height, 0px);
		overflow: hidden;
		min-width: 0;
		background: var(--page);
	}

	.log :global(section.pane) {
		flex: 1;
		min-width: 0;
	}

	.top {
		position: absolute;
		left: 50%;
		bottom: 0.6rem;
		transform: translateX(-50%);
		z-index: 2;
		min-height: 24px;
		padding: 0.25rem 0.9rem;
		background: var(--ground);
		border: 1px solid var(--accent);
		border-radius: 999px;
		color: var(--accent-bright);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.top:hover,
	.top:focus-visible {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}
</style>
