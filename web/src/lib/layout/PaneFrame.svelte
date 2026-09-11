<!--
	The frame of one pane: its bar, its tabs, its `…` menu, and the swipe.

	A PANE is the holder (vividn's word); what it holds is in the slot. With
	one tab there is no bar at all: the title sits ON the frame's outline,
	small and quiet, straddling the line the way a legend does, and the
	`…` is a badge right after it, on the same line (vividn, 17:56 — "just
	overlay the title on the frame, with the nice ... button visible next
	to it", da-jgrn). With more tabs the tab strip sits there instead and
	the badge follows the LAST tab. The line is the edge the pane's `bar`
	names — top or bottom running across, left or right running DOWN with
	the words rotated (`writing-mode: vertical-rl`), and on the left turned
	round so it reads bottom-to-top like a book's spine (vividn, 15:39) —
	with the title or the tabs gathered along it by `bar.align`. The title,
	the tabs and the badge are centred ON the outline, half out and half
	in, and the content is padded by nothing: the row lies over the top
	strip of what is underneath (vividn's third look, da-13bc — "just overlay
	it directly on the frame […] it can overlap slightly with the underlying
	display"). The row is as tall as its words and outlined like the frame
	(vividn's fourth look, da-om9v — "the widget titles bleed too much into
	other frames"): `ROW`, 16px, so the 8px that hang out fit the 8px gap
	and nothing lies over the neighbour. The badge is the row's height too,
	except under a coarse pointer, where a finger needs 24px and 4px of it
	still reach over. The bar itself takes no pointer, so the border
	between two panes is still grabbed right beside it.

	A pane whose tab holds a split rather than a widget is a HOLDER: no
	border, no background, and — with one tab — no bar at all: a box that
	is not there, since the children's own `…` can split them (vividn's
	second look, da-2a5y: "they have their own ... button floating there,
	but aren't really necessary"). With two or more tabs it keeps its tab
	strip, as a row of its own, and the `…` after the last tab. It is
	`section.holder` rather than `section.pane` so that "the panes" — as
	the test harness and the rest of the app find them — remain exactly
	the ones with a widget in them. The ROOT pane draws no bar of its own
	at all, however many tabs it has: the shell draws the root's tabs as
	the page's top bar (da-0a5o). And a LOCKED frame
	draws no `…` — nothing under a lock can be split, tabbed or resized,
	and the tabs still switch.

	Swiping across the content moves between tabs along the bar's axis:
	left/right under a horizontal bar, up/down under a vertical one. Touch
	only, with a threshold, and never over a scroll — the gesture starts
	only when nothing under the finger scrolls that way, so the strip's
	stack and a wide tail table keep their own scroll.
-->
<script lang="ts" context="module">
	/**
	 * The bar's row, in pixels: the title's words (0.75rem at a line of
	 * 14px) and its 1px outline. The frame's `--row` and Layout.svelte's
	 * chrome both read it, so the floors follow the row drawn. Half of it
	 * hangs outside the outline, and must fit the gap between panes
	 * (Layout.svelte's `BORDER`).
	 */
	export const ROW = 16;
</script>

<script lang="ts">
	import { setContext } from 'svelte';
	import { writable } from 'svelte/store';
	import type { BarAlign, BarEdge, Pane, Tab } from './types.js';
	import { isSplit } from './types.js';
	import { apply } from './current.js';
	import { addTab, closeTab, setActive, setBar, splitPane, splitTab } from './tree.js';
	import { KINDS, titleOf } from './registry.js';
	import PaneMenu from './PaneMenu.svelte';

	export let pane: Pane;
	/** False on the last pane: the close item is greyed, not hidden. */
	export let canClose = true;
	/** The layout's root: the one pane whose content box is the scroller. */
	export let root = false;
	/** Under a locked tab: no `…`; the tab strip still switches. */
	export let locked = false;
	/**
	 * The active widget's kind. Its title goes on a wrapper round the frame
	 * as a class — `.repl`, `.tail`, `.dayview` — which is how the old
	 * grid's areas were named and how the rest of the app and its tests
	 * still find a pane's insides.
	 */
	export let kind: string | undefined = undefined;

	const EDGES: BarEdge[] = ['top', 'bottom', 'left', 'right'];
	const ALIGNS: BarAlign[] = ['start', 'center', 'end'];
	/**
	 * Where `start`, `center` and `end` put the title along each edge, as
	 * the eye sees it: the left-hand bar is turned round, so its start is
	 * at the bottom.
	 */
	const SEEN: Record<BarEdge, string[]> = {
		top: ['left', 'center', 'right'],
		bottom: ['left', 'center', 'right'],
		left: ['bottom', 'middle', 'top'],
		right: ['top', 'middle', 'bottom']
	};
	const AT: Record<string, number> = { left: 3, top: 3, center: 5.5, middle: 5.5, right: 8, bottom: 8 };
	/** The Bar submenu's icon: a thick bar `long` px on `edge` of a 16px square, `at` px along it. */
	function mark(edge: BarEdge, at = 2, long = 12): number[] {
		const across = edge === 'top' || edge === 'left' ? 2 : 11;
		return edge === 'top' || edge === 'bottom' ? [at, across, long, 3] : [across, at, 3, long];
	}
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
	/** No bar at all: the root's is the shell's, a lone holder's is its children's. */
	$: bare = root || (!many && holder);

	const titleFor = (tab: Tab): string =>
		tab.title ?? (isSplit(tab.content) ? 'split' : titleOf(tab.content.kind));

	$: items = [
		{ label: 'Add tab', run: () => apply((l) => addTab(l, pane.id)) },
		// A holder's children split themselves; its own split is a tab's.
		...(holder
			? []
			: [
					{
						label: 'Split',
						row: [
							{ text: '|', title: 'split side by side', run: () => apply((l) => splitPane(l, pane.id, 'row')) },
							{ text: '−', title: 'split top and bottom', run: () => apply((l) => splitPane(l, pane.id, 'column')) }
						]
					}
				]),
		...(many
			? [
					{
						label: 'Split Tab',
						row: [
							{ text: '|', title: 'split tab side by side', run: () => apply((l) => splitTab(l, pane.id, pane.active, 'row')) },
							{ text: '−', title: 'split tab top and bottom', run: () => apply((l) => splitTab(l, pane.id, pane.active, 'column')) }
						]
					}
				]
			: []),
		{
			label: many ? 'Close tab' : 'Close pane',
			disabled: !canClose,
			run: () => apply((l) => closeTab(l, pane.id, pane.active))
		},
		{
			label: 'Bar',
			sub: [
				EDGES.map((edge) => ({
					label: edge,
					title: `bar on the ${edge}`,
					icon: mark(edge),
					checked: pane.bar.edge === edge,
					run: () => apply((l) => setBar(l, pane.id, { ...pane.bar, edge }))
				})),
				ALIGNS.map((align, i) => {
					const where = SEEN[pane.bar.edge][i];
					return {
						label: align,
						title: `align to the ${align} (${where})`,
						icon: mark(pane.bar.edge, AT[where], 5),
						checked: pane.bar.align === align,
						run: () => apply((l) => setBar(l, pane.id, { ...pane.bar, align }))
					};
				})
			]
		}
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
	class:bare
	class:clip={!root && kind !== undefined && !KINDS[kind]?.fit}
	data-pane={pane.id}
	data-title={title}
	style:--row="{ROW}px"
>
	{#if !bare}
	<div class="titlebar">
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
			{:else}
				<h2 bind:this={h2}>{#if !$hosted}{title}{/if}</h2>
			{/if}
			<!-- Inside `.along`, after the title or the last tab: the badge
			     goes where the alignment puts them, and right beside them. -->
			{#if !locked}
				<PaneMenu {items} label="menu of the {title} pane" />
			{/if}
		</div>
	</div>
	{/if}
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
		/* The title, the tabs and the badge are all `--row` tall (`ROW`, set
		   on the element) and hang half of it outside the outline and half
		   over the content (Layout.svelte's chrome). The badge alone keeps a
		   finger's 24px where the pointer is one (reachable.test.mjs). */
		--badge: var(--row);
	}
	@media (pointer: coarse) {
		section {
			--badge: 24px;
		}
	}

	.pane {
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
	}

	/* A holder has no outline to sit on, so its bar stays a row of its own.
	   The bar's edge is the flex direction: the bar is always first in the
	   DOM (so it is first for the keyboard) and the direction puts it on
	   the edge asked for. */
	.holder.edge-bottom {
		flex-direction: column-reverse;
	}
	.holder.edge-left {
		flex-direction: row;
	}
	.holder.edge-right {
		flex-direction: row-reverse;
	}

	.titlebar {
		display: flex;
		align-items: center;
		flex: none;
		min-width: 0;
		min-height: 0;
	}

	/* On a pane the bar is a LINE, not a box: zero thick, laid down the
	   middle of the 1px outline (half a pixel out from the padding box),
	   with its items centred on it — so they straddle the outline, as much
	   out as in. It takes no pointer itself (the border beside it must
	   stay grabbable); its items do. */
	.pane > .titlebar {
		position: absolute;
		z-index: 2;
		pointer-events: none;
	}
	.pane.edge-top > .titlebar {
		top: -0.5px;
		left: 0;
		right: 0;
		height: 0;
	}
	.pane.edge-bottom > .titlebar {
		bottom: -0.5px;
		left: 0;
		right: 0;
		height: 0;
	}
	.pane.edge-left > .titlebar {
		left: -0.5px;
		top: 0;
		bottom: 0;
		width: 0;
	}
	.pane.edge-right > .titlebar {
		right: -0.5px;
		top: 0;
		bottom: 0;
		width: 0;
	}

	/* How far the title lies over the content on its edge, for a widget
	   whose own controls sit in that corner to keep clear of it (the
	   nowview's steps). The display underneath is not moved. */
	.pane.edge-top:not(.bare) {
		--under-top: calc(var(--row) / 2);
	}
	.pane.edge-left:not(.bare) {
		--under-left: calc(var(--row) / 2);
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

	.along {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		min-height: 0;
		pointer-events: none;
		/* Off the rounded corners. Logical: along the bar, whichever way it runs. */
		padding-inline: 0.5rem;
	}
	.holder > .titlebar > .along {
		padding-inline: 0;
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
	/* The words and a line round them, the frame's own: nothing across
	   the bar but the outline, whichever way the bar runs. */
	h2 {
		box-sizing: border-box;
		margin: 0;
		padding-block: 0;
		padding-inline: 0.4rem;
		block-size: var(--row);
		line-height: calc(var(--row) - 2px);
		min-inline-size: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: #7c7cff;
		white-space: nowrap;
		pointer-events: auto;
		/* Its own ground, so the outline stops at the words rather than
		   running through them. */
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
	}

	.tabs {
		display: flex;
		gap: 4px;
		min-width: 0;
		min-height: 0;
		pointer-events: auto;
	}

	/* Each tab a small pill on the line, the title's shape and height. */
	.tabs button {
		box-sizing: border-box;
		block-size: var(--row);
		padding-block: 0;
		padding-inline: 0.5rem;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
		font: inherit;
		font-size: 0.75rem;
		line-height: 1;
		letter-spacing: 0.06em;
		color: #55558a;
		white-space: nowrap;
		cursor: pointer;
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
		border-color: #e8820c;
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

	/* A pane dragged down to its frame (da-zh2o) shows nothing of its
	   widget outside it — the body's own padding included. The widget
	   bar is the exception: it sizes itself, so it is never squeezed, and
	   its panels hang upward out of its pane. */
	.clip > .content {
		overflow: hidden;
	}

	/* Only the root scrolls. A split whose children's floors do not fit
	   overflows into it and the page scrolls to the rest — while the
	   widget bar's panels, which hang upward out of their pane
	   (WidgetBar.svelte), stay unclipped at every level in between. */
	.root > .content {
		overflow: auto;
	}
	/* The titles along the page's own edges hang half their row outside
	   their panes — their badges half theirs — and the scroller would cut
	   that half off: it reaches out over the page's margin by that much
	   and pads it back, so the panes stay where they were and their titles
	   are whole. */
	.holder.root.bare > .content {
		margin: calc(max(var(--row), var(--badge)) / -2);
		padding: calc(max(var(--row), var(--badge)) / 2);
	}
</style>
