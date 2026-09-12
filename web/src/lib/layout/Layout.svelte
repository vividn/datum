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
	(no handle, no keyboard) and every frame draws no `…`.

	Floors: a child's cell is never narrower or shorter than its frame
	(`floorOf` over `chromeOf`: a widget pane's outline and the inside
	half of its title's row, a tabbed holder's row of tabs, and nothing
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
	The arrows do not snap. A kind that sizes itself
	(`fit`) takes its content's height and no share of the split; the
	border beside it is a gap, not a handle. The last content-sized child
	of a column sticks to the bottom of the scroller — which is the widget
	bar's phone pinning (da-hkpd), now a property of where it sits.
-->
<script lang="ts">
	import { getContext, setContext } from 'svelte';
	import { readable, writable, type Readable } from 'svelte/store';
	import Pane from '$lib/Pane.svelte';
	import PaneFrame, { ROW } from './PaneFrame.svelte';
	import Picker from './Picker.svelte';
	import type { NodeId, Pane as PaneNode, Split } from './types.js';
	import { isSplit } from './types.js';
	import { apply } from './current.js';
	import { EMPTY_KIND, floorOf, newWidget, rootFloorOf, setRatios, setWidget } from './tree.js';
	import { KINDS, legibleOfKind, snapsOfKind } from './registry.js';
	import { resizing } from './resizing.js';
	import { inlineInto } from './shell.js';

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

	// The layouts open above this pane's content (da-0a5o): the ones its
	// picker must not offer, or `day` could hold `day`. A store, since the
	// active tab changes and a context is set once.
	const above = getContext<Readable<string[]> | undefined>('layout-ancestors') ?? readable<string[]>([]);
	const ancestors = writable<string[]>([]);
	setContext('layout-ancestors', ancestors);
	$: ancestors.set(active.layout === undefined ? $above : [...$above, active.layout]);
	$: kind = isSplit(content) || content.kind === EMPTY_KIND ? undefined : KINDS[content.kind];
	$: canClose = !(pane.id === rootId && pane.tabs.length === 1);
	$: lockedHere = locked || !!active.locked;

	const fits = (child: PaneNode): boolean => {
		const c = child.tabs[child.active].content;
		return !isSplit(c) && !!KINDS[c.kind]?.fit;
	};
	/**
	 * What each cell grows by: its ratio over the children that grow, since
	 * flexbox hands out only a fraction of the free space when the grows
	 * sum below 1 — a content-sized child's share would go empty (da-6twf).
	 */
	const grown = (split: Split, ratios: number[]): number[] => {
		const sum = split.children.reduce((s, c, i) => (fits(c) ? s : s + ratios[i]), 0) || 1;
		return ratios.map((r) => r / sum);
	};
	/** The old grid named its areas by what they showed; a cell does too. */
	const named = (child: PaneNode): string => {
		const c = child.tabs[child.active].content;
		return isSplit(c) ? '' : (KINDS[c.kind]?.meta.title ?? c.kind);
	};
	/**
	 * What a pane's frame adds around its content: see PaneFrame.svelte.
	 * The bar's row (`ROW`, the frame's `--row`) is the whole of it across
	 * a tabbed holder's edge, and the inside half of it on a widget pane's,
	 * where it straddles the outline.
	 */
	const chromeOf = (p: PaneNode) => {
		const widget = !isSplit(p.tabs[p.active].content);
		const outline = widget ? OUTLINE : 0;
		const bar = widget ? ROW / 2 : p.tabs.length > 1 ? ROW : 0;
		return p.bar.edge === 'left' || p.bar.edge === 'right'
			? { width: outline + bar, height: outline }
			: { width: outline, height: outline + bar };
	};
	// One function, two callers, two `minOf`s — keep them apart: the drag's
	// floor is the frame alone, the root's is every pane legible.
	const floor = (child: PaneNode) => floorOf(child, () => NONE, BORDER, chromeOf);
	/** Each cell's drawn height: what a content-sized child takes of the root's minimum. */
	let drawn: number[] = [];
	/**
	 * The root split's least box: its panes' legible boxes, and a
	 * content-sized child at the height it is drawn where that is more —
	 * the bar wraps to as many rows as its chips need.
	 */
	const legible = (root: PaneNode, split: Split, drawn: number[]) =>
		rootFloorOf(root, split, legibleOfKind, BORDER, chromeOf, (child, i) => (fits(child) ? (drawn[i] ?? 0) : 0));
	/** A child's snaps as extents along the split: its widget tabs' snaps, plus its frame. */
	const snapsOf = (child: PaneNode, split: Split): number[] => {
		const axis = split.direction === 'row' ? 'width' : 'height';
		const chrome = chromeOf(child)[axis];
		return child.tabs.flatMap((t) =>
			isSplit(t.content) ? [] : snapsOfKind(t.content.kind, t.content.settings)[axis].map((s) => s + chrome)
		);
	};

	// ---- dragging a border -----------------------------------------------

	let splitEl: HTMLDivElement | undefined;
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

	function cells(): HTMLElement[] {
		return splitEl ? [...splitEl.querySelectorAll<HTMLElement>(':scope > .cell')] : [];
	}

	function extent(el: HTMLElement, split: Split): number {
		const box = el.getBoundingClientRect();
		return split.direction === 'row' ? box.width : box.height;
	}

	/** Each child's floor as a share of the split, from what is on screen. */
	function minRatios(split: Split): number[] {
		const boxes = cells();
		let free = 0;
		let share = 0;
		split.children.forEach((child, i) => {
			if (fits(child) || !boxes[i]) return;
			free += extent(boxes[i], split);
			share += split.ratios[i];
		});
		return split.children.map((child, i) => {
			if (fits(child) || free <= 0) return 0;
			const f = floor(child);
			return ((split.direction === 'row' ? f.width : f.height) * share) / free;
		});
	}

	function down(event: PointerEvent, split: Split, index: number) {
		const border = event.currentTarget as HTMLElement;
		border.setPointerCapture(event.pointerId);
		border.focus();
		const boxes = cells();
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
	locked={lockedHere}
	kind={isSplit(content) ? undefined : content.kind}
>
	{#if isSplit(content)}
		{@const split = content}
		{@const least = pane.id === rootId ? legible(pane, split, drawn) : undefined}
		{@const grow = grown(split, preview ?? split.ratios)}
		<div
			class="split {split.direction}"
			bind:this={splitEl}
			data-split={split.id}
			style:min-width={least && `${least.width}px`}
			style:min-height={least && `${least.height}px`}
		>
			{#each split.children as child, i (child.id)}
				{#if i > 0}
					{#if lockedHere || fits(split.children[i - 1]) || fits(child)}
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
							aria-valuenow={Math.round((preview ?? split.ratios)[i - 1] * 100)}
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
					class:fit={fits(child)}
					bind:offsetHeight={drawn[i]}
					style:--ratio={grow[i]}
					style:--floor="{split.direction === 'row' ? floor(child).width : floor(child).height}px"
				>
					<svelte:self pane={child} {epoch} {rootId} locked={lockedHere} />
				</div>
			{/each}
		</div>
	{:else if content.kind === EMPTY_KIND}
		{@const tabId = active.id}
		<Picker
			on:pick={(e) => apply((l) => setWidget(l, tabId, newWidget(e.detail)))}
			on:layout={(e) => void inlineInto(tabId, e.detail)}
		/>
	{:else if kind}
		{@const widget = content}
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
				fold && apply((l) => setWidget(l, active.id, { ...widget, settings: fold(widget.settings, e.detail) }))}
		/>
	{:else}
		<Pane title={content.kind}>
			<p class="quiet">no widget of kind “{content.kind}” in this build</p>
		</Pane>
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
	.cell.fit {
		flex: 0 0 auto;
	}

	/* The bar's phone pinning (da-hkpd): where a column overflows the
	   root's scroller, its last content-sized child stays on screen and
	   the panes scroll behind it. */
	.column > .cell.fit:last-child {
		position: sticky;
		bottom: 0;
		z-index: 1;
		background: var(--page);
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

	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
</style>
