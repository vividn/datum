// The operations on a layout tree — pure, and the one place the tree's
// invariants are kept.
//
// Every operation takes a `Layout` and returns a NEW one; nothing here
// mutates. Every operation ends in `normalize`, which is the whole of the
// canon: no split with fewer than two children (a lone child is unnested
// into the pane above it — vividn: "auto unnests if there is ever just one
// split in a tab"), no pane with no tabs (it leaves its split), ratios
// that sum to 1 with one entry per child, and an `active` that names a
// tab the pane has. The last pane is the one thing an operation refuses
// to remove: `closeTab` on the root's only tab returns the layout as it
// was, and the menu greys the item.
//
// Type-only imports, on purpose: `tests/tree.test.mjs` loads this file
// straight into node (which strips the types), so it must not reach for
// anything that would need a bundler.

import type { Bar, BarEdge, Layout, NodeId, Pane, Split, Tab, Widget } from './types.js';

/** The kind an empty tab holds: one Choose Widget/Layout button, until something is chosen. */
export const EMPTY_KIND = 'empty';

const isSplit = (content: Widget | Split): content is Split => 'children' in content;

/**
 * A fresh id: eight base-36 characters of randomness. Ids are minted here
 * and nowhere else, so nothing ever reuses one — a saved layout's ids
 * stay stable, and a new node never collides with one.
 */
export function newId(): NodeId {
	const bytes = new Uint8Array(6);
	if (typeof crypto !== 'undefined' && crypto.getRandomValues) crypto.getRandomValues(bytes);
	else for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
	let n = 0;
	for (const b of bytes) n = n * 256 + b;
	return n.toString(36).padStart(8, '0').slice(-8);
}

export const newWidget = (kind: string, settings: Record<string, unknown> = {}): Widget => ({
	id: newId(),
	kind,
	settings
});

export const newTab = (content: Widget | Split = newWidget(EMPTY_KIND), title?: string): Tab =>
	title === undefined ? { id: newId(), content } : { id: newId(), title, content };

export const newPane = (tabs: Tab[] = [newTab()], bar: Bar = { edge: 'top', align: 'start' }): Pane => ({
	id: newId(),
	tabs,
	active: 0,
	bar
});

export const newSplit = (direction: Split['direction'], children: Pane[]): Split => ({
	id: newId(),
	direction,
	ratios: children.map(() => 1 / children.length),
	children
});

/**
 * The four arrows a split or a break-out is aimed with (da-9vtw): where
 * on the screen the pane lands. Spatial, always — `right` is to the right
 * of the pane whatever edge its bar is on.
 */
export type Toward = 'left' | 'right' | 'up' | 'down';

/** The split direction each arrow needs, and whether it lands before its neighbour. */
const AXIS: Record<Toward, Split['direction']> = { left: 'row', right: 'row', up: 'column', down: 'column' };
const BEFORE: Record<Toward, boolean> = { left: true, right: false, up: true, down: false };

const ordered = (pane: Pane, fresh: Pane, before: boolean): Pane[] => (before ? [fresh, pane] : [pane, fresh]);

/** `fresh` into `split` beside child `at`, on the side `before` says, taking half that child's share. */
function inserted(split: Split, at: number, fresh: Pane, before: boolean): Split {
	const children = [...split.children];
	const ratios = [...split.ratios];
	children.splice(before ? at : at + 1, 0, fresh);
	ratios.splice(at, 1, ratios[at] / 2, ratios[at] / 2);
	return { ...split, children, ratios };
}

// ---- walking ---------------------------------------------------------

/** Rebuild the tree with `fn` applied to every pane, children first. */
function mapPanes(pane: Pane, fn: (pane: Pane) => Pane): Pane {
	const tabs = pane.tabs.map((tab) =>
		isSplit(tab.content)
			? {
					...tab,
					content: {
						...tab.content,
						children: tab.content.children.map((child) => mapPanes(child, fn))
					}
				}
			: tab
	);
	return fn({ ...pane, tabs });
}

function withRoot(layout: Layout, root: Pane): Layout {
	return normalize({ ...layout, root });
}

function* panes(pane: Pane): Generator<Pane> {
	yield pane;
	for (const tab of pane.tabs)
		if (isSplit(tab.content)) for (const child of tab.content.children) yield* panes(child);
}

