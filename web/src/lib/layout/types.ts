// The layout tree — the shape every part of the flexible dashboard agrees on.
//
// Decided with vividn on 2026-09-10 (the design conversation is in vremya's
// notes, `dashboard-layout-design-2026-09-10.md`). This file is the contract
// between the engine (the renderer and the tree operations), persistence
// (layouts are datum entries in an internal database), the widgets and the
// kits that ship layouts, so that they can be built in parallel. Types and
// the version only: no functions live here.
//
// The words, in vividn's vocabulary: a PANE is a holder, a WIDGET is the
// thing inside it. A pane has one or more TABS; a tab holds either a widget
// or a SPLIT, and a split holds panes — so it is trees all the way down.
// The dashboard's own top-level tabs are simply the root pane's tabs, and a
// saved layout can be embedded in any pane as a widget of kind `layout`.
//
// (The fields widget's own buttons were once called "widgets" too — the
// older sense, da-nz29; they are `FieldButton`s in `fields.ts` now, and the
// widget as a whole is one widget KIND here.)

/** Bumped when the shape below changes. A stored tree carries its version. */
export const LAYOUT_VERSION = 1;

/** Stable within one layout; minted when a node is created, never reused. */
export type NodeId = string;

/** Where a pane's title bar (and its tabs, when it has several) sits. */
export type BarEdge = 'top' | 'bottom' | 'left' | 'right';

/** Where along that edge the title and the tabs are gathered. */
export type BarAlign = 'start' | 'center' | 'end';

export interface Bar {
	edge: BarEdge;
	align: BarAlign;
}

/**
 * A widget instance: a registered kind plus its own settings.
 *
 * `settings` is the widget's to interpret (the day strip's bar height, the
 * command pane's argv, an embedded layout's name) and is saved with the
 * tree, which is how two day views can differ. Unknown keys are kept, not
 * dropped: a newer app may have written them.
 */
export interface Widget {
	id: NodeId;
	kind: string;
	settings: Record<string, unknown>;
}

/**
 * A split: panes side by side (`row`) or stacked (`column`).
 *
 * `ratios` has one entry per child and sums to 1; dragging the border
 * between two children moves weight from one to the other and leaves the
 * rest alone. A split always has at least two children — one child is not
 * a split, and the tree operations collapse it (auto-unnest).
 */
export interface Split {
	id: NodeId;
	direction: 'row' | 'column';
	ratios: number[];
	children: Pane[];
}

/** One tab of a pane: a widget, or a split of further panes (nesting). */
export interface Tab {
	id: NodeId;
	/** Shown on the tab; when absent, the widget kind's own title is used. */
	title?: string;
	/** Nothing under this tab can be split, tabbed or resized (da-2a5y). */
	locked?: boolean;
	content: Widget | Split;
	/**
	 * The saved layout this tab holds, inlined (da-0a5o): its content IS
	 * that layout's tree, and the shell saves and restores it by this name.
	 */
	layout?: string;
}

/**
 * A pane: the holder. Always at least one tab. The bar carries the title
 * when there is one tab and the tab strip when there are more; swiping
 * across the pane's content moves between tabs along the bar's axis.
 */
export interface Pane {
	id: NodeId;
	tabs: Tab[];
	/** Index into `tabs`. */
	active: number;
	bar: Bar;
}

/**
 * A whole layout, as stored: one root pane.
 *
 * `name` is the layout's identity (the datum entry's id); `designedFor`
 * is advisory — any layout may be shown on any device, and vividn wants the
 * choice left to the user, so this only helps a picker sort.
 */
export interface Layout {
	version: typeof LAYOUT_VERSION;
	name: string;
	root: Pane;
	designedFor?: 'mobile' | 'desktop';
}

/**
 * What the registry knows about a widget kind, independent of Svelte.
 *
 * The component that draws the kind is registered beside this in the
 * engine; this is the part persistence, the picker and the kit tooling can
 * read without importing a component. A kind has no floor: a pane shrinks
 * until only its frame is left (da-zh2o).
 */
export interface WidgetKindMeta {
	kind: string;
	/** The title the pane shows when the tab has none of its own. */
	title: string;
	/** What a fresh instance starts with. */
	defaultSettings: Record<string, unknown>;
}

/** Type guards, so a tab's content can be told apart without a `kind` tag. */
export const isSplit = (content: Widget | Split): content is Split => 'children' in content;
export const isWidget = (content: Widget | Split): content is Widget => 'kind' in content;
