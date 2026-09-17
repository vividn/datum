<!--
	The layout engine's renderer: one pane, recursively.

	A pane is a `PaneFrame` (bar, tabs, menu) around its active tab. The
	tab holds a widget — drawn by the kind's registered component — or a
	split, whose children are this component again, laid along the split's
	direction with a draggable BORDER between each pair.

	Borders: pointer events with capture, so a fast drag does not lose the
	border; `touch-action: none` on the border only, so a finger dragging
	it is not a scroll. While the border is held the split re-lays out
	with CSS alone — the ratios being dragged are a local preview on the
	cells' `--ratio`, and the tree is not written — and the drop applies
	them once. Every CLI pane re-runs a WASI process per size it is shown
	at, so the panes are told a drag is on (`resizing`, layout/resizing.ts)
	and keep their last picture, scaled, until the drop (da-jgrn). The
	keyboard has the border too: it is focusable, `role="separator"` with
	the ratio as its value, and the arrows along the split's axis move it
	by 2%.

	Locked (da-2a5y): a tab's `locked` covers its whole subtree — the
	flag is threaded down as a prop, and under it every border is `static`
	(no handle, no keyboard) and every frame draws no `…`. The frame is
	told the lock from ABOVE it and adds its own tab's, so it can tell the
	two apart: a layout tab that is itself locked still shows the menu its
	`Unlock layout` is in (da-crp5).

	Floors: a child's cell is never narrower or shorter than its frame
	(`floorOf` over `chromeOf`: a widget pane's outline and the inside
	half of its title's row, a tabbed holder's row of tabs and the
	half-row its children's titles hang into under it, and nothing
	for a holder with one tab, which draws no bar at all — each on the
	edge the bar is on, so a left-hand title adds width and no height,
	da-13bc) — the kinds have no floor of their own (da-zh2o), so a pane
	drags down to its frame and a split too small for even that overflows
	to the root's scroll rather than hiding one.
	The root's own split takes a second floor, `legible`: every kind's
	legible box laid out as the tree lays it (the same `floorOf`, with the
	registry's `legibleOfKind`), so a screen too small for the panes
	scrolls the page instead of squeezing them. A squeezed pane still
	counts in that sum and a closed one does not: to fit a phone without
	scrolling, close a pane; to give a sibling a pane's room, squeeze it.
	Snaps: a kind may name content sizes (`snapsOfKind` — the strip's is
	one whole day); a drag whose preview comes within `SNAP_PX` of one,
	on either side of the border, lands on it, and past that carries on.
	The arrows do not snap. Every pane takes the share of the split its
	ratio gives it — none sizes itself to its content (da-mbz2), so a
	widget with more in it than its box holds scrolls inside it.
-->
<script lang="ts">
	import Pane from '$lib/Pane.svelte';
	import PaneFrame, { ROW } from './PaneFrame.svelte';
	import TabBody from './TabBody.svelte';
	import { fillTab } from './chooser.js';
	import type { NodeId, Pane as PaneNode, Split } from './types.js';
	import { isSplit } from './types.js';
	import { apply } from './current.js';
	import { EMPTY_KIND, floorOf, rootFloorOf, setRatios, setWidget } from './tree.js';
	import { maximized, maximizedIn } from './maximize.js';
	import { KINDS, legibleOfKind, snapsOfKind } from './registry.js';
	import { resizing } from './resizing.js';

	export let pane: PaneNode;
	export let epoch = 0;
	/** The root pane's id: the one pane whose last tab cannot be closed. */
	export let rootId: NodeId;
	/** Whether a tab above this pane is locked; its own active tab may add to it. */
	export let locked = false;

	/**
	 * The border between two children, in pixels — the gap between panes,
	 * and the whole of it is the handle. Halved from the old grid's 12
	 * (vividn: "reduce margins some"), to the floor a finger needs. The half
	 * of a title's row that hangs outside its pane (`ROW / 2`) fits in it.
	 */
	const BORDER = 8;
	/** A widget pane's outline, 1px a side. */
	const OUTLINE = 2;
	/** What one arrow press moves a border. */
	const NUDGE = 0.02;
	/**
	 * How near a dragged extent comes to a snap, in whole pixels, before it
	 * lands on it — to the nearest pixel, since the extent is worked out
	 * through the pair's ratio and 8 can come back as 8.000001.
	 */
	const SNAP_PX = 8;
	const NONE = { width: 0, height: 0 };

	$: active = pane.tabs[pane.active];
	$: content = active.content;
	$: canClose = !(pane.id === rootId && pane.tabs.length === 1);
	/**
	 * The maximized pane (da-00am), resolved HERE and nowhere else: the root
	 * render is the one place that substitutes a subtree, so there is no
	 * `maximized` prop threaded down every level. Null unless this is the
	 * root, the state belongs to the tab in front, and the pane is still in
	 * its tree — which is how a closed or broken-out pane heals itself.
	 */
	$: big = pane.id === rootId ? maximizedIn($maximized, active) : null;

	/** The old grid named its areas by what they showed; a cell does too. */
	const named = (child: PaneNode): string => {
		const c = child.tabs[child.active].content;
		return isSplit(c) ? '' : (KINDS[c.kind]?.meta.title ?? c.kind);
	};
	/**
	 * What a pane's frame adds around its content: see PaneFrame.svelte.
	 * The bar's row (`ROW`, the frame's `--row`) and half another across a
	 * tabbed holder's edge — its strip, and the room its children's titles
	 * straddle their outlines in — and the inside half of it on a framed
	 * pane's, where it straddles the outline.
	 *
	 * A tab carrying a LAYOUT draws the frame too, however its content is
	 * shaped (da-9puz), and insets its panes from it — the gap all round
	 * and the whole row on the bar's edge, which is where the title's own
	 * half-row already lies. The root's chrome cancels itself in
	 * `rootFloorOf`, so neither test need except it.
	 */
	const chromeOf = (p: PaneNode) => {
		const tab = p.tabs[p.active];
		const nested = isSplit(tab.content) && tab.layout !== undefined;
		const widget = !isSplit(tab.content);
		const outline = widget || nested ? OUTLINE : 0;
		// The inset already holds the title's half-row: only a widget pane's
		// title lies over its content. A tabbed holder's strip is a row, and
		// the half-row its children's titles hang above them is kept clear
		// under it (da-cesj).
		const bar = widget ? ROW / 2 : nested ? ROW + BORDER : p.tabs.length > 1 ? ROW + ROW / 2 : 0;
		const across = nested ? 2 * BORDER : 0;
		return p.bar.edge === 'left' || p.bar.edge === 'right'
			? { width: outline + bar, height: outline + across }
			: { width: outline + across, height: outline + bar };
	};
	// One function, two callers, two `minOf`s — keep them apart: the drag's
	// floor is the frame alone, the root's is every pane legible.
	const floor = (child: PaneNode) => floorOf(child, () => NONE, BORDER, chromeOf);
	/** The root split's least box: its panes' legible boxes, laid out as the tree lays them. */
	const legible = (root: PaneNode) => rootFloorOf(root, legibleOfKind, BORDER, chromeOf);
	/** A child's snaps as extents along the split: its widget tabs' snaps, plus its frame. */
	const snapsOf = (child: PaneNode, split: Split): number[] => {
		const axis = split.direction === 'row' ? 'width' : 'height';
		const chrome = chromeOf(child)[axis];
		return child.tabs.flatMap((t) =>
			isSplit(t.content) ? [] : snapsOfKind(t.content.kind, t.content.settings)[axis].map((s) => s + chrome)
		);
	};

	// ---- dragging a border -----------------------------------------------

	/** Every mounted tab's split, by id: a hidden tab keeps its own (da-rjmg). */
	const splitEls: Record<NodeId, HTMLDivElement> = {};
	let drag: {
		index: number;
		origin: number;
		pair: number;
		span: number;
		from: number[];
		minRatios: number[];
		/** The first child's extents a drag snaps to: its own snaps and the second's, from the far side. */
		stops: number[];
	} | null = null;
	/** The ratios being dragged, shown on the cells and not yet in the tree. */
	let preview: number[] | null = null;

	function cells(split: Split): HTMLElement[] {
		const el = splitEls[split.id];
		return el ? [...el.querySelectorAll<HTMLElement>(':scope > .cell')] : [];
	}

	function extent(el: HTMLElement, split: Split): number {
		const box = el.getBoundingClientRect();
		return split.direction === 'row' ? box.width : box.height;
	}

	/** Each child's floor as a share of the split, from what is on screen. */
	function minRatios(split: Split): number[] {
		const boxes = cells(split);
		let free = 0;
		let share = 0;
		split.children.forEach((child, i) => {
			if (!boxes[i]) return;
			free += extent(boxes[i], split);
			share += split.ratios[i];
		});
		return split.children.map((child) => {
			if (free <= 0) return 0;
			const f = floor(child);
			return ((split.direction === 'row' ? f.width : f.height) * share) / free;
		});
	}

	function down(event: PointerEvent, split: Split, index: number) {
		const border = event.currentTarget as HTMLElement;
		border.setPointerCapture(event.pointerId);
		border.focus();
		const boxes = cells(split);
		if (!boxes[index] || !boxes[index + 1]) return;
		const span = extent(boxes[index], split) + extent(boxes[index + 1], split);
		drag = {
			index,
			origin: split.direction === 'row' ? event.clientX : event.clientY,
			pair: split.ratios[index] + split.ratios[index + 1],
			span,
			from: split.ratios,
			minRatios: minRatios(split),
			stops: [
				...snapsOf(split.children[index], split),
				...snapsOf(split.children[index + 1], split).map((s) => span - s)
			]
		};
		resizing.set(true);
		event.preventDefault();
	}

	function move(event: PointerEvent, split: Split) {
		if (!drag || drag.span <= 0) return;
		const at = split.direction === 'row' ? event.clientX : event.clientY;
		const delta = ((at - drag.origin) / drag.span) * drag.pair;
		const next = shifted(drag.from, drag.index, delta, drag.minRatios);
		// The two children share the span as they share the pair's ratio.
		const px = (next[drag.index] / drag.pair) * drag.span;
		const stop = drag.stops.reduce((a, b) => (Math.abs(b - px) < Math.abs(a - px) ? b : a), Infinity);
		preview =
			Math.abs(stop - px) < SNAP_PX + 0.5
				? shifted(drag.from, drag.index, (stop / drag.span) * drag.pair - drag.from[drag.index], drag.minRatios)
				: next;
	}

	/** The drop: the tree takes the previewed ratios, once, and the panes redraw. */
	function up(split: Split) {
		const ratios = preview;
		const floors = drag?.minRatios;
		drag = null;
		preview = null;
		if (ratios) apply((l) => setRatios(l, split.id, ratios, floors));
		resizing.set(false);
	}

	/**
	 * Move `delta` of the split from child `index + 1` to child `index`,
	 * neither side below its floor — so what the drag shows is what the
	 * drop stores, and the drop moves nothing.
	 */
	function shifted(ratios: number[], index: number, delta: number, floors?: number[]): number[] {
		const next = [...ratios];
		const pair = next[index] + next[index + 1];
		const lo = Math.min(floors?.[index] ?? 0, pair);
		const hi = Math.max(lo, pair - (floors?.[index + 1] ?? 0));
		next[index] = Math.min(hi, Math.max(lo, next[index] + delta));
		next[index + 1] = pair - next[index];
		return next;
	}

	function nudge(event: KeyboardEvent, split: Split, index: number) {
		const grow = split.direction === 'row' ? 'ArrowRight' : 'ArrowDown';
		const shrink = split.direction === 'row' ? 'ArrowLeft' : 'ArrowUp';
		const delta = event.key === grow ? NUDGE : event.key === shrink ? -NUDGE : 0;
		if (delta === 0) return;
		event.preventDefault();
		const floors = minRatios(split);
		apply((l) => setRatios(l, split.id, shifted(split.ratios, index, delta, floors), floors));
	}
</script>

<PaneFrame
	{pane}
	{canClose}
	root={pane.id === rootId}
	locked={locked}
	kind={isSplit(content) ? undefined : content.kind}
>
	{#if big}
		<!-- The pane alone, under the root bar and in place of the whole
		     tree — a pane inside a nested layout tab included, since a
		     maximize into a small box is not one. It keeps its own frame,
		     and nothing in the tree changed, so the press that puts it back
		     puts everything back with it. -->
		<svelte:self pane={big.pane} {epoch} {rootId} locked={big.locked} />
	{:else}
		<!-- Every tab of the pane is mounted, the one in front and the ones
		     behind it (da-rjmg), so a swipe moves a picture already drawn.
		     Not the root's: its tabs are whole layouts, and one is plenty. -->
		{#each pane.id === rootId ? [active] : pane.tabs as tab (tab.id)}
		{@const content = tab.content}
		<TabBody active={tab === active}>
			{#if isSplit(content)}
				{@const split = content}
				{@const least = pane.id === rootId ? legible(pane) : undefined}
				{@const grow = (tab === active && preview) || split.ratios}
				<div
					class="split {split.direction}"
					bind:this={splitEls[split.id]}
					data-split={split.id}
					style:min-width={least && `${least.width}px`}
					style:min-height={least && `${least.height}px`}
				>
					{#each split.children as child, i (child.id)}
						{#if i > 0}
							{#if locked || tab.locked}
								<div class="border static" aria-hidden="true"></div>
							{:else}
								<!-- A focusable separator is a widget in ARIA (it carries a
								     value), which is what the keyboard needs it to be. -->
								<!-- svelte-ignore a11y-no-noninteractive-tabindex a11y-no-noninteractive-element-interactions -->
								<div
									class="border"
									role="separator"
									tabindex="0"
									aria-orientation={split.direction === 'row' ? 'vertical' : 'horizontal'}
									aria-valuenow={Math.round(grow[i - 1] * 100)}
									aria-valuemin="0"
									aria-valuemax="100"
									aria-label="resize"
									on:pointerdown={(e) => down(e, split, i - 1)}
									on:pointermove={(e) => move(e, split)}
									on:pointerup={() => up(split)}
									on:pointercancel={() => up(split)}
									on:keydown={(e) => nudge(e, split, i - 1)}
								></div>
							{/if}
						{/if}
						<div
							class="cell {named(child)}"
							style:--ratio={grow[i]}
							style:--floor="{split.direction === 'row' ? floor(child).width : floor(child).height}px"
						>
							<svelte:self pane={child} {epoch} {rootId} locked={locked || !!tab.locked} />
						</div>
					{/each}
				</div>
			{:else if content.kind === EMPTY_KIND}
				{@const tabId = tab.id}
				<Pane title="empty">
					<div class="choose">
						<button type="button" class="control" on:click={() => void fillTab(tabId)}
							>Choose Widget/Layout</button
						>
					</div>
				</Pane>
			{:else if KINDS[content.kind]}
				{@const widget = content}
				{@const kind = KINDS[content.kind]}
				{@const fold = kind.onChange}
				<!-- A widget that changes its own settings (the strip's day height,
				     the stats widget's pickers) says so with `change`, and the tree
				     keeps them — through the kind's `onChange`, which knows what
				     the event's detail is. -->
				<svelte:component
					this={kind.component}
					{...kind.props ? kind.props(widget.settings) : { settings: widget.settings }}
					{epoch}
					on:change={(e) =>
						fold && apply((l) => setWidget(l, tab.id, { ...widget, settings: fold(widget.settings, e.detail) }))}
				/>
			{:else}
				<Pane title={content.kind}>
					<p class="quiet">no widget of kind “{content.kind}” in this build</p>
				</Pane>
			{/if}
		</TabBody>
		{/each}
	{/if}
</PaneFrame>

<style>
	.split {
		display: flex;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.split.row {
		flex-direction: row;
	}
	.split.column {
		flex-direction: column;
	}

	.cell {
		display: flex;
		flex-direction: column;
		flex: var(--ratio) 1 0px;
		min-width: 0;
		min-height: 0;
	}
	.row > .cell {
		min-width: var(--floor);
	}
	.column > .cell {
		min-height: var(--floor);
	}
	.border {
		flex: 0 0 8px;
		touch-action: none;
		position: relative;
	}
	.row > .border {
		cursor: col-resize;
	}
	.column > .border {
		cursor: row-resize;
	}
	.border.static {
		cursor: default;
	}

	/* The handle itself is a hairline in the middle of the gap; the gap
	   is what a finger lands on. */
	.border:not(.static)::after {
		content: '';
		position: absolute;
		inset: 50% auto auto 50%;
		background: var(--tint);
		border-radius: 2px;
	}
	.row > .border:not(.static)::after {
		width: 2px;
		height: 2rem;
		transform: translate(-50%, -50%);
	}
	.column > .border:not(.static)::after {
		width: 2rem;
		height: 2px;
		transform: translate(-50%, -50%);
	}
	.border:hover::after,
	.border:focus-visible::after {
		background: var(--accent);
	}
	.border:focus-visible {
		outline: none;
	}

	/* One centred button, and nothing else: an empty pane is a question,
	   not a menu (vividn, da-uyx1 — a grid of pills read as a fields widget).
	   Not `.empty`: a cell carries the class of what it shows (`named`), so
	   an empty tab's cell IS `.cell.empty` and this would centre the pane
	   itself inside it. */
	.choose {
		display: grid;
		place-items: center;
		block-size: 100%;
	}
	.choose button {
		min-block-size: 24px;
		padding: 0.3rem 0.9rem;
		color: var(--accent);
	}
	.choose button:hover,
	.choose button:focus-visible {
		color: var(--accent-bright);
		border-color: var(--accent);
	}

	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
</style>