export function findPane(layout: Layout, paneId: NodeId): Pane | undefined {
	for (const pane of panes(layout.root)) if (pane.id === paneId) return pane;
	return undefined;
}

export function findSplit(layout: Layout, splitId: NodeId): Split | undefined {
	for (const pane of panes(layout.root))
		for (const tab of pane.tabs) if (isSplit(tab.content) && tab.content.id === splitId) return tab.content;
	return undefined;
}

/**
 * Where a pane sits: the split that holds it, the tab and pane that hold
 * that split, and which child of the split it is. The root pane sits in
 * nothing, and is the one pane with no `in`.
 */
interface Spot {
	pane: Pane;
	in: { split: Split; tab: Tab; holder: Pane; at: number } | null;
}

function spotOf(root: Pane, paneId: NodeId): Spot | null {
	if (root.id === paneId) return { pane: root, in: null };
	const walk = (holder: Pane): Spot | null => {
		for (const tab of holder.tabs) {
			if (!isSplit(tab.content)) continue;
			const split = tab.content;
			const at = split.children.findIndex((child) => child.id === paneId);
			if (at >= 0) return { pane: split.children[at], in: { split, tab, holder, at } };
			for (const child of split.children) {
				const found = walk(child);
				if (found) return found;
			}
		}
		return null;
	};
	return walk(root);
}

export function findTab(layout: Layout, tabId: NodeId): Tab | undefined {
	for (const pane of panes(layout.root)) for (const tab of pane.tabs) if (tab.id === tabId) return tab;
	return undefined;
}

// ---- the canon -------------------------------------------------------

/**
 * Ratios that sum to 1, one per child; equal shares when they cannot.
 *
 * Ratios that already sum to 1 come back untouched — dividing 0.2, 0.7
 * and 0.1 by their floating-point sum moves them in the last digit, and
 * a saved layout that "changes" on being read is a layout the save and
 * restore controls (da-wpgy) would forever report as modified.
 */
function evenOut(ratios: number[], count: number): number[] {
	const given = ratios.slice(0, count).map((r) => (Number.isFinite(r) && r > 0 ? r : 0));
	while (given.length < count) given.push(0);
	const sum = given.reduce((a, b) => a + b, 0);
	if (sum <= 0) return given.map(() => 1 / count);
	if (Math.abs(sum - 1) < 1e-9) return given;
	return given.map((r) => r / sum);
}

/**
 * Bring a pane and everything under it into canon.
 *
 * Children first, so that by the time a pane is looked at its splits are
 * already canonical: a split left with one child hands that child's tabs
 * up into this pane in place of the tab that held it (the unnest), a
 * split left with none takes its tab with it, and a pane left with no
 * tabs is dropped by the split above (which is what makes a chain of
 * closes collapse all the way up).
 */
function canon(pane: Pane): Pane {
	const tabs: Tab[] = [];
	for (const tab of pane.tabs) {
		if (!isSplit(tab.content)) {
			tabs.push(tab);
			continue;
		}
		const children = tab.content.children.map(canon).filter((child) => child.tabs.length > 0);
		// A tab holding a layout (da-0a5o) keeps its identity whatever
		// happens under it: emptied, it goes back to being an empty tab;
		// down to one child, it keeps that child rather than dissolving into
		// its tabs — the tab IS the layout, and its name must not vanish.
		if (tab.layout !== undefined) {
			if (children.length === 0) tabs.push({ ...tab, content: newWidget(EMPTY_KIND) });
			else tabs.push({ ...tab, content: { ...tab.content, children, ratios: evenOut(tab.content.ratios, children.length) } });
			continue;
		}
		if (children.length === 0) continue;
		if (children.length === 1) {
			tabs.push(...children[0].tabs);
			continue;
		}
		tabs.push({
			...tab,
			content: { ...tab.content, children, ratios: evenOut(tab.content.ratios, children.length) }
		});
	}
	const active = Math.min(Math.max(0, Math.floor(pane.active) || 0), Math.max(0, tabs.length - 1));
	return { ...pane, tabs, active };
}

