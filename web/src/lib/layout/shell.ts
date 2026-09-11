// The shell's root tabs: layouts inlined into the one tree (da-0a5o).
//
// The page is one layout tree (`current.ts`), and the root pane's tabs are
// the page's top bar. A root tab whose `layout` is set (types.ts) HOLDS
// that saved layout — its content is the saved tree inlined, so the
// engine, `apply` and the draft never learn that layouts exist. What this
// module owns is the seam between a tab and its save: `inline` (a saved
// `Layout` → the tab that holds it) and `extract` (the tab → the `Layout`
// to save or compare), the boot that fills the root from this device's
// list of open layouts, and the four acts on the active tab — save,
// restore, save as, lock — which the root bar's icons call.
//
// Ids: a saved layout's ids are unique within it and no further. Two
// root tabs made from one save (`day` and a `mine` saved from it) share
// every id, and the tree operations find a pane by id — a split on one
// tab's `tail` would split both. So `inline` re-mints an id that is
// already in the tree as `<id>@<name>`, and `extract` strips that suffix
// again, so the save keeps the ids it had and nothing has to remember a
// map. Only colliding ids are touched: the first `day` keeps `tail`,
// `strip` and the rest, which is how a test and a reader name them.
//
// Modified, per tab, is the tab's `extract` against the save's own
// canonical form (`baselineOf`), never the save as stored: inlining drops
// the root pane's own id and bar — the root bar draws the tab, not the
// pane's frame — and comparing to the raw save would report that drop as
// an edit forever. A lock is per device (vividn, 2026-09-11 12:56) and is
// stripped by `extract`, so it is never saved and never an edit.
//
// A layout may be inlined deeper too — a tab anywhere with `layout` set,
// from the picker — and is expanded on load from its own save, with one
// guard: a name already open above it (`day` inside `day`) is a notice,
// not a recursion. A nested layout is a view of that layout: the outer
// save keeps only the reference (`extract` stubs it), and edits made
// inside it belong to nobody's save — open the layout as a root tab to
// change it.

import { derived, get, writable } from 'svelte/store';
import { onExternalChange } from '$lib/changes.js';
import { initDb } from '$lib/db.js';
import type { Layout, NodeId, Pane, Split, Tab, Widget } from './types.js';
import { LAYOUT_VERSION, isSplit } from './types.js';
import { apply, layout, loaded } from './current.js';
import { ALIASES, PHONE_WIDTH, builtinLayout } from './defaults.js';
import {
	clearDraft,
	isModified,
	loadDraft,
	loadLocked,
	loadRoot,
	saveDraft,
	saveLocked,
	saveRoot
} from './draft.js';
import {
	loadLayout,
	restore as restoreSaved,
	saveAs as saveAsSaved,
	saveLayout,
	versions
} from './store.js';
import { EMPTY_KIND, allIds, closeTab, newId, normalize, setActive, setLocked } from './tree.js';

export { versions };

/** The widget kind a refused or missing layout is shown as. */
export const NOTICE_KIND = 'notice';

/** The layout every device starts with. */
const HOME = 'day';

const BAR = { edge: 'top', align: 'start' } as const;

const phone = () =>
	typeof matchMedia === 'function' && matchMedia(`(max-width: ${PHONE_WIDTH}px)`).matches;

/** The name a URL or a picker means: the old routes and kind names map to one layout each. */
export const canonicalName = (name: string): string => ALIASES[name] ?? name;

/** The saved layout `name`, else the built-in one, else null. */
export async function resolve(name: string): Promise<Layout | null> {
	return (await loadLayout(name).catch(() => null)) ?? builtinLayout(name, phone());
}

// ---- walking ----------------------------------------------------------

/** Rebuild a pane with `f` applied to every tab, deepest first. */
function mapTabs(pane: Pane, f: (tab: Tab) => Tab): Pane {
	return {
		...pane,
		tabs: pane.tabs.map((tab) =>
			f(
				isSplit(tab.content)
					? { ...tab, content: { ...tab.content, children: tab.content.children.map((c) => mapTabs(c, f)) } }
					: tab
			)
		)
	};
}

/** Every id in a pane, mapped through `f`. */
function mapIds(pane: Pane, f: (id: NodeId) => NodeId): Pane {
	const content = (c: Widget | Split): Widget | Split =>
		isSplit(c) ? { ...c, id: f(c.id), children: c.children.map((child) => mapIds(child, f)) } : { ...c, id: f(c.id) };
	return { ...pane, id: f(pane.id), tabs: pane.tabs.map((tab) => ({ ...tab, id: f(tab.id), content: content(tab.content) })) };
}

