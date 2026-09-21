<!--
	The log: what is under the bar, below the fold (da-ckw3).

	vividn, 2026-09-17: "have the REPL be in reverse order […] the ability to
	go back in time to view previous commands and their outputs. […] When
	scrolled down into the REPL, put a (return to top) at the bottom, which
	restores back to the dashboard with the input section down in its normal
	place."

	So it is the REPL COMPONENT — the same widget a layout can hold, reading
	the same scrollback store — drawn `reversed` (da-gvdc): newest first,
	each entry keeping its own order, the view pinned to the top. Not a
	second rendering of it and not a second reversal; the reversal already
	exists and it keys its entries by the ENTRY, which is what keeps a run
	from rebuilding the whole scrollback (review on #33).

	`bare`, because a titled box under a reader's own prompt says "this is a
	widget" where it has to say "this is the rest of what you just saw" —
	vividn, 2026-09-21: "Remove the 'repl' header line."

	`skip={1}`, because the bar's OUTPUT SECTION is row one of this same
	list (PromptBar.svelte draws it with the same `ReplEntry`). vividn: "the
	output section … should actually *be* the first line of the repl … it
	shouldn't be repeated when you drag it up." So the log starts at row
	two, and what a reader drags up is the continuation of what they were
	already looking at.

	It mounts on the first gesture at the bar rather than at boot: until a
	finger or a wheel has touched the bar nobody has ever seen this box, and
	a scrollback rendered twice on every command is a cost paid for nothing.
	The box's own height is fixed in CSS, so nothing above it moves when it
	fills.

	`close history` is PINNED TO THE BOTTOM OF THE SCREEN — vividn, 16:25, of
	the name it had then: "yes, return to top is pinned at the bottom". A
	history can be thousands
	of entries long, and a control at the end of one is a control nobody can
	reach. The box's bottom edge IS the screen's bottom edge once the page
	has come to rest in the log ($lib/log.ts has that arithmetic), so the
	button is simply at the bottom of the box.
-->
<script lang="ts">
	import Repl from '$lib/Repl.svelte';
	import { armed, closeLog, inLog } from '$lib/log.js';
</script>

<!-- `inert` is for the REPL inside: `close history` is already out of the
     DOM until the log is reached, and a scrollback nobody can see must not
     be in the Tab order either (canary on PR #46). -->
<div class="log" class:reached={$inLog} inert={!$inLog}>
	{#if $armed}
		<Repl reversed bare skip={1} />
	{/if}
	{#if $inLog}
		<button type="button" class="top" on:click={closeLog}>close history</button>
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
		   and the bar's top is the top of the page; past that the pane's own
		   body scrolls. */
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