/**
 * The canonical form of `layout`. Idempotent. The root is the one pane
 * that may not vanish: emptied, it is given a fresh empty tab instead.
 */
export function normalize(layout: Layout): Layout {
	let root = canon(layout.root);
	if (root.tabs.length === 0) root = { ...root, tabs: [newTab()], active: 0 };
	return { ...layout, root };
}

// ---- the operations --------------------------------------------------

/**
 * Split a pane in two: it becomes one child of a new split, beside a new
 * pane holding one empty tab, on the arrow's side. All its tabs travel
 * with it. When the pane already sits in a split running the same way,
 * the new pane simply joins that split beside it, taking half its share —
 * the same picture with one fewer level of tree.
 */
export function splitPane(layout: Layout, paneId: NodeId, toward: Toward): Layout {
	return withRoot(layout, placed(layout.root, paneId, newPane(), toward));
}

/** `fresh` on the arrow's side of pane `paneId`, in that pane's own box: `splitPane`'s picture. */
function placed(root: Pane, paneId: NodeId, fresh: Pane, toward: Toward): Pane {
	const direction = AXIS[toward];
	const before = BEFORE[toward];
	let done = false;
	const joined = mapPanes(root, (pane) => ({
		...pane,
		tabs: pane.tabs.map((tab) => {
			if (!isSplit(tab.content) || tab.content.direction !== direction) return tab;
			const at = tab.content.children.findIndex((child) => child.id === paneId);
			if (at < 0) return tab;
			done = true;
			return { ...tab, content: inserted(tab.content, at, fresh, before) };
		})
	}));
	if (done) return joined;
	return mapPanes(root, (pane) =>
		pane.id === paneId ? newPane([newTab(newSplit(direction, ordered(pane, fresh, before)))]) : pane
	);
}

/**
 * Whether `breakOut` has anywhere to go: every pane in a split, and a tab
 * stack anywhere — the root's own included, unless a tab of it holds a
 * layout: those are the shell's root bar (shell.ts), not a stack of
 * widgets, and wrapping it would bury every layout in one untitled tab.
 */
export function canBreakOut(layout: Layout, paneId: NodeId): boolean {
	const spot = spotOf(layout.root, paneId);
	if (!spot) return false;
	const { pane } = spot;
	return !!spot.in || (pane.tabs.length > 1 && pane.tabs.every((tab) => tab.layout === undefined));
}

/**
 * Break a widget out of the group it is in, one level (da-9vtw, da-n632).
 *
 * A pane with several tabs leaves its TAB STRIP: the active tab becomes a
 * pane of its own, beside the stack, in the stack's own box — what
 * `splitPane` does to the pane, with the tab where the empty pane would
 * be (vividn: "it acts similar to split when there is only one tab"). So on
 * the phone's `COL(daystrip, TABS[tail · nowview · custom], widgetbar)`:
 *
 *   right : COL(daystrip, ROW(TABS[nowview · custom], tail), widgetbar)
 *   left  : COL(daystrip, ROW(tail, TABS[nowview · custom]), widgetbar)
 *   down  : COL(daystrip, TABS[nowview · custom], tail, widgetbar)
 *   up    : COL(daystrip, tail, TABS[nowview · custom], widgetbar)
 *
 * A pane with one tab leaves its SPLIT: the pane itself goes, what is left
 * of the split closes up behind it (`normalize`), and it lands beside the
 * pane that held that split — in the grouping holding it when that runs
 * the arrow's way, else around the whole of that grouping, in a new split
 * of the right direction. So repeating the operation walks a widget up
 * the tree, out of one grouping after another, until it is a band across
 * the layout; a root with nothing to leave comes back unchanged
 * (`canBreakOut`).
 */
export function breakOut(layout: Layout, paneId: NodeId, toward: Toward): Layout {
	const spot = spotOf(layout.root, paneId);
	if (!spot || !canBreakOut(layout, paneId)) return layout;
	const { pane } = spot;
	if (pane.tabs.length > 1) {
		const moving = newPane([pane.tabs[pane.active]], pane.bar);
		return withRoot(layout, placed(withoutTab(layout.root, pane.id, pane.active), pane.id, moving, toward));
	}
	const within = spot.in!;
	const root = withoutPane(layout.root, pane.id);
	// The box left behind, and where that box itself sits: that is the
	// grouping to land in, and the box is what it lands beside. A box in no
	// split of its own is the root's, and the grouping is then the tab the
	// moving pane has just come out of — with no box to land beside.
	const box = spotOf(root, within.holder.id);
	const where =
		box && box.in ? { tab: box.in.tab.id, anchor: box.pane.id } : { tab: within.tab.id, anchor: null };
	return withRoot(layout, beside(root, where.tab, where.anchor, pane, toward));
}

