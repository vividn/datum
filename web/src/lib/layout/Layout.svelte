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

	Floors: a child's cell is never narrower or shorter than its floor
	(`floorOf`: the largest of its tabs' kinds' minimums, plus the frame
	— `chromeOf`: a widget pane's outline and the inside half of its
	title's row, a tabbed holder's row of tabs, and nothing for a holder
	with one tab, which draws no bar at all — each on the edge the bar is
	on, so a left-hand title adds width and no height, da-13bc),
	so a drag clamps and a split too small for its children overflows to
	the root's scroll rather than hiding one. A kind that sizes itself
	(`fit`) takes its content's height and no share of the split; the
	border beside it is a gap, not a handle. The last content-sized child
	of a column sticks to the bottom of the scroller — which is the widget
	bar's phone pinning (da-hkpd), now a property of where it sits.
-->
<script lang="ts">
	import { getContext, setContext } from 'svelte';
	import { readable, writable, type Readable } from 'svelte/store';
	import Pane from '$lib/Pane.svelte';
	import PaneFrame from './PaneFrame.svelte';
	import Picker from './Picker.svelte';
	import type { NodeId, Pane as PaneNode, Split } from './types.js';
	import { isSplit } from './types.js';
	import { apply } from './current.js';
	import { EMPTY_KIND, floorOf, newWidget, setRatios, setWidget } from './tree.js';
	import { KINDS, floorOfKind } from './registry.js';
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
	 * (vividn: "reduce margins some"), to the floor a finger needs.
	 */
	const BORDER = 8;
	/** A widget pane's outline, 1px a side. */
	const OUTLINE = 2;
	/**
	 * The bar's row (PaneFrame's `--row`): the whole of it across a tabbed
	 * holder's edge, and the inside half of it on a widget pane's, where it
	 * straddles the outline.
	 */
	const ROW = 24;
	/** What one arrow press moves a border. */
	const NUDGE = 0.02;

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
	/** The old grid named its areas by what they showed; a cell does too. */
	const named = (child: PaneNode): string => {
		const c = child.tabs[child.active].content;
		return isSplit(c) ? '' : (KINDS[c.kind]?.meta.title ?? c.kind);
	};
	/** What a pane's frame adds around its content: see PaneFrame.svelte. */
	const chromeOf = (p: PaneNode) => {
		const widget = !isSplit(p.tabs[p.active].content);
		const outline = widget ? OUTLINE : 0;
		const bar = widget ? ROW / 2 : p.tabs.length > 1 ? ROW : 0;
		return p.bar.edge === 'left' || p.bar.edge === 'right'
			? { width: outline + bar, height: outline }
			: { width: outline, height: outline + bar };
	};
	const floor = (child: PaneNode) => floorOf(child, floorOfKind, BORDER, chromeOf);

	// ---- dragging a border -----------------------------------------------

	let splitEl: HTMLDivElement | undefined;
	let drag: {
		index: number;
		origin: number;
		pair: number;
		span: number;
		from: number[];
		minRatios: number[];
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
		drag = {
			index,
			origin: split.direction === 'row' ? event.clientX : event.clientY,
			pair: split.ratios[index] + split.ratios[index + 1],
			span: extent(boxes[index], split) + extent(boxes[index + 1], split),
			from: split.ratios,
			minRatios: minRatios(split)
		};
		resizing.set(true);
		event.preventDefault();
	}

	function move(event: PointerEvent, split: Split) {
		if (!drag || drag.span <= 0) return;
		const at = split.direction === 'row' ? event.clientX : event.clientY;
		const delta = ((at - drag.origin) / drag.span) * drag.pair;
		preview = shifted(drag.from, drag.index, delta, drag.minRatios);
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
		<div class="split {split.direction}" bind:this={splitEl} data-split={split.id}>
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
					style:--ratio={(preview ?? split.ratios)[i]}
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
		background: #0a0a1a;
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
		background: #1e1e38;
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
		background: #7c7cff;
	}
	.border:focus-visible {
		outline: none;
	}

	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}
</style>