const without = <T extends object, K extends keyof T>(o: T, key: K): Omit<T, K> => {
	const copy = { ...o };
	delete copy[key];
	return copy;
};

// ---- inline / extract -------------------------------------------------

/**
 * The tab that holds `saved`, its ids re-minted where `taken` already has
 * them (and the new ones added to `taken`). A root pane with one tab is
 * lifted — the tab's content becomes the root tab's, under the pane's id
 * — and one with several is kept whole under a one-child split, which
 * the canon allows only under a layout tab (tree.ts).
 */
export function inline(saved: Layout, taken: Set<NodeId>): Tab {
	const name = saved.name;
	const mint = (id: NodeId): NodeId => {
		let next = taken.has(id) ? `${id}@${name}` : id;
		if (taken.has(next)) next = newId();
		taken.add(next);
		return next;
	};
	const root = mapIds(
		mapTabs(saved.root, (tab) => without(tab, 'locked')),
		mint
	);
	if (root.tabs.length === 1) {
		const only = root.tabs[0];
		return { id: root.id, ...(only.title === undefined ? {} : { title: only.title }), content: only.content, layout: name };
	}
	return { id: mint(newId()), content: { id: mint(newId()), direction: 'column', ratios: [1], children: [root] }, layout: name };
}

/**
 * The layout a tab holds, as it is saved and compared: the lift and the
 * re-minting undone, the lock (per device) and every nested layout's
 * content (its own save's) left out.
 */
export function extract(tab: Tab): Layout {
	const name = tab.layout ?? '';
	const suffix = `@${name}`;
	const unmint = (id: NodeId) => (id.endsWith(suffix) ? id.slice(0, -suffix.length) : id);
	const content = tab.content;
	// The lifted root pane is rebuilt from the tab's UNMINTED id: the inner
	// tab's id derives from it, and `<id>@<name>-tab` is not a suffix
	// `unmint` would strip.
	const id = unmint(tab.id);
	const root: Pane =
		isSplit(content) && content.children.length === 1
			? content.children[0]
			: {
					id,
					tabs: [{ id: `${id}-tab`, ...(tab.title === undefined ? {} : { title: tab.title }), content }],
					active: 0,
					bar: BAR
				};
	const clean = mapTabs(mapIds(root, unmint), (t) => {
		const bare = without(t, 'locked');
		return bare.layout === undefined ? bare : { ...bare, content: { id: `${bare.id}-stub`, kind: EMPTY_KIND, settings: {} } };
	});
	return { version: LAYOUT_VERSION, name, root: clean };
}

/** The form a save is compared against: what it looks like once inlined and taken back out. */
const baselineOf = (saved: Layout): Layout => extract(inline(normalize(saved), new Set()));

// ---- expanding nested layouts ------------------------------------------

const notice = (text: string): Widget => ({ id: newId(), kind: NOTICE_KIND, settings: { text } });

async function expandContent(content: Widget | Split, path: string[], taken: Set<NodeId>): Promise<Widget | Split> {
	if (!isSplit(content)) return content;
	const children: Pane[] = [];
	for (const child of content.children) {
		const tabs: Tab[] = [];
		for (const tab of child.tabs) tabs.push(await expandTab(tab, path, taken));
		children.push({ ...child, tabs });
	}
	return { ...content, children };
}

/** A root tab: what it holds is expanded under its own name, so it may not hold itself. */
async function expandRoot(tab: Tab, taken: Set<NodeId>): Promise<Tab> {
	return { ...tab, content: await expandContent(tab.content, [tab.layout as string], taken) };
}

/** A tab with its nested layouts (any depth) filled in from their saves. */
async function expandTab(tab: Tab, path: string[], taken: Set<NodeId>): Promise<Tab> {
	if (tab.layout === undefined) return { ...tab, content: await expandContent(tab.content, path, taken) };
	const name = tab.layout;
	if (path.includes(name)) return { ...tab, content: notice(`the layout “${name}” cannot hold itself`) };
	const saved = await resolve(name);
	if (!saved) return { ...tab, content: notice(`no layout named “${name}”`) };
	const inlined = inline(normalize(saved), taken);
	return { ...tab, content: await expandContent(inlined.content, [...path, name], taken) };
}

// ---- the root ---------------------------------------------------------

/** Each open layout's save, in `baselineOf` form, by name. */
const baselines = writable<Record<string, Layout>>({});

