<!--
	One tab's content inside its frame, mounted whether or not it is the tab
	in front (da-rjmg: "preload and keep fresh the display of all tabs in a
	widget even when not displayed so that swiping looks nicer and all the
	data is ready to go").

	Every tab is a box of the frame's content box's size: the one in front
	in the flow, filling it as its widget did when it was the only one
	mounted, and at the root no smaller than the split's legible floor
	(`least`), so the page scrolls exactly as far. A tab behind it is laid
	over the content and padded as it is, `visibility: hidden` rather than
	`display: none`, since a drawn widget measures its box to render — and
	is `inert`, so nothing in it takes focus, a key or a pointer. The one
	`beside` the tab in front is the neighbour a swipe brings on
	(da-24fgb): the frame shows it, a box away, while the strip moves
	(PaneFrame.svelte). It hears the epoch and the minute tick as the tab
	in front does, so a swipe shows a picture already drawn.

	A root tab behind (`kept`) is neither hidden nor inert (da-p77nk): an
	inherited change on a tab's box restyles everything in it, and for
	`main` on a phone at 4× each of the two flags cost about 50 ms of every
	switch, once for the tab going behind and once for the one coming on.
	So the root's tabs change nothing inherited as they come and go: one
	behind is clipped to nothing, which draws nothing and hits nothing, is
	`aria-hidden`, and hands on whatever focus reaches it (`refuse`). An
	inner pane's tabs keep the two flags.

	The frame's context is passed down through a gate: what a widget sends
	up to its frame — its own title controls (da-ndjo's `.own` box) and its
	alarm — reaches the frame from the tab in front only. Every tab is
	mounted, but only one is ACTIVE, and the portal keys on that.
-->
<script lang="ts">
	import { getContext, setContext, untrack, type Snippet } from 'svelte';
	import { derived, writable, type Writable } from 'svelte/store';

	interface Props {
		active: boolean;
		/** The tab a swipe in hand is bringing on. */
		beside?: boolean;
		/**
		 * Where the swipe's strip has moved this tab, while it moves
		 * (PaneFrame.svelte): an inline transform, so a move restyles this
		 * box alone and nothing inherits it (da-7etnf).
		 */
		moved?: string;
		/**
		 * How much smaller, each side, a root tab's layout lands than the one in
		 * front (`lands`, PaneFrame.svelte): behind, it keeps the box it is drawn
		 * in up front, so nothing in it is drawn again as it comes and goes. A
		 * larger one is grown inside its box, which clips it: a layout behind
		 * never widens the page's scroll. It is -1, 0 or 1 (`lands` is 0 or 1),
		 * so the growth is a class and not a custom property: one set on the
		 * tab's box restyled everything in it at every switch (da-p77nk).
		 */
		shy?: number;
		/**
		 * The root split's legible floor, which the tab in front holds as its
		 * own: an overflowing split inside a smaller box would lose the
		 * content's end padding from the page's scroll. A tab behind takes no
		 * floor, being clipped. In front or behind, the tab is a flex box, so a
		 * switch changes its `position` and not its `display`: going behind
		 * lays out the box and not the layout in it again (da-1acrc).
		 */
		least?: { width: number; height: number } | undefined;
		/** A root tab: behind, it is clipped rather than hidden or inert. */
		kept?: boolean;
		children?: Snippet;
	}

	let {
		active,
		beside = false,
		moved = undefined,
		shy = 0,
		least = undefined,
		kept = false,
		children
	}: Props = $props();

	interface Frame {
		alarm: Writable<boolean>;
		titleHost: Writable<HTMLElement | null>;
		hosted: Writable<boolean>;
		close: Writable<(() => void) | null>;
	}

	const frame = getContext<Frame>('layout-frame');
	const front = writable(untrack(() => active));
	const alarm = writable(false);
	const hosted = writable(false);
	$effect.pre(() => {
		front.set(active);
	});
	$effect.pre(() => {
		if (active) frame.alarm.set($alarm);
	});
	$effect.pre(() => {
		if (active) frame.hosted.set($hosted);
	});
	const TABBABLE = 'a[href], button, input, select, textarea, summary, [tabindex], [contenteditable]';
	let body: HTMLElement | undefined = $state();
	/**
	 * What `inert` does for an inner tab, for a root tab behind: focus that
	 * lands in it goes on to the nearest control outside it the way it was
	 * going — before it, if it came from after — or nowhere.
	 */
	function refuse(event: FocusEvent) {
		const from = event.relatedTarget instanceof Node ? event.relatedTarget : null;
		const back = from !== null && (body!.compareDocumentPosition(from) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
		const side = back ? Node.DOCUMENT_POSITION_PRECEDING : Node.DOCUMENT_POSITION_FOLLOWING;
		const open = [...document.querySelectorAll<HTMLElement>(TABBABLE)].filter(
			(el) =>
				el.tabIndex >= 0 &&
				!el.matches(':disabled') &&
				body!.compareDocumentPosition(el) & side &&
				!el.closest('[inert], [aria-hidden="true"]') &&
				el.checkVisibility({ visibilityProperty: true })
		);
		const to = back ? open.at(-1) : open[0];
		if (to) to.focus();
		else (event.target as HTMLElement).blur();
	}
	// And focus already in it as it goes behind, by a swipe or a script, is let go.
	$effect(() => {
		if (!active && kept && body?.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
	});
	setContext('layout-frame', {
		alarm,
		hosted,
		// A tab behind is inert, or clipped and refusing focus, so the frame's
		// close only ever hears the one in front.
		close: frame.close,
		titleHost: derived([frame.titleHost, front], ([host, on]) => (on ? host : null))
	});
</script>

<div
	class="tab-body"
	class:behind={!active}
	class:beside
	class:grown={!active && shy < 0}
	class:kept
	bind:this={body}
	inert={!active && !kept}
	aria-hidden={!active && kept ? 'true' : undefined}
	onfocusin={!active && kept ? refuse : undefined}
	style:min-width={active && least ? `${least.width}px` : null}
	style:min-height={active && least ? `${least.height}px` : null}
	style:transform={moved}
	style:margin={(!active && shy > 0) || (beside && moved) ? `${shy}px` : null}
>{@render children?.()}</div>

<style>
	.tab-body {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.behind {
		position: absolute;
		inset: 0;
		padding: inherit;
	}
	.behind:not(.kept) {
		visibility: hidden;
	}
	/* Drawn nowhere and hit by nothing, the neighbour too until the strip
	   moves it (PaneFrame.svelte). */
	.kept.behind {
		clip-path: inset(50%);
	}
	/* The tab body's clip is what bounds a grown layout: its children are
	   larger than it and not clipped themselves. */
	.behind:not(.beside) {
		overflow: clip;
	}
	.grown:not(.beside) > :global(*) {
		margin: -1px;
	}
</style>