/** The pane without tab `index`: the tab a break-out takes with it. */
function withoutTab(root: Pane, paneId: NodeId, index: number): Pane {
	return mapPanes(root, (pane) =>
		pane.id !== paneId
			? pane
			: {
					...pane,
					tabs: pane.tabs.filter((_, i) => i !== index),
					active: Math.max(0, pane.active > index ? pane.active - 1 : pane.active)
				}
	);
}

/** The tree without that pane: it leaves its split, whose ratios `normalize` shares out again. */
function withoutPane(root: Pane, paneId: NodeId): Pane {
	return mapPanes(root, (pane) => ({
		...pane,
		tabs: pane.tabs.map((tab) => {
			if (!isSplit(tab.content)) return tab;
			const at = tab.content.children.findIndex((child) => child.id === paneId);
			if (at < 0) return tab;
			return {
				...tab,
				content: {
					...tab.content,
					children: tab.content.children.filter((_, i) => i !== at),
					ratios: tab.content.ratios.filter((_, i) => i !== at)
				}
			};
		})
	}));
}

/**
 * Put `moving` into what `tabId` holds, on the arrow's side: beside
 * `anchor` when that content is a split already running the arrow's way
 * (at its far end when there is no anchor — the pane came out of the
 * grouping it is landing in), and otherwise around the whole of it, in a
 * new split of the right direction.
 */
function beside(root: Pane, tabId: NodeId, anchor: NodeId | null, moving: Pane, toward: Toward): Pane {
	const direction = AXIS[toward];
	const before = BEFORE[toward];
	return mapPanes(root, (pane) => ({
		...pane,
		tabs: pane.tabs.map((tab) => {
			if (tab.id !== tabId) return tab;
			const content = tab.content;
			if (!isSplit(content) || content.direction !== direction)
				return { ...tab, content: newSplit(direction, ordered(newPane([newTab(content)]), moving, before)) };
			const at = anchor === null ? -1 : content.children.findIndex((child) => child.id === anchor);
			return { ...tab, content: inserted(content, at < 0 ? (before ? 0 : content.children.length - 1) : at, moving, before) };
		})
	}));
}

/** The share a pane added along an edge of the dashboard starts with. */
export const EDGE_RATIO = 0.2;

/**
 * Add a pane — one empty tab, asking to be filled — spanning a whole edge of the
 * active root tab's content (da-qk2k): the dashboard extended from its
 * side, however split up it already is. A content already split the way
 * the edge runs takes the new pane as its first or last child; anything
 * else is wrapped into a new split beside it. The new pane has
 * `EDGE_RATIO` and the rest share what is left as they shared the whole.
 */
export function addEdge(layout: Layout, edge: BarEdge): Layout {
	const direction: Split['direction'] = edge === 'top' || edge === 'bottom' ? 'column' : 'row';
	const first = edge === 'top' || edge === 'left';
	const root = layout.root;
	const tab = root.tabs[root.active];
	if (!tab) return layout;
	const c = tab.content;
	const split = isSplit(c) && c.direction === direction ? c : newSplit(direction, [newPane([newTab(c)])]);
	const fresh = newPane();
	const rest = split.ratios.map((r) => r * (1 - EDGE_RATIO));
	const content: Split = {
		...split,
		children: first ? [fresh, ...split.children] : [...split.children, fresh],
		ratios: first ? [EDGE_RATIO, ...rest] : [...rest, EDGE_RATIO]
	};
	const tabs = root.tabs.map((t, i) => (i === root.active ? { ...t, content } : t));
	return withRoot(layout, { ...root, tabs });
}

/**
 * A tab at the end of the pane, made active — an empty one, unless the
 * caller has minted its own (the `+` badge fills it with what the chooser
 * came back with, and needs its id to do that).
 */
