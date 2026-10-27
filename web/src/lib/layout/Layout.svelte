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

	Locked is NORMAL MODE (da-vvcxc): outside layout mode (`mode.ts`) the
	flag is threaded down as a prop from the root, and under it every
	border is `static` (no handle, no keyboard) and every frame draws no
	`+` and no `×`. In layout mode every border is a handle, drawn big in
	the gap so it reads as one (vividn: «prominent handles in the gaps
	between panes»), and every frame's outline is drawn in the accent.

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
	import { untrack } from 'svelte';
	import Layout from './Layout.svelte';
	import Pane from '$lib/Pane.svelte';
	import { act } from '$lib/act.js';
	import PaneFrame, { ROW, badge, lands } from './PaneFrame.svelte';
	import TabBody from './TabBody.svelte';
	import Stale from './Stale.svelte';
	import { ready } from './engine.js';
	import { fillTab } from './chooser.js';
	import { CHANGE } from './menu.js';
	import type { NodeId, Pane as PaneNode, Split, Tab } from './types.js';
	import { isSplit } from './types.js';
	import { apply, layout } from './current.js';
	import { EMPTY_KIND, floorOf, rootFloorOf, setRatios } from './tree.js';
	import { maximized, maximizedIn } from './maximize.js';
	import { KINDS, legibleOfKind, snapsOfKind } from './registry.js';
	import { resizing } from './resizing.js';
	import { keyOf, session, setSession, withSession } from './session.js';
	import { settled } from '$lib/run.js';

	let {
		pane,
		epoch = 0,
		/** The root pane's id: the one pane whose last tab cannot be closed. */
		rootId,
		/** Normal mode: the page is not in layout mode, so nothing here moves the tree. */
		locked = false
	}: {
		pane: PaneNode;
		epoch?: number;
		rootId: NodeId;
		locked?: boolean;
	} = $props();

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

	const active = $derived(pane.tabs[pane.active]);
	const content = $derived(active.content);
	const canClose = $derived(!(pane.id === rootId && pane.tabs.length === 1));
	/**
	 * The maximized pane (da-00am), resolved HERE and nowhere else: there is
	 * no `maximized` prop threaded down every level. Null unless this is the
	 * root, the state belongs to the tab in front, and the pane is still in
	 * its tree — which is how a closed or broken-out pane heals itself.
	 *
	 * The tree stays mounted under it (da-x7xjg): the root's split is marked
	 * `maximized`, which hides it, and the pane's own frame (`.big`,
	 * PaneFrame.svelte) is laid over the root's box by the CSS below. Every
	 * other pane keeps its box and its picture, so a restore draws nothing
	 * again but the pane that was big, and the layouts kept behind stay.
	 */
	const big = $derived(pane.id === rootId ? maximizedIn($maximized, active) : null);

	/**
	 * The root's layouts once opened, the latest first (da-tajsm): each stays
	 * mounted behind the one in front as an inner pane's tabs do, so a
	 * return shows a picture already drawn instead of building every widget
	 * and asking the CLI again. `KEPT` of them, the tab in front included:
	 * a phone's three default layouts cost 3 MB of heap and 6,000 DOM nodes
	 * over one. A test's shared app keeps one (`__datumKeptLayouts`).
	 */
	const KEPT = (globalThis as { __datumKeptLayouts?: number }).__datumKeptLayouts ?? 3;
	let opened = $state.raw<NodeId[]>([]);
	// Called here, declared below: `opened` is reassigned, never mutated, so
	// Svelte sees each change.
	$effect.pre(() => {
		const id = active.id;
		if (pane.id === rootId) untrack(() => remember(id));
	});
	function remember(id: NodeId): void {
		if (opened[0] !== id) opened = [id, ...opened.filter((o) => o !== id)].slice(0, KEPT);
	}
	/**
	 * The epoch the layouts behind are drawn at: the page's, once every
	 * command has answered and a second has gone by with no other. A run
	 * cannot be stopped once started, and a hidden layout's refresh drawn the
	 * moment a write lands kept a line typed after it waiting about 250 ms
	 * longer on a phone at 4× (vremya's LINE2, da-tajsm). The tab in front,
	 * and the one a swipe is bringing on, are always at the page's: a layout
	 * brought to the front catches up then.
	 */
	const QUIET_MS = 1000;
	let behind = $state(untrack(() => epoch));
	let lagging = 0;
	$effect.pre(() => {
		const next = epoch;
		if (pane.id === rootId) untrack(() => void lag(next));
	});
	async function lag(next: number): Promise<void> {
		const mine = ++lagging;
		await settled();
		await new Promise((resolve) => setTimeout(resolve, QUIET_MS));
		if (mine === lagging) behind = next;
	}
	/**
	 * The epoch each root tab was last drawn at live. A tab left in the
	 * moment after a write is drawn at no less: an epoch going back is a
	 * change, and its panes ran again at the old one and once more as it
	 * caught up (vremya on #1188). A plain map, so the write below, made as
	 * the tab is drawn, invalidates nothing.
	 */
	const live = new Map<NodeId, number>();
	function drawnAt(tab: Tab, front: boolean, now: number, held: number): number {
		if (front) live.set(tab.id, now);
		return front ? now : Math.max(held, live.get(tab.id) ?? held);
	}

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
	const chromeOf = $derived((p: PaneNode) => {
		const tab = p.tabs[p.active];
		const nested = isSplit(tab.content) && tab.layout !== undefined;
		const widget = !isSplit(tab.content);
		const outline = widget || nested ? OUTLINE : 0;
		// The inset already holds the title's half-row: only a widget pane's
		// title lies over its content. A tabbed holder's strip is a row, and
		// the half-row its children's titles hang above them is kept clear
		// under it (da-cesj). Both are the badge's box, which is a finger's
		// under a coarse pointer and the row under a fine one (da-kpjn).
		const bar = widget ? ROW / 2 : nested ? ROW + BORDER : p.tabs.length > 1 ? $badge + $badge / 2 : 0;
		const across = nested ? 2 * BORDER : 0;
		return p.bar.edge === 'left' || p.bar.edge === 'right'
			? { width: outline + bar, height: outline + across }
			: { width: outline + across, height: outline + bar };
	});
	/**
	 * The settings a tab's widget is DRAWN from (da-vhwq): the tree's with
	 * this session's over them, the same merge the widget itself is handed.
	 *
	 * Everything that SIZES a widget reads this and not the tree, or a
	 * session-level day height would be drawn at one height and snapped and
	 * floored at another.
	 *
	 * `keyOf` is the ONE derivation of a session key, here and at the write
	 * below and in PaneFrame and RootBar (da-vhwq, the panel on #64). It was
	 * a `scope` prop threaded down beside the lock, which agreed with
	 * `scopes()` for every tree that ships — but only because a root tab
	 * always names a layout. A pane's `scope` came from its pane's ACTIVE
	 * tab, and `scopes()` walks the tab's own path, so a nameless root tab
	 * over a pane whose tabs carry layouts of their own had the writer and
	 * the reader disagreeing: the setting would be written under one key,
	 * read under another, and pruned as dead. One derivation cannot
	 * disagree with itself.
	 */
	const shownOf = (tab: Tab): Record<string, unknown> =>
		isSplit(tab.content) ? {} : withSession(tab.content.settings, $session[keyOf($layout, tab.id)]);

	// One function, two callers, two `minOf`s — keep them apart: the drag's
	// floor is the frame alone, the root's is every pane legible.
	const floor = $derived((child: PaneNode) => floorOf(child, () => NONE, BORDER, chromeOf));
	/** The root split's least box: its panes' legible boxes, laid out as the tree lays them. */
	const legible = $derived((root: PaneNode) =>
		rootFloorOf(
			root,
			(tab) => (isSplit(tab.content) ? NONE : legibleOfKind(tab.content.kind, shownOf(tab))),
			BORDER,
			chromeOf
		));
	/** A child's snaps as extents along the split: its widget tabs' snaps, plus its frame. */
	const snapsOf = (child: PaneNode, split: Split): number[] => {
		const axis = split.direction === 'row' ? 'width' : 'height';
		const chrome = chromeOf(child)[axis];
		return child.tabs.flatMap((t) =>
			isSplit(t.content) ? [] : snapsOfKind(t.content.kind, shownOf(t))[axis].map((s) => s + chrome)
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
	let preview = $state.raw<number[] | null>(null);

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
	{#snippet children(peek, moves)}
		<!-- Every tab of the pane is mounted, the one in front and the ones
		     behind it (da-rjmg), so a swipe moves a picture already drawn.
		     The root's: the layouts it has opened (`opened`), and the one a
		     swipe in hand is bringing on (da-24fgb). In the tree's order, so
		     a switch moves no element, and a moved one loses its scroll. By
		     id and with `pane.active` too: the slot can be updated with a
		     newer `pane` than its `active`, and the tab in front must not
		     drop out for a frame and remount. -->
		{#each pane.id === rootId ? pane.tabs.filter((t, i) => i === peek || i === pane.active || opened.includes(t.id)) : pane.tabs as tab (tab.id)}
		{@const content = tab.content}
		{@const at = pane.id === rootId ? drawnAt(tab, tab === active || tab === pane.tabs[peek], epoch, behind) : epoch}
		{@const least = pane.id === rootId && isSplit(content) ? legible(pane) : undefined}
		<TabBody
			active={tab === active}
			beside={tab === pane.tabs[peek]}
			moved={tab === active ? moves?.front : tab === pane.tabs[peek] ? moves?.beside : undefined}
			shy={pane.id === rootId ? lands(tab) - lands(active) : 0}
			{least}
			kept={pane.id === rootId}
		>
			{#if isSplit(content)}
				{@const split = content}
				{@const grow = (tab === active && preview) || split.ratios}
				<div
					class="split {split.direction}"
					class:maximized={tab === active && big !== null}
					bind:this={splitEls[split.id]}
					data-split={split.id}
					style:--border="{BORDER}px"
					style:min-width={least && `${least.width}px`}
					style:min-height={least && `${least.height}px`}
				>
					{#each split.children as child, i (child.id)}
						{#if i > 0}
							{#if locked}
								<div class="border static" aria-hidden="true"></div>
							{:else}
								<!-- A focusable separator is a widget in ARIA (it carries a
								     value), which is what the keyboard needs it to be. -->
								<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
								<div
									class="border"
									role="separator"
									tabindex="0"
									aria-orientation={split.direction === 'row' ? 'vertical' : 'horizontal'}
									aria-valuenow={Math.round(grow[i - 1] * 100)}
									aria-valuemin="0"
									aria-valuemax="100"
									aria-label="resize"
									onpointerdown={(e) => down(e, split, i - 1)}
									onpointermove={(e) => move(e, split)}
									onpointerup={() => up(split)}
									onpointercancel={() => up(split)}
									onkeydown={(e) => nudge(e, split, i - 1)}
								></div>
							{/if}
						{/if}
						<div
							class="cell {named(child)}"
							style:--ratio={grow[i]}
							style:--floor="{split.direction === 'row' ? floor(child).width : floor(child).height}px"
						>
							<Layout pane={child} epoch={at} {rootId} {locked} />
						</div>
					{/each}
				</div>
			{:else if content.kind === EMPTY_KIND}
				{@const tabId = tab.id}
				<Pane title="empty">
					<div class="choose">
						<button type="button" class="control" use:act={'open'} onclick={() => void fillTab(tabId)}
							>{CHANGE}</button
						>
					</div>
				</Pane>
			{:else if KINDS[content.kind]}
				{@const widget = content}
				{@const kind = KINDS[content.kind]}
				{@const fold = kind.onChange}
				{@const key = keyOf($layout, tab.id)}
				<!-- What the widget is DRAWN from: the tree's settings with this
				     session's over them (da-vhwq, session.ts). A session setting
				     never reaches the tree, so a widget asks for neither — it is
				     handed one set of settings and cannot tell which is which. -->
				{@const settings = withSession(widget.settings, $session[key])}
				<!-- A widget that changes a setting of its own (nowview's − / +,
				     the strip's handle, the stats widget's range chips) says so
				     with `onchange`, and it lands in THIS SESSION, exactly where the menu's option
				     rows land — through the kind's `onChange`, which knows what
				     the callback's argument is. The layout hears of it only when the
				     reader asks («Save options with layout»), which is what makes
				     the layout safe from a widget's own in-body controls without
				     a guard on every one of them.

				     `fold` is given the settings as SHOWN, so a change folds onto
				     what this session already set rather than onto the tree
				     underneath it; `setSession` keeps only what differs from the
				     tree, so a setting moved back leaves nothing held. -->
				<!-- The tree is drawn before the CLI has answered (da-bhudz), with each
				     widget's last output over it; the widget itself mounts once the
				     bar can take a command, so the bar's own reads are not queued
				     behind every pane's. A `live` kind, which asks nothing, is
				     mounted at once and takes no copy (da-ch6oc): the copy,
				     dimmed, stayed over it until the queue ran dry, on the boot
				     and on every return to its root tab. -->
				{#if !kind.live}<Stale id={widget.id} live={$ready} />{/if}
				{#if $ready || kind.live}
					<kind.component
						{...kind.props ? kind.props(settings) : { settings }}
						epoch={at}
						{key}
						onchange={(moved: unknown) => fold && setSession(key, widget.settings, fold(settings, moved))}
					/>
				{/if}
			{:else}
				<Pane title={content.kind}>
					<p class="quiet">no widget of kind “{content.kind}” in this build</p>
				</Pane>
			{/if}
		</TabBody>
		{/each}
	{/snippet}
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

	/* A maximize (da-x7xjg): the root's content is hidden, the tree in it
	   laid out as it was, and the big pane shown over the root's whole box —
	   a pane inside a nested layout included, since a maximize into a small
	   box is not one. Its containing block is the root's section: every
	   frame between them is unpositioned while it is big, and the root's
	   scroller too, which neither scrolls nor clips it then. */
	:global(.root > .content:has(> .tab-body > .split.maximized)) {
		position: static;
		visibility: hidden;
	}
	.maximized :global(:is(section, .content):has(section.big)) {
		position: static;
	}
	.maximized :global(section.big) {
		position: absolute;
		inset: 0;
		visibility: visible;
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
	/* `BORDER`, the one number the floors are worked out from. */
	.border {
		flex: 0 0 var(--border);
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

	/* A border is a handle only in layout mode (da-vvcxc), and there it is
	   meant to be SEEN: a thick accent grip in the middle of the gap. The
	   gap is what a finger lands on. */
	.border:not(.static)::after {
		content: '';
		position: absolute;
		inset: 50% auto auto 50%;
		background: var(--accent);
		border-radius: 3px;
	}
	.row > .border:not(.static)::after {
		width: 4px;
		height: 3rem;
		transform: translate(-50%, -50%);
	}
	.column > .border:not(.static)::after {
		width: 3rem;
		height: 4px;
		transform: translate(-50%, -50%);
	}
	.border:hover::after,
	.border:focus-visible::after {
		background: var(--accent-bright);
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
