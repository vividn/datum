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

import type { Bar, Layout, NodeId, Pane, Split, Tab, Widget } from './types.js';

/** The kind an empty tab holds: the picker, until a widget is chosen. */
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
 * pane holding one empty tab. All its tabs travel with it. When the pane
 * already sits in a split running the same way, the new pane simply joins
 * that split after it, taking half its share — the same picture with one
 * fewer level of tree.
 */
export function splitPane(layout: Layout, paneId: NodeId, direction: Split['direction']): Layout {
	let done = false;
	const joined = mapPanes(layout.root, (pane) => ({
		...pane,
		tabs: pane.tabs.map((tab) => {
			if (!isSplit(tab.content) || tab.content.direction !== direction) return tab;
			const at = tab.content.children.findIndex((child) => child.id === paneId);
			if (at < 0) return tab;
			done = true;
			const children = [...tab.content.children];
			const ratios = [...tab.content.ratios];
			children.splice(at + 1, 0, newPane());
			ratios.splice(at, 1, ratios[at] / 2, ratios[at] / 2);
			return { ...tab, content: { ...tab.content, children, ratios } };
		})
	}));
	if (done) return withRoot(layout, joined);
	const wrapped = mapPanes(layout.root, (pane) =>
		pane.id === paneId ? newPane([newTab(newSplit(direction, [pane, newPane()]))]) : pane
	);
	return withRoot(layout, wrapped);
}

/**
 * Split one tab of a pane: its content becomes a split of [a new pane
 * holding the old content, a new empty pane]. Only with two or more tabs
 * — with one, this would draw the same picture as `splitPane`, so it is
 * refused (vividn, 15:51).
 */
export function splitTab(
	layout: Layout,
	paneId: NodeId,
	tabIndex: number,
	direction: Split['direction']
): Layout {
	const pane = findPane(layout, paneId);
	if (!pane || pane.tabs.length < 2 || !pane.tabs[tabIndex]) return layout;
	return withRoot(
		layout,
		mapPanes(layout.root, (p) =>
			p.id !== paneId
				? p
				: {
						...p,
						tabs: p.tabs.map((tab, i) =>
							i !== tabIndex
								? tab
								: {
										...tab,
										content: newSplit(direction, [newPane([newTab(tab.content, tab.title)]), newPane()])
									}
						)
					}
		)
	);
}

/** A new empty tab at the end of the pane, made active. */
export function addTab(layout: Layout, paneId: NodeId): Layout {
	return withRoot(
		layout,
		mapPanes(layout.root, (pane) =>
			pane.id !== paneId ? pane : { ...pane, tabs: [...pane.tabs, newTab()], active: pane.tabs.length }
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
			if (children.length < 2 || ratios.length !== children.length) return false;
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
 * tabs' kinds (`minOf`, from the registry), where a tab holding a split
 * needs its children's floors laid along the split's direction with a
 * `border` between each pair — plus the frame's own `chrome` (its title
 * bar and border) on top. The renderer sets this as the pane's `min-*`,
 * which is what makes a split refuse to drag a child below it and show
 * every child at its floor, scrolling, when the box cannot fit them all.
 */
export function floorOf(
	pane: Pane,
	minOf: (kind: string) => Floor,
	border: number,
	chrome: Floor
): Floor {
	let width = 0;
	let height = 0;
	for (const tab of pane.tabs) {
		const f = isSplit(tab.content) ? splitFloor(tab.content) : minOf(tab.content.kind);
		width = Math.max(width, f.width);
		height = Math.max(height, f.height);
	}
	return { width: width + chrome.width, height: height + chrome.height };

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