export function addTab(layout: Layout, paneId: NodeId, tab: Tab = newTab()): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) =>
			pane.id !== paneId ? pane : { ...pane, tabs: [...pane.tabs, tab], active: pane.tabs.length }
		)
	);
}

/** Whether `closeTab` would do anything: everything but the root's last tab. */
export function canCloseTab(layout: Layout, paneId: NodeId): boolean {
	const pane = findPane(layout, paneId);
	return !!pane && !(pane.id === layout.root.id && pane.tabs.length === 1);
}

/**
 * Remove a tab. A pane left with none leaves its split, and a split left
 * with one child unnests (`normalize`). The last pane cannot be closed:
 * the layout comes back unchanged.
 */
export function closeTab(layout: Layout, paneId: NodeId, tabIndex: number): Layout {
	if (!canCloseTab(layout, paneId)) return layout;
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => {
			if (pane.id !== paneId || !pane.tabs[tabIndex]) return pane;
			const tabs = pane.tabs.filter((_, i) => i !== tabIndex);
			const active = pane.active > tabIndex || pane.active >= tabs.length ? pane.active - 1 : pane.active;
			return { ...pane, tabs, active };
		})
	);
}

export function setActive(layout: Layout, paneId: NodeId, active: number): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => (pane.id !== paneId ? pane : { ...pane, active }))
	);
}

/**
 * Lock or unlock a tab: nothing under it may be split, tabbed or resized
 * while it is locked (the renderer threads the lock down the subtree).
 * Unlocking removes the key, so a tree that was never locked and one
 * that was locked and unlocked are the same tree.
 */
export function setLocked(layout: Layout, tabId: NodeId, locked: boolean): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => ({
			...pane,
			tabs: pane.tabs.map((tab) => {
				if (tab.id !== tabId) return tab;
				const { locked: _, ...rest } = tab;
				return locked ? { ...rest, locked } : rest;
			})
		}))
	);
}

export function setBar(layout: Layout, paneId: NodeId, bar: Bar): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => (pane.id !== paneId ? pane : { ...pane, bar }))
	);
}

/**
 * New ratios for a split. They are renormalized to sum to 1, and clamped
 * so no child falls under its floor — `floors` are the same shape, the
 * least share each child may have (its widget's minimum size over the
 * split's extent), which the renderer measures and the tree cannot.
 * Weight taken from a child at its floor goes back where it came from.
 */
export function setRatios(layout: Layout, splitId: NodeId, ratios: number[], floors?: number[]): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => ({
			...pane,
			tabs: pane.tabs.map((tab) =>
				!isSplit(tab.content) || tab.content.id !== splitId
					? tab
					: { ...tab, content: { ...tab.content, ratios: clamped(ratios, tab.content, floors) } }
			)
		}))
	);
}

function clamped(ratios: number[], split: Split, floors: number[] | undefined): number[] {
	const count = split.children.length;
	const next = evenOut(ratios, count);
	if (!floors) return next;
	const min = floors.slice(0, count).map((f) => (Number.isFinite(f) && f > 0 ? f : 0));
	while (min.length < count) min.push(0);
	if (min.reduce((a, b) => a + b, 0) >= 1) return evenOut(split.ratios, count);
	// Lift every child to its floor, and pay for the lift out of the
	// children with room to spare, in proportion to their room.
	for (;;) {
		const short = next.map((r, i) => Math.max(0, min[i] - r));
		const owed = short.reduce((a, b) => a + b, 0);
		if (owed <= 1e-9) break;
		const room = next.map((r, i) => Math.max(0, r - min[i]));
		const spare = room.reduce((a, b) => a + b, 0);
		for (let i = 0; i < count; i++) {
			if (short[i] > 0) next[i] = min[i];
			else next[i] -= (owed * room[i]) / spare;
		}
	}
	return evenOut(next, count);
}

/** Fill a tab — an empty one, usually — with a widget. */
export function setWidget(layout: Layout, tabId: NodeId, widget: Widget): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => ({
			...pane,
			tabs: pane.tabs.map((tab) => (tab.id !== tabId ? tab : { ...tab, content: widget }))
		}))
	);
}

