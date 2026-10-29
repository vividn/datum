<!--
	The log: the history ABOVE the bar's output section (da-ckw3, da-mwp4m).

	vividn, 2026-10-27: «keep the input bar on the bottom, and put the
	output/return bar above it again / make the CLI chronological order
	again». So it reads as a terminal does, top to bottom: the oldest
	command first and the newest last, nearest the input — the CLI widget
	COMPONENT, the same widget a layout can hold, drawn in its own order.

	`bare`, because a titled box over a reader's own prompt says "this is a
	widget" where it has to say "this is the rest of what you just saw" —
	vividn, 2026-09-21: "Remove the 'repl' header line."

	`skip={1}`, because the bar's OUTPUT SECTION is the newest row of this
	same list (PromptBar.svelte draws it with the same `CliEntry`). vividn:
	"the output section … should actually *be* the first line of the repl …
	it shouldn't be repeated when you drag it up." So the log ends at the
	row before it, and what a reader pulls up is the continuation of what
	they were already looking at. FULL, the strip is gone and the log draws
	every row, the newest its last line, next to the input (da-0b2vy), ONE
	COMMAND A LINE (`lines`): vividn, 2026-10-27, «Don't but multiple
	commands on the same line like currently, each command get's it's own
	line».

	ITS ROWS SCROLL ON THEIR OWN, pinned to the newest whenever a command
	lands, as a terminal's do. The bar's number opens it and the number no
	longer scrolls it (da-tj6m's phase 2 went with the order it was for).
	AT THE TOP OF THE HISTORY, ONE MORE SCROLL UP — a wheel turned up, a
	finger swiped down, as a gesture of its own — collapses it to the row:
	vividn, 2026-10-27, «scrolling scrolls through the history of the CLI
	until you reach the end of the history at which point scrolling
	up/swiping down again closes the cli back».

	It mounts the first time the bar opens past the notch rather than at
	boot: until then nobody has ever seen this box, and a scrollback
	rendered twice on every command is a cost paid for nothing.

	`close history` is PINNED TO THE BOTTOM OF THE BOX, just over the output
	section — vividn, 16:25, of the name it had then: "yes, return to top is
	pinned at the bottom". A history can be thousands of entries long, and a
	control at the end of one is a control nobody can reach.
-->
<script lang="ts">
	import { tick, untrack } from 'svelte';
	import CliScrollback from '#lib/CliScrollback.svelte';
	import { cli, stepped } from '#lib/cliStore.ts';
	import { armed, closeLog, full, inLog } from '#lib/log.ts';

	let body: HTMLDivElement | undefined = $state();

	/** A wheel this soon after the last is the same gesture (#lib/grip.ts's `INERTIA_MS`). */
	const SAME_MS = 150;
	/** How far a finger moves before it is a swipe (#lib/grip.ts's `DRAG_START`). */
	const SWIPE = 8;
	let wheeled = -Infinity;
	/** Where a touch that began at the top of the history began, or null. */
	let fromTop: number | null = null;

	/**
	 * The strip's newest and every button's undo or redo after it, which the
	 * strip draws as marks on it (da-25q3k): the row before them is the log's last.
	 */
	const skip = $derived.by(() => {
		const all = $cli.scrollback;
		let n = 1;
		while (n < all.length && stepped(all[all.length - n])) n += 1;
		return n;
	});

	const atTop = (): boolean => $full && !!body && body.scrollTop <= 0;

	/** A wheel up that STARTS at the top: the scroll that reached the top is not it. */
	function onwheel(event: WheelEvent): void {
		const fresh = event.timeStamp - wheeled > SAME_MS;
		wheeled = event.timeStamp;
		if (fresh && event.deltaY < 0 && atTop()) closeLog();
	}

	function ontouchstart(event: TouchEvent): void {
		fromTop = atTop() ? event.touches[0].clientY : null;
	}

	function ontouchmove(event: TouchEvent): void {
		if (fromTop === null || event.touches[0].clientY - fromTop < SWIPE) return;
		fromTop = null;
		closeLog();
	}

	// Gone full, the log has one more row, the newest: back to it.
	$effect(() => {
		if ($full && body) body.scrollTop = body.scrollHeight;
	});
	// A command that lands is the newest row, at the bottom: the list comes
	// back to it, wherever the reader had scrolled.
	$effect.pre(() => {
		void $cli.scrollback;
		const box = body;
		if (box) untrack(() => tick().then(() => (box.scrollTop = box.scrollHeight)));
	});
</script>

<!-- `inert` is for the CLI widget inside: `close history` is already out of the
     DOM until the log is reached, and a scrollback nobody can see must not
     be in the Tab order either (canary on PR #46). -->
<!-- The touch handlers only watch a scroll for its overscroll; Escape and
     `close history` are the keyboard's way back. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="log" class:reached={$inLog} class:full={$full} inert={!$inLog} {onwheel} {ontouchstart} {ontouchmove}>
	{#if $armed}
		<CliScrollback bare skip={$full ? 0 : skip} lines bind:body />
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

	/* And at the foot for `close history`, so the newest line is never under it. */
	.log.full :global(.body) {
		padding-bottom: 2.75rem;
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
