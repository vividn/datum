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
		 * How much smaller, each side, a root tab's layout lands than the one in
		 * front (`lands`, PaneFrame.svelte): behind, it keeps the box it is drawn
		 * in up front, so nothing in it is drawn again as it comes and goes. A
		 * larger one is grown inside its box, which clips it: a layout behind
		 * never widens the page's scroll.
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
		children?: Snippet;
	}

	let {
		active,
		beside = false,
		shy = 0,
		least = undefined,
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
	setContext('layout-frame', {
		alarm,
		hosted,
		// A tab behind is inert, so the frame's close only ever hears the one in front.
		close: frame.close,
		titleHost: derived([frame.titleHost, front], ([host, on]) => (on ? host : null))
	});
</script>

<div
	class="tab-body"
	class:behind={!active}
	class:beside
	class:grown={!active && shy < 0}
	inert={!active}
	style:min-width={active && least ? `${least.width}px` : null}
	style:min-height={active && least ? `${least.height}px` : null}
	style:margin={!active && shy > 0 ? `${shy}px` : null}
	style:--grown={!active && shy < 0 ? `${shy}px` : null}
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
		visibility: hidden;
	}
	/* The tab body's clip is what bounds a grown layout: its children are
	   larger than it and not clipped themselves. */
	.behind:not(.beside) {
		overflow: clip;
	}
	.grown:not(.beside) > :global(*) {
		margin: var(--grown);
	}
</style>