export const rootTabs = (l: Layout): Tab[] => l.root.tabs;

/** The open layouts whose tab differs from its save. */
export const modified = derived([layout, baselines], ([$layout, $baselines]) => {
	const names = new Set<string>();
	for (const tab of rootTabs($layout)) {
		const base = tab.layout === undefined ? undefined : $baselines[tab.layout];
		if (base && isModified(extract(tab), base)) names.add(tab.layout as string);
	}
	return names;
});

function tabOf(name: string): Tab | undefined {
	return rootTabs(get(layout)).find((t) => t.layout === name);
}

/** A root tab for `name` — the draft this device has of it, else its save — or null for an unknown name. */
async function openTab(name: string, taken: Set<NodeId>): Promise<Tab | null> {
	const saved = await resolve(name);
	if (!saved) return null;
	baselines.update((b) => ({ ...b, [name]: baselineOf(saved) }));
	const source = loadDraft(name) ?? saved;
	const tab = await expandRoot(inline(normalize({ ...source, name }), taken), taken);
	return loadLocked(name) ? { ...tab, locked: true } : tab;
}

let booted = false;
let booting: Promise<void> | null = null;

/** Fill the root from this device's open layouts (`day` alone at first). Once. */
export function boot(): Promise<void> {
	return (booting ??= (async () => {
		await initDb().catch(() => {});
		const state = loadRoot() ?? { names: [HOME], active: 0 };
		const taken = new Set<NodeId>();
		const tabs: Tab[] = [];
		for (const name of state.names) {
			const tab = await openTab(name, taken);
			if (tab) tabs.push(tab);
		}
		if (tabs.length === 0) tabs.push((await openTab(HOME, taken)) as Tab);
		layout.set(
			normalize({
				version: LAYOUT_VERSION,
				name: 'root',
				root: { id: 'shell', tabs, active: Math.min(state.active, tabs.length - 1), bar: BAR }
			})
		);
		booted = true;
		loaded.set(true);
		onExternalChange(() => void followSaves(), '_internal');
	})());
}

// Every change to the tree, and every move of a baseline (a save, a
// restore, another tab's save): the open list and the active tab to this
// device, and each tab's draft — kept while it differs from its save,
// cleared once it does not.
function settle(): void {
	if (!booted) return;
	const $layout = get(layout);
	const tabs = rootTabs($layout);
	saveRoot({ names: tabs.map((t) => t.layout).filter((n): n is string => n !== undefined), active: $layout.root.active });
	const bases = get(baselines);
	for (const tab of tabs) {
		if (tab.layout === undefined) continue;
		const tree = extract(tab);
		const base = bases[tab.layout];
		if (base && !isModified(tree, base)) clearDraft(tab.layout);
		else saveDraft(tree);
	}
}
layout.subscribe(settle);
baselines.subscribe(settle);

/** Replace the tab holding `name` with `saved`, re-inlined; `unless` may veto at the last instant. */
async function replaceTab(name: string, saved: Layout, unless?: (tab: Tab) => boolean): Promise<void> {
	const others = new Set(allIds(get(layout)).filter((id) => !idsOf(tabOf(name)).has(id)));
	const next = await expandRoot(inline(normalize({ ...saved, name }), others), others);
	apply((l) => {
		const at = rootTabs(l).findIndex((t) => t.layout === name);
		if (at < 0) return l;
		const was = rootTabs(l)[at];
		if (unless?.(was)) return l;
		const tabs = [...l.root.tabs];
		tabs[at] = { ...next, ...(was.locked ? { locked: true } : {}) };
		return normalize({ ...l, root: { ...l.root, tabs } });
	});
}

function idsOf(tab: Tab | undefined): Set<NodeId> {
	if (!tab) return new Set();
	return new Set(allIds({ version: LAYOUT_VERSION, name: '', root: { id: '', tabs: [tab], active: 0, bar: BAR } }).filter(Boolean));
}

/**
 * Another tab saved. Each open layout's baseline moves to the new save,
 * and a tab nobody edited here moves with it. Whether it was edited is
 * decided AFTER the load, against the baseline it was edited from, and
 * again at the instant of the swap — so an edit made while the load was
 * in flight is kept, dot and all (the review thread on #168).
 */
