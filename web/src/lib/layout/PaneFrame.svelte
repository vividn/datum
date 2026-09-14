<!--
	The frame of one pane: its bar, its tabs, the menu its TITLE opens, and
	the swipe.

	A PANE is the holder (vividn's word); what it holds is in the slot. With
	one tab there is no bar at all: the title sits ON the frame's outline,
	small and quiet, straddling the line the way a legend does, and the
	`+` is a badge right after it, on the same line (vividn, 17:56 — "just
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
	is not there, since the children's own titles can split them (vividn's
	second look, da-2a5y: "they have their own ... button floating there,
	but aren't really necessary"). With two or more tabs it keeps its tab
	strip, as a row of its own, and the `+` after the last tab. It is
	`section.holder` rather than `section.pane` so that "the panes" — as
	the test harness and the rest of the app find them — remain exactly
	the ones with a thing in them.

	A tab carrying a LAYOUT is such a thing, whatever shape its content
	has (da-9puz): the layout is the boundary vividn drew, so it draws the
	frame and wears the layout's name (`labelOf`, registry.ts — the rule
	the root bar's tabs read too), where a holder made by splitting is
	scaffolding with no name of its own. Without that, choosing a layout
	of several panes for a tab spilled them loose into its space,
	indistinguishable from the panes around them. The panes inside keep
	their own frames, so the layout's is a second, outer line — which is
	what says where it begins and ends. The ROOT pane is the exception at
	the other end: it draws no bar of its own at all, however many tabs
	it has, and no box, because the shell draws the root's tabs as the
	page's top bar (da-0a5o). And a LOCKED frame
	draws no menu — nothing under a lock can be split, tabbed or resized,
	and the tabs still switch — nor a `+`, and its title is not a button.

	A layout tab's frame is also where that layout's identity is worked
	(da-crp5): the four acts the root bar draws for a root tab — save,
	restore, save as, lock — are in this menu when the tab holds a layout,
	whatever its depth, and the orange modified dot follows the title or
	the tab. The one exception to the lock above: a layout tab that is
	itself locked keeps a menu holding those acts alone, so `Unlock
	layout` can be reached.

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
	import { setContext, tick } from 'svelte';
	import { writable } from 'svelte/store';
	import type { BarAlign, BarEdge, Layout, Pane } from './types.js';
	import { isSplit } from './types.js';
	import { apply, layout } from './current.js';
	import { choose, fillTab, put } from './chooser.js';
	import { ancestorsOf, modified, restore, save, saveAs, toggleLock } from './shell.js';
	import type { Toward } from './tree.js';
	import { addTab, breakOut, canBreakOut, closeTab, newTab, setActive, setBar, splitPane } from './tree.js';
	import { labelOf, titleOf } from './registry.js';
	import PaneMenu from './PaneMenu.svelte';

	export let pane: Pane;
	/** False on the last pane: the close item is greyed, not hidden. */
	export let canClose = true;
	/** The layout's root: the one pane whose content box is the scroller. */
	export let root = false;
	/**
	 * Whether a tab ABOVE this pane is locked. The active tab's own lock is
	 * added to it below: under either there is no `+` and none of the pane
	 * items, and the tab strip still switches — but a layout tab that is
	 * itself locked keeps the menu holding its `Unlock layout` (da-crp5),
	 * or a lock would be a thing nobody could undo.
	 */
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

	/**
	 * The four arrows, in the one order, and where each one puts a pane on
	 * the screen — a rotated bar does not turn them, since the menu is
	 * drawn on `document.body` and reads the screen's way (PaneMenu.svelte).
	 */
	const ARROWS: { toward: Toward; text: string; where: string }[] = [
		{ toward: 'left', text: '←', where: 'to the left' },
		{ toward: 'right', text: '→', where: 'to the right' },
		{ toward: 'up', text: '↑', where: 'above' },
		{ toward: 'down', text: '↓', where: 'below' }
	];

	/** One menu item's row: the same operation aimed four ways, each named in full. */
	const aimed = (what: string, op: (layout: Layout, toward: Toward) => Layout) =>
		ARROWS.map(({ toward, text, where }) => ({
			text,
			title: `${what} ${where}`,
			run: () => apply((l) => op(l, toward))
		}));

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
	/**
	 * Scaffolding, not a thing: a split someone made by splitting. A tab
	 * carrying a LAYOUT is not one however its content is shaped (da-9puz)
	 * — the layout is the boundary, so it draws the frame and wears the
	 * name, instead of spilling its panes loose among the panes around it.
	 * The page root is the exception at the other end: the shell draws its
	 * identity (its tabs, its name, its acts), so the root is never a box.
	 */
	$: holder = isSplit(active.content) && (root || active.layout === undefined);
	/** A frame round a LAYOUT's panes: they are inset from it, so the two outlines read as two. */
	$: nested = isSplit(active.content) && !holder;
	$: many = pane.tabs.length > 1;
	$: vertical = pane.bar.edge === 'left' || pane.bar.edge === 'right';
	/**
	 * `labelOf` (registry.ts), the one rule — except at the root, which
	 * wears no name of its own: its tab always carries a layout, and the
	 * shell already wears that name on the root bar (da-0a5o). So the root
	 * is called what it SHOWS, which is what `data-title` means everywhere
	 * the shell is not the one naming the pane.
	 */
	$: title = labelOf(root ? { ...active, layout: undefined } : active);
	/** No bar at all: the root's is the shell's, a lone holder's is its children's. */
	$: bare = root || (!many && holder);
	/** Whether this pane has a group to leave: everything but the root's own pane. */
	$: canBreak = canBreakOut($layout, pane.id);

	/** This pane's own lock: one above it, or its active tab's own. */
	$: lockedHere = locked || !!active.locked;

	/**
	 * A failed act: reported where a developer sees it. The root bar has a
	 * line for the active layout's errors; a pane's bar is one row of words
	 * with no room for one, and the act is the reader's own press, so it
	 * must not throw into the page either.
	 */
	const attempt = (job: Promise<unknown>) => void job.catch((e) => console.error(e));

	/**
	 * The four acts on the layout this tab HOLDS (da-crp5) — the same four
	 * the root bar draws, since a nested copy is no different in kind. Not
	 * offered under a lock from above; the tab's OWN lock leaves them, so
	 * that `Unlock layout` can be reached.
	 */
	$: layoutActs =
		active.layout === undefined || locked
			? []
			: [
					{ label: 'Save layout', run: () => attempt(save(active.id)) },
					{ label: 'Restore layout', run: () => attempt(restore(active.id)) },
					{
						label: 'Save layout as…',
						run: () => {
							const to = prompt('Save this layout as:', '')?.trim();
							if (to) attempt(saveAs(active.id, to));
						}
					},
					{ label: active.locked ? 'Unlock layout' : 'Lock layout', run: () => toggleLock(active.id) }
				];

	$: paneItems = [
		{ label: 'Choose…', run: () => void fillTab(active.id) },
		// A holder's children split themselves; its own split is a tab's.
		...(holder ? [] : [{ label: 'Split', row: aimed('split: a new pane', (l, t) => splitPane(l, pane.id, t)) }]),
		...(canBreak ? [{ label: 'Break out', row: aimed('break out: this widget', (l, t) => breakOut(l, pane.id, t)) }] : []),
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

	$: items = lockedHere ? layoutActs : [...paneItems, ...layoutActs];

	/**
	 * The `+`: the chooser, then a tab holding what it came back with —
	 * dismissed, it creates nothing. The names open above the new tab are
	 * the ones above its sibling, less that sibling's own.
	 */
	async function addChosen(): Promise<void> {
		const picked = await choose(ancestorsOf(active.id).filter((name) => name !== active.layout));
		if (!picked) return;
		const tab = newTab();
		apply((l) => addTab(l, pane.id, tab));
		await put(tab.id, picked);
	}

	const activate = (index: number) => apply((l) => setActive(l, pane.id, index));

	/** The tab strip, to put the focus back on the tab switched to. */
	let strip: HTMLElement | null = null;

	/**
	 * Arrow keys walk the tab strip; the tab focused is the tab shown.
	 *
	 * After the tick, since switching swaps which tab is the menu's button
	 * (da-9vtw) and so replaces two of the strip's elements: the tab to
	 * focus is the one that is there once it has.
	 */
	async function onTabKey(event: KeyboardEvent, index: number) {
		const forward = vertical ? 'ArrowDown' : 'ArrowRight';
		const back = vertical ? 'ArrowUp' : 'ArrowLeft';
		const next = event.key === forward ? index + 1 : event.key === back ? index - 1 : null;
		if (next === null || next < 0 || next >= pane.tabs.length) return;
		event.preventDefault();
		activate(next);
		await tick();
		strip?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
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
	class:nested
	class:bare
	class:clip={!root && kind !== undefined}
	data-pane={pane.id}
	data-title={title}
	style:--row="{ROW}px"
>
	{#if !bare}
	<div class="titlebar">
		<div class="along align-{pane.bar.align}">
			{#if many}
				<div
					class="tabs"
					role="tablist"
					aria-orientation={vertical ? 'vertical' : 'horizontal'}
					bind:this={strip}
				>
					{#each pane.tabs as tab, i (tab.id)}
						{#if i === pane.active && items.length > 0}
							<!-- The tab in front is the menu's button; the others switch. -->
							<PaneMenu
								{items}
								look="tab"
								label="menu of the {labelOf(tab)} tab"
								title="tab menu"
								role="tab"
								aria-selected="true"
								tabindex="0"
								on:keydown={(e) => onTabKey(e, i)}
								>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</PaneMenu
							>
						{:else}
							<button
								type="button"
								role="tab"
								class="control tab"
								class:modified={$modified.has(tab.id)}
								aria-selected={i === pane.active}
								tabindex={i === pane.active ? 0 : -1}
								on:click={() => activate(i)}
								on:keydown={(e) => onTabKey(e, i)}
								>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</button
							>
						{/if}
					{/each}
				</div>
			{:else}
				<!-- The title's own box, and the menu's button laid over the
				     whole of it — beside the h2 rather than inside it, so that
				     the words may be clipped to their box and the button not. -->
				<div class="titlebox">
					<h2 bind:this={h2}>{#if !$hosted}{title}{#if $modified.has(active.id)}<span class="dot" title="modified">•</span>{/if}{/if}</h2>
					{#if items.length > 0}
						<PaneMenu {items} look="title" label="menu of the {title} pane" />
					{/if}
				</div>
			{/if}
			<!-- Inside `.along`, after the title or the last tab: the badge
			     goes where the alignment puts them, and right beside them. -->
			{#if !lockedHere}
				<button
					type="button"
					class="control badge add"
					aria-label="add a tab to the {title} pane"
					title="add a tab"
					on:click={() => void addChosen()}>+</button
				>
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
		background: var(--ground);
		border: 1px solid var(--rule);
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

	/* A holder's bar is a row of its own, and the titles of the panes under
	   it straddle the top of the content right below it (da-om9v): the row
	   stays above them, or the tab in front — the button that opens the
	   menu (da-9vtw) — is half covered by the first child's title. */
	.holder > .titlebar {
		position: relative;
		z-index: 3;
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
	/* The box the title is in: the words, and over the whole of them the
	   button that opens the menu (`.title`, PaneMenu.svelte). It is what
	   positions that button, and is exactly the h2 — which is why the
	   button is not inside the h2, whose overflow is hidden to clip the
	   words and would clip the button's outer pixels with them. */
	.titlebox {
		position: relative;
		display: flex;
		flex: 0 1 auto;
		min-inline-size: 0;
		min-block-size: 0;
	}

	/* The words and a line round them, the frame's own: nothing across
	   the bar but the outline, whichever way the bar runs. A widget's own
	   controls in here (the command pane's ✎) are lifted above the button
	   laid over them, and are `:global` because the widget draws them. */
	h2 {
		box-sizing: border-box;
		margin: 0;
		padding-block: 0;
		padding-inline: 0.4rem;
		/* The row's height, and a finger's where the pointer is coarse — the
		   title is a button now (da-9vtw), and a button is on the floor
		   `reachable.test.mjs` holds the badges to. */
		block-size: var(--badge);
		line-height: calc(var(--badge) - 2px);
		min-inline-size: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: var(--accent);
		white-space: nowrap;
		pointer-events: auto;
		/* Its own ground, so the outline stops at the words rather than
		   running through them. */
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 4px;
	}

	h2 :global(button),
	h2 :global(input) {
		position: relative;
		z-index: 1;
	}

	/* The modified dot (da-crp5): the same mark the root bar puts on a
	   modified layout's tab, in the same orange, wherever the copy is
	   drawn. `:global` on the tabs, since the tab in front is PaneMenu's
	   button and carries no scope of this file's. */
	.dot,
	.tabs :global(.dot) {
		margin-inline-start: 0.3rem;
		color: var(--warn);
	}

	/* The whole title says it can be pressed. */
	h2:hover {
		color: var(--accent-bright);
		border-color: var(--accent);
	}

	/* The `+` does not stretch, and takes the pointer where the line it
	   sits on does not. */
	.add {
		flex: none;
		pointer-events: auto;
	}

	.tabs {
		display: flex;
		gap: 4px;
		min-width: 0;
		min-height: 0;
		pointer-events: auto;
	}

	/* `.control` (app.css) is the pill. On the line a tab is the title's
	   shape and height, and a smaller radius than a control gets elsewhere
	   — the line it straddles is 1px, and 6px of round on a 16px box reads
	   as a lozenge. `:global`, and under `.tabs` so that it reaches no
	   further: the tab in front is drawn by PaneMenu, being the button
	   that opens the menu (da-9vtw), and carries no scope of this file's. */
	.tabs :global(.tab) {
		block-size: var(--row);
		padding-block: 0;
		padding-inline: 0.5rem;
		border-radius: 4px;
		font-size: 0.75rem;
		line-height: 1;
		letter-spacing: 0.06em;
		white-space: nowrap;
	}

	/* The tab in front says so with its outline, which is why the hover
	   below is here and not on `.control`: it has to be able to argue with
	   this rule, and it can only do that from inside the component. */
	.tabs :global(.tab[aria-selected='true']) {
		color: var(--accent);
		border-color: var(--accent);
	}

	.tabs :global(.tab:hover),
	.tabs :global(.tab:focus-visible) {
		color: var(--accent-bright);
	}

	/* Orange rather than red: this is "your data is not safe yet", which
	   is a warning about the future, not a report of a failure that
	   already happened. It has to be visible at a glance from across a
	   desk. (From Pane.svelte, which drew it before there was a frame.) */
	.pane.alarm {
		border-color: var(--alarm);
		box-shadow: 0 0 0 1px var(--alarm) inset;
	}
	.pane.alarm h2 {
		background: var(--alarm);
		border-color: var(--alarm);
		/* Not a token: the almost-black the alarm's orange is legible on,
		   and nothing else in the app is painted on orange. */
		color: #1a1000;
	}

	/* The content box: `Pane.svelte` inside a frame is `display:
	   contents`, so its scrolling body is this box's own flex item and
	   takes the room. */
	.content {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	/* A pane dragged down to its frame (da-zh2o) shows nothing of its
	   widget outside it — the body's own padding included. A widget with
	   more in it than the box holds scrolls inside its own body. */
	.clip > .content {
		overflow: hidden;
	}

	/* Only the root scrolls, of the frames: a split whose children's
	   floors do not fit overflows into it and the page scrolls to the
	   rest. A widget's own body may scroll inside its pane. */
	.root > .content {
		overflow: auto;
	}
	/* A layout's panes are inset from its frame: the page's own gap all
	   round (`BORDER`, Layout.svelte — half a row), so the two outlines
	   read as two rather than as one thick line, and the WHOLE row on the
	   bar's edge, where both the layout's title and its first pane's
	   straddle their own outline and each reaches half a row over the
	   other's. Without it they land on each other and the layout's name is
	   not there at all (da-9puz). `chromeOf` counts this. */
	.pane.nested > .content {
		padding: calc(var(--row) / 2);
	}
	.pane.nested.edge-top > .content {
		padding-top: var(--row);
	}
	.pane.nested.edge-bottom > .content {
		padding-bottom: var(--row);
	}
	.pane.nested.edge-left > .content {
		padding-left: var(--row);
	}
	.pane.nested.edge-right > .content {
		padding-right: var(--row);
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
