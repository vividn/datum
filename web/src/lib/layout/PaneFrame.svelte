<!--
	The frame of one pane: its bar, its tabs, its `…` menu, and the swipe.

	A PANE is the holder (vividn's word); what it holds is in the slot. With
	one tab the title sits on the frame, as `Pane.svelte`'s `h2` always
	has; with more, a tab strip takes its place. The bar sits on the edge
	the pane's `bar` names — top or bottom running across, left or right
	running DOWN with the words rotated (`writing-mode: vertical-rl`), and
	on the left turned round so it reads bottom-to-top like a book's spine
	(vividn, 15:39) — with the title or the tabs gathered along it by
	`bar.align` and the menu always at the end.

	A pane whose tab holds a split rather than a widget is a HOLDER: no
	border, no background, and — with one untitled tab — a bar that is
	only the menu, so that the root pane costs the page a few pixels and
	not a title row. It is `section.holder` rather than `section.pane`
	so that "the panes" — as the test harness and the rest of the app
	find them — remain exactly the ones with a widget in them.

	Swiping across the content moves between tabs along the bar's axis:
	left/right under a horizontal bar, up/down under a vertical one. Touch
	only, with a threshold, and never over a scroll — the gesture starts
	only when nothing under the finger scrolls that way, so the strip's
	stack and a wide tail table keep their own scroll.
-->
<script lang="ts">
	import { setContext } from 'svelte';
	import { writable } from 'svelte/store';
	import type { BarAlign, BarEdge, Pane, Tab } from './types.js';
	import { isSplit } from './types.js';
	import { apply } from './current.js';
	import { addTab, closeTab, setActive, setBar, splitPane, splitTab } from './tree.js';
	import { titleOf } from './registry.js';
	import PaneMenu from './PaneMenu.svelte';

	export let pane: Pane;
	/** False on the last pane: the close item is greyed, not hidden. */
	export let canClose = true;
	/** The layout's root: the one pane whose content box is the scroller. */
	export let root = false;
	/**
	 * The active widget's kind. Its title goes on a wrapper round the frame
	 * as a class — `.repl`, `.tail`, `.dayview` — which is how the old
	 * grid's areas were named and how the rest of the app and its tests
	 * still find a pane's insides.
	 */
	export let kind: string | undefined = undefined;

	const EDGES: BarEdge[] = ['top', 'bottom', 'left', 'right'];
	const ALIGNS: BarAlign[] = ['start', 'center', 'end'];
	/** A swipe shorter than this is a touch, not a swipe. */
	const SWIPE_PX = 40;

	// What `Pane.svelte` looks for: it renders body-only inside a frame and
	// reports its alarm here, so the orange reaches whichever pane holds
	// the REPL.
	const alarm = writable(false);
	/** The h2, for a widget that draws its own title (the command pane's ✎). */
	const titleHost = writable<HTMLElement | null>(null);
	const hosted = writable(false);
	setContext('layout-frame', { alarm, titleHost, hosted });
	let h2: HTMLElement | null = null;
	$: titleHost.set(h2);

	$: active = pane.tabs[pane.active];
	$: holder = isSplit(active.content);
	$: many = pane.tabs.length > 1;
	$: vertical = pane.bar.edge === 'left' || pane.bar.edge === 'right';
	$: title = titleFor(active);
	/** A holder with one untitled tab has nothing to say but `…`. */
	$: compact = holder && !many && active.title === undefined;

	const titleFor = (tab: Tab): string =>
		tab.title ?? (isSplit(tab.content) ? 'split' : titleOf(tab.content.kind));

	$: items = [
		{ label: 'Add tab', run: () => apply((l) => addTab(l, pane.id)) },
		{ label: 'Split |', run: () => apply((l) => splitPane(l, pane.id, 'row')) },
		{ label: 'Split −', run: () => apply((l) => splitPane(l, pane.id, 'column')) },
		...(many
			? [
					{ label: 'Split Tab |', run: () => apply((l) => splitTab(l, pane.id, pane.active, 'row')) },
					{ label: 'Split Tab −', run: () => apply((l) => splitTab(l, pane.id, pane.active, 'column')) }
				]
			: []),
		{
			label: many ? 'Close tab' : 'Close pane',
			disabled: !canClose,
			run: () => apply((l) => closeTab(l, pane.id, pane.active))
		},
		...EDGES.map((edge) => ({
			label: `Bar: ${edge}`,
			checked: pane.bar.edge === edge,
			run: () => apply((l) => setBar(l, pane.id, { ...pane.bar, edge }))
		})),
		...ALIGNS.map((align) => ({
			label: `Align: ${align}`,
			checked: pane.bar.align === align,
			run: () => apply((l) => setBar(l, pane.id, { ...pane.bar, align }))
		}))
	];

	const activate = (index: number) => apply((l) => setActive(l, pane.id, index));

	/** Arrow keys walk the tab strip; the tab focused is the tab shown. */
	function onTabKey(event: KeyboardEvent, index: number) {
		const forward = vertical ? 'ArrowDown' : 'ArrowRight';
		const back = vertical ? 'ArrowUp' : 'ArrowLeft';
		const next = event.key === forward ? index + 1 : event.key === back ? index - 1 : null;
		if (next === null || next < 0 || next >= pane.tabs.length) return;
		event.preventDefault();
		activate(next);
		(event.currentTarget as HTMLElement).parentElement
			?.querySelectorAll<HTMLElement>('[role="tab"]')
			[next]?.focus();
	}

	// ---- swipe -----------------------------------------------------------

	let touch: { x: number; y: number; dead: boolean } | null = null;

	/** Does anything between `target` and `root` scroll along `axis` here? */
	function scrolls(target: EventTarget | null, root: HTMLElement, axis: 'x' | 'y'): boolean {
		for (let el = target as HTMLElement | null; el && el !== root; el = el.parentElement) {
			const style = getComputedStyle(el);
			const overflow = axis === 'x' ? style.overflowX : style.overflowY;
			if (overflow !== 'auto' && overflow !== 'scroll') continue;
			if (axis === 'x' ? el.scrollWidth > el.clientWidth : el.scrollHeight > el.clientHeight) return true;
		}
		return false;
	}

	function onTouchStart(event: TouchEvent) {
		const t = event.touches[0];
		touch = {
			x: t.clientX,
			y: t.clientY,
			dead:
				event.touches.length > 1 ||
				scrolls(event.target, event.currentTarget as HTMLElement, vertical ? 'y' : 'x')
		};
	}

	function onTouchEnd(event: TouchEvent) {
		const was = touch;
		touch = null;
		if (!was || was.dead) return;
		const t = event.changedTouches[0];
		const dx = t.clientX - was.x;
		const dy = t.clientY - was.y;
		const along = vertical ? dy : dx;
		const across = vertical ? dx : dy;
		if (Math.abs(along) < SWIPE_PX || Math.abs(across) > Math.abs(along)) return;
		// Pushing the content away from the bar's start is "next".
		const next = pane.active + (along < 0 ? 1 : -1);
		if (next >= 0 && next < pane.tabs.length) activate(next);
	}
</script>

<div class="host {kind === undefined ? '' : titleOf(kind)}">
<section
	class="{holder ? 'holder' : 'pane'} edge-{pane.bar.edge}"
	class:alarm={$alarm}
	class:vertical
	class:root
	data-pane={pane.id}
	data-title={title}
>
	<div class="titlebar" class:compact>
		<div class="along align-{pane.bar.align}">
			{#if many}
				<div class="tabs" role="tablist" aria-orientation={vertical ? 'vertical' : 'horizontal'}>
					{#each pane.tabs as tab, i (tab.id)}
						<button
							type="button"
							role="tab"
							aria-selected={i === pane.active}
							tabindex={i === pane.active ? 0 : -1}
							on:click={() => activate(i)}
							on:keydown={(e) => onTabKey(e, i)}>{titleFor(tab)}</button
						>
					{/each}
				</div>
			{:else if !compact}
				<h2 bind:this={h2}>{#if !$hosted}{title}{/if}</h2>
			{/if}
		</div>
		<PaneMenu {items} label="menu of the {title} pane" />
	</div>
	<div
		class="content"
		on:touchstart={many ? onTouchStart : undefined}
		on:touchend={many ? onTouchEnd : undefined}
		on:touchcancel={many ? () => (touch = null) : undefined}
	>
		<slot />
	</div>
</section>
</div>

<style>
	/* A name for the pane, not a box: the section is the box. */
	.host {
		display: contents;
	}

	section {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	.pane {
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
	}

	/* The bar's edge is the flex direction: the bar is always first in the
	   DOM (so it is first for the keyboard) and the direction puts it on
	   the edge asked for. */
	.edge-bottom {
		flex-direction: column-reverse;
	}
	.edge-left {
		flex-direction: row;
	}
	.edge-right {
		flex-direction: row-reverse;
	}

	.titlebar {
		display: flex;
		align-items: center;
		flex: none;
		min-width: 0;
		min-height: 0;
		border-bottom: 1px solid #1e1e38;
	}
	.edge-bottom > .titlebar {
		border-bottom: none;
		border-top: 1px solid #1e1e38;
	}
	.edge-left > .titlebar {
		border-bottom: none;
		border-right: 1px solid #1e1e38;
	}
	.edge-right > .titlebar {
		border-bottom: none;
		border-left: 1px solid #1e1e38;
	}

	/* Along the left and the right the words are rotated. `vertical-rl`
	   reads top-to-bottom, which is right for the right-hand edge; the
	   left-hand bar is turned round so it reads bottom-to-top, like the
	   spine of a book on a shelf (vividn, 15:39). A half turn maps the bar's
	   box onto itself, so nothing else moves. */
	.vertical > .titlebar {
		writing-mode: vertical-rl;
	}
	.edge-left > .titlebar {
		transform: rotate(180deg);
	}

	.holder > .titlebar {
		border: none;
	}
	.titlebar.compact {
		/* The root's whole cost: a hairline strip with `…` at its end. */
		min-height: 0.9rem;
		font-size: 0.7rem;
	}

	.along {
		flex: 1;
		display: flex;
		min-width: 0;
		min-height: 0;
	}
	.align-start {
		justify-content: flex-start;
	}
	.align-center {
		justify-content: center;
	}
	.align-end {
		justify-content: flex-end;
	}

	h2 {
		margin: 0;
		padding: 0.35rem 0.6rem;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: #7c7cff;
		white-space: nowrap;
	}

	.tabs {
		display: flex;
		min-width: 0;
		min-height: 0;
	}

	.tabs button {
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		padding: 0.3rem 0.6rem;
		font: inherit;
		font-size: 0.75rem;
		letter-spacing: 0.06em;
		color: #55558a;
		white-space: nowrap;
		cursor: pointer;
	}
	.vertical .tabs button {
		border-bottom: none;
		border-left: 2px solid transparent;
	}

	.tabs button[aria-selected='true'] {
		color: #7c7cff;
		border-color: #7c7cff;
	}

	.tabs button:hover,
	.tabs button:focus-visible {
		color: #adadff;
	}

	/* Orange rather than red: this is "your data is not safe yet", which
	   is a warning about the future, not a report of a failure that
	   already happened. It has to be visible at a glance from across a
	   desk. (From Pane.svelte, which drew it before there was a frame.) */
	.pane.alarm {
		border-color: #e8820c;
		box-shadow: 0 0 0 1px #e8820c inset;
	}
	.pane.alarm h2 {
		background: #e8820c;
		color: #1a1000;
	}

	/* The content box: `Pane.svelte` inside a frame is `display:
	   contents`, so its scrolling body and its footer are this box's
	   own flex items — the body takes the room, the footer stays put. */
	.content {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	/* Only the root scrolls. A split whose children's floors do not fit
	   overflows into it and the page scrolls to the rest — while the
	   widget bar's panels, which hang upward out of their pane
	   (WidgetBar.svelte), stay unclipped at every level in between. */
	.root > .content {
		overflow: auto;
	}
</style>