async function followSaves(): Promise<void> {
	for (const tab of rootTabs(get(layout))) {
		const name = tab.layout;
		if (name === undefined) continue;
		const next = await loadLayout(name).catch(() => null);
		if (!next) continue;
		const before = get(baselines)[name];
		const base = baselineOf(next);
		baselines.update((b) => ({ ...b, [name]: base }));
		const edited = (t: Tab) => !!before && isModified(extract(t), before);
		const now = tabOf(name);
		if (!now || edited(now)) continue;
		await replaceTab(name, next, edited);
	}
}

// ---- the acts ---------------------------------------------------------

/**
 * Open `name` — a saved layout, a built-in, a widget kind — as a root tab,
 * or bring it to the front if it is one already. False for a name that is
 * none of those.
 */
export async function open(name: string): Promise<boolean> {
	await boot();
	const n = canonicalName(name);
	const at = rootTabs(get(layout)).findIndex((t) => t.layout === n);
	if (at >= 0) {
		apply((l) => setActive(l, l.root.id, at));
		return true;
	}
	const tab = await openTab(n, new Set(allIds(get(layout))));
	if (!tab) return false;
	apply((l) => normalize({ ...l, root: { ...l.root, tabs: [...l.root.tabs, tab], active: l.root.tabs.length } }));
	return true;
}

export const activate = (index: number) => apply((l) => setActive(l, l.root.id, index));

/** Close a root tab; the last one stays (tree.ts). Its draft, if any, is kept for next time. */
export const close = (index: number) => apply((l) => closeTab(l, l.root.id, index));

/** Save the tab's tree as `name`'s new version; the tab is then unmodified. */
export async function save(name: string): Promise<void> {
	const tab = tabOf(name);
	if (!tab) return;
	const tree = extract(tab);
	await saveLayout(tree);
	baselines.update((b) => ({ ...b, [name]: baselineOf(tree) }));
}

/**
 * Back to the save: the last one, or `version` of it (a new version, as
 * `undo` makes one — the way forward again is `restore(name)` with none).
 */
export async function restore(name: string, version?: number): Promise<void> {
	const saved = version === undefined && get(modified).has(name) ? await resolve(name) : await restoreSaved(name, version);
	if (!saved) return;
	baselines.update((b) => ({ ...b, [name]: baselineOf(saved) }));
	await replaceTab(name, saved);
}

/** The tab's tree under a new name, and the tab pointed at it; the original keeps its save. */
export async function saveAs(name: string, newName: string): Promise<void> {
	const tab = tabOf(name);
	if (!tab || !newName || newName === name) return;
	const copy = await saveAsSaved(extract(tab), newName);
	// The tab first, then its baseline, then the old draft: `settle` runs
	// on each, and would write the edit back under the old name if the tab
	// still carried it when the new baseline landed.
	apply((l) => ({
		...l,
		root: { ...l.root, tabs: l.root.tabs.map((t) => (t.layout === name ? { ...t, layout: newName } : t)) }
	}));
	baselines.update((b) => ({ ...b, [newName]: baselineOf(copy) }));
	clearDraft(name);
	await replaceTab(newName, copy);
}

/** Lock or unlock a root tab, on this device only. */
export function toggleLock(name: string): void {
	const tab = tabOf(name);
	if (!tab) return;
	const locked = !tab.locked;
	saveLocked(name, locked);
	apply((l) => setLocked(l, tab.id, locked));
}

/** Fill a tab anywhere in the tree with layout `name`, inlined; the picker's other choice. */
export async function inlineInto(tabId: NodeId, name: string): Promise<void> {
	const taken = new Set(allIds(get(layout)));
	const filled = await expandTab({ id: tabId, content: notice(''), layout: name }, ancestorsOf(tabId), taken);
	apply((l) => {
		const tabs = (pane: Pane): Pane => ({
			...pane,
			tabs: pane.tabs.map((t) =>
				t.id === tabId
					? { ...t, layout: name, content: filled.content }
					: isSplit(t.content)
						? { ...t, content: { ...t.content, children: t.content.children.map(tabs) } }
						: t
			)
		});
		return normalize({ ...l, root: tabs(l.root) });
	});
}

/** The layout names open above `tabId` (the tab's own included, if it is one): what it may not hold. */
export function ancestorsOf(tabId: NodeId): string[] {
	const walk = (pane: Pane, path: string[]): string[] | null => {
		for (const tab of pane.tabs) {
			const here = tab.layout === undefined ? path : [...path, tab.layout];
			if (tab.id === tabId) return here;
			if (isSplit(tab.content))
				for (const child of tab.content.children) {
					const found = walk(child, here);
					if (found) return found;
				}
		}
		return null;
	};
	return walk(get(layout).root, []) ?? [];
}