/**
 * A tab stops holding a saved layout (da-8o07).
 *
 * The identity is the NAME (shell.ts), and a tab that carries one is a
 * COPY of that layout — so a tab given a widget of its own is no longer
 * that copy: the name goes, and with it the title `inline` lifted off the
 * save. What is left is a plain tab wearing its widget's own title. The
 * save itself is not this module's business and is untouched.
 */
export function shedLayout(layout: Layout, tabId: NodeId): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) => ({
			...pane,
			tabs: pane.tabs.map((tab) => {
				if (tab.id !== tabId || tab.layout === undefined) return tab;
				const { layout: _name, title: _title, ...rest } = tab;
				return rest;
			})
		}))
	);
}

/** Every node id in the tree, for a uniqueness check. */
export function allIds(layout: Layout): NodeId[] {
	const ids: NodeId[] = [];
	for (const pane of panes(layout.root)) {
		ids.push(pane.id);
		for (const tab of pane.tabs) {
			ids.push(tab.id);
			ids.push(tab.content.id);
		}
	}
	return ids;
}

/**
 * Whether a layout is canonical — what `normalize` guarantees, stated
 * so a test can check it after any sequence of operations.
 */
export function isCanonical(layout: Layout): boolean {
	for (const pane of panes(layout.root)) {
		if (pane.tabs.length === 0) return false;
		if (!Number.isInteger(pane.active) || pane.active < 0 || pane.active >= pane.tabs.length) return false;
		for (const tab of pane.tabs) {
			if (!isSplit(tab.content)) continue;
			const { children, ratios } = tab.content;
			if (children.length < (tab.layout === undefined ? 2 : 1) || ratios.length !== children.length) return false;
			if (Math.abs(ratios.reduce((a, b) => a + b, 0) - 1) > 1e-9) return false;
		}
	}
	return true;
}

// ---- floors ----------------------------------------------------------

export interface Floor {
	width: number;
	height: number;
}

/**
 * The least box a pane can be drawn in: the largest floor among its
 * tabs' kinds (`minOf`, given the widget's settings — Layout.svelte has
 * two: the drag's gives none, so a pane drags down to its frame, da-zh2o,
 * and the root's minimum gives each kind's `legible`), where a tab
 * holding a split needs its children's floors laid along the split's
 * direction with a `border`
 * between each pair — plus the frame's own `chrome` (its title bar and
 * border; asked per pane, since a holder draws no bar and a tabbed one
 * a row of its own) on top. The renderer sets this as the pane's
 * `min-*`, which is what makes a split refuse to drag a child below it
 * and show every child at its floor, scrolling, when the box cannot fit
 * them all.
 */
export function floorOf(
	pane: Pane,
	minOf: (kind: string, settings: Record<string, unknown>) => Floor,
	border: number,
	chrome: (pane: Pane) => Floor
): Floor {
	let width = 0;
	let height = 0;
	for (const tab of pane.tabs) {
		const f = isSplit(tab.content) ? splitFloor(tab.content) : minOf(tab.content.kind, tab.content.settings);
		width = Math.max(width, f.width);
		height = Math.max(height, f.height);
	}
	const c = chrome(pane);
	return { width: width + c.width, height: height + c.height };

	function splitFloor(split: Split): Floor {
		const floors = split.children.map((child) => floorOf(child, minOf, border, chrome));
		const gaps = border * (split.children.length - 1);
		const widths = floors.map((f) => f.width);
		const heights = floors.map((f) => f.height);
		return split.direction === 'row'
			? { width: widths.reduce((a, b) => a + b, 0) + gaps, height: Math.max(...heights) }
			: { width: Math.max(...widths), height: heights.reduce((a, b) => a + b, 0) + gaps };
	}
}

/**
 * The root's least box: its `floorOf` less its own frame, which the root
 * draws around the box rather than inside it.
 */
export function rootFloorOf(
	root: Pane,
	minOf: (kind: string, settings: Record<string, unknown>) => Floor,
	border: number,
	chrome: (pane: Pane) => Floor
): Floor {
	const f = floorOf(root, minOf, border, chrome);
	const c = chrome(root);
	return { width: f.width - c.width, height: f.height - c.height };
}
