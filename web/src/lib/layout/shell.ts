// The shell: layouts inlined into the one tree, at any depth (da-0a5o,
// da-crp5).
//
// The page is one layout tree (`current.ts`), and the root pane's tabs are
// the page's top bar. A tab whose `layout` is set (types.ts) HOLDS that
// saved layout — its content is the saved tree inlined, so the engine,
// `apply` and the draft never learn that layouts exist. What this module
// owns is the seam between a tab and its save: `inline` (a saved `Layout`
// → the tab that holds it) and `extract` (the tab → the `Layout` to save
// or compare), the boot that fills the root from this device's list of
// open layouts, and the acts — save, restore, save as, lock, and reset for
// a layout that shipped with datum (da-luwe) — which the root bar's menu
// and a pane's menu call.
//
// IDENTITY IS THE NAME, and a tab carrying it is a COPY (da-crp5). The
// root bar's tabs have no privilege: a tab six panes down carrying
// `layout` is the same kind of thing, and the baseline, the draft and the
// lock all hang off the NAME, never off where the tab sits. So:
//
//   - `modified` names every COPY whose subtree differs from its save, by
//     tab id, so each copy wears its own dot wherever it is drawn.
//   - the acts take a copy's tab id and find it anywhere in the tree.
//     Saving a copy writes THAT copy's tree under the name — which moves
//     the baseline, and with it every other copy's dot: a second copy of
//     the same layout, left as it was, is then the one that differs. That
//     is what a reader sees, and it is the honest report.
//   - restoring and a save arriving from another tab move every copy of
//     the name that nobody edited here.
//   - the draft (per name, `localStorage`) is written from the FIRST copy
//     in tree order that differs from its save, and cleared when no copy
//     does. One name, one draft: with the same layout open twice and both
//     edited, the outer copy's edit is the one kept over a reload.
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
// A layout is held BY REFERENCE wherever it is nested: the outer save
// keeps only the name and the tab's id (`extract` stubs the content), so
// saving the outer layout never inlines the inner one into it, and an edit
// made inside the inner one goes to the inner one's own save and draft.
// A nested tab is expanded on load from that draft, else that save, with
// one guard: a name already open above it (`day` inside `day`) is a
// notice, not a recursion.
//
// A PREVIEW is a root tab that is none of this device's business
// (da-xamd). `/<name>?from=<layout>` seeds one with another layout's tree
// under a name of its own; it is drawn in italics and nothing about it is
// written down — not the open list, not a draft, not a save — so it lives
// exactly as long as the page does. `previews` holds those names, and
// `settle` reads it at the moment it writes, which is the only place the
// exclusion can be honest: every change to the tree comes back through
// there. `Keep tab` is the door out, and it is a save under the name.
//
// A nested tab takes the id `inline` gives it — the saved root pane's,
// re-minted where it is taken — rather than the id the outer tree had for
// it, because `extract` rebuilds the root pane from the tab's own id, and
// a tab whose id was the outer tree's would differ from its save forever.
// That id is then what the outer save records, and `expandTab` lets the
// tab's own id go before re-inlining, so the second copy of one layout
// takes `<id>@<name>` again on every load instead of drifting to a fresh
// id and reporting the outer layout as modified.

import { derived, get, writable } from 'svelte/store';
import { onExternalChange } from '$lib/changes.js';
import { currentDb } from '$lib/database.js';
import { initDb } from '$lib/db.js';
import type { Layout, NodeId, Pane, Split, Tab, Widget } from './types.js';
import { LAYOUT_VERSION, isSplit } from './types.js';
import { apply, layout, loaded } from './current.js';
import { ALIASES, BUILTIN_NAMES, PHONE_WIDTH, builtinLayout } from './defaults.js';
import {
	clearDraft,
	clearLocked,
	isModified,
	loadDraft,
	loadLocked,
	loadRoot,
	saveDraft,
	saveLocked,
	saveRoot,
	type RootState
} from './draft.js';
import {
	deleteLayout,
	kitLayout,
	listLayouts,
	loadLayout,
	refreshKitLayouts,
	restore as restoreSaved,
	saveAs as saveAsSaved,
	saveLayout,
	versions
} from './store.js';
import { EMPTY_KIND, allIds, closeTab, findTab, newId, newWidget, normalize, setActive, setLocked, setWidget, shedLayout } from './tree.js';
import { unmaximize } from './maximize.js';
import { defaultsOfKind } from './registry.js';

export { versions };

/** The widget kind a refused or missing layout is shown as. */
export const NOTICE_KIND = 'notice';

const BAR = { edge: 'top', align: 'start' } as const;

const phone = () =>
	typeof matchMedia === 'function' && matchMedia(`(max-width: ${PHONE_WIDTH}px)`).matches;

/**
 * The layout a device starts with: `mobile` on a phone (da-shzs), `day`
 * elsewhere. Asked only where nothing is stored — once the open list is
 * written, turning or widening the screen never swaps it.
 */
const home = () => (phone() ? 'mobile' : 'day');

/** The root bar a fresh install shows: the one layout, in front. */
const fresh = (): RootState => ({ names: [home()], active: 0 });

/** This screen, in the word a layout's `designedFor` uses. */
export const device = (): NonNullable<Layout['designedFor']> => (phone() ? 'mobile' : 'desktop');

/**
 * Whether `l` may SHADOW a same-named layout here (vividn, 2026-09-15 18:01):
 * "load a different set of layouts by default when the device is detected
 * to be mobile, but still let them both load each other's custom layouts".
 *
 * So `designedFor` is not a filter — a layout designed for the other device
 * is still listed and still opens by name. It only decides who WINS a name
 * three things may answer to: a layout that says nothing suits either
 * device and shadows; one that names the other device stands aside.
 */
const forHere = (l: { designedFor?: Layout['designedFor'] } | null): boolean =>
	!!l && (!l.designedFor || l.designedFor === device());

/**
 * Of the candidates for one name, in precedence order (the person's save,
 * then the kit's, then the built-in), the first that may shadow here — and
 * failing that the first that EXISTS, so a mobile-only layout still opens
 * on a desktop when nothing else answers to its name.
 */
const pick = (...candidates: (Layout | null)[]): Layout | null =>
	candidates.find(forHere) ?? candidates.find(Boolean) ?? null;

/** The name a URL or the chooser means: the old routes and kind names map to one layout each. */
export const canonicalName = (name: string): string => ALIASES[name] ?? name;

/**
 * The layout `name` means here — the save, else the kit's, else the
 * built-in — and whether it SHIPPED (a kit's or a built-in) rather than
 * being the person's own save: what a lock defaults to (da-gshh).
 */
async function meaning(name: string): Promise<{ layout: Layout | null; shipped: boolean }> {
	const own = await loadLayout(name).catch(() => null);
	const layout = pick(own, kitLayout(name), builtinLayout(name, phone()));
	return { layout, shipped: !!layout && layout !== own };
}

/** The layout `name` means here: the save, else the kit's, else the built-in. */
export const resolve = async (name: string): Promise<Layout | null> => (await meaning(name)).layout;

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
 * The kinds that have been RETIRED, and what a layout naming one opens as.
 *
 * A kind's id is a saved-layout key (docs/web-conventions.md #10), so a
 * kind cannot simply go: every layout and draft already stored on a
 * device still names it, and an unknown kind draws as an untitled empty
 * pane.
 *
 * EMPTY, and kept: `input` was the one entry (da-22vg, `input` → `repl`)
 * and the input widget is back under that very id (da-3zon), so a layout
 * saved with one opens as one again — which is the whole point of a kind's
 * id being a saved-layout key. The next kind to go writes its arm here.
 *
 * Here rather than in `tree.ts` because this is where layouts are READ:
 * `inline` is the one door every save, draft and built-in comes through,
 * and `extract` writes the new name back out — so a layout migrates once,
 * against its own baseline, and stays migrated. `normalize` would run it
 * on every drag of a border instead, and would give the tree's shape
 * module an opinion about kinds, which it has never had.
 */
const RETIRED_KINDS: Record<string, string> = {};

/** A tab whose widget is a retired kind, as the kind it became. */
const migrated = (tab: Tab): Tab => {
	if (isSplit(tab.content)) return tab;
	const kind = RETIRED_KINDS[tab.content.kind];
	return kind === undefined ? tab : { ...tab, content: { ...tab.content, kind } };
};

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
		mapTabs(saved.root, (tab) => migrated(without(tab, 'locked'))),
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
		// A nested layout is kept by REFERENCE: its name and the id its tab
		// takes, and nothing else — not its content and not its title, which
		// are its own save's and would otherwise report this layout as
		// modified whenever that one changed.
		return bare.layout === undefined
			? bare
			: { id: bare.id, layout: bare.layout, content: { id: `${bare.id}-stub`, kind: EMPTY_KIND, settings: {} } };
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

/**
 * The tab holding layout `name` with `source` as its tree — its save, its
 * draft, a version restored — its own nested layouts expanded under it,
 * and this device's lock for the name on it. `path` is the names open
 * above it, `name` included.
 *
 * A device that has never locked or unlocked the name gets the lock a
 * first open does (da-gshh): a layout that SHIPPED opens locked, one a
 * person made opens unlocked.
 */
async function tabHolding(name: string, source: Layout, path: string[], taken: Set<NodeId>, shipped = false): Promise<Tab> {
	const inlined = inline(normalize({ ...source, name }), taken);
	const filled: Tab = { ...inlined, content: await expandContent(inlined.content, path, taken) };
	return (loadLocked(name) ?? shipped) ? { ...filled, locked: true } : filled;
}

/** A tab with its nested layouts (any depth) filled in from their drafts, else their saves. */
async function expandTab(tab: Tab, path: string[], taken: Set<NodeId>): Promise<Tab> {
	if (tab.layout === undefined) return { ...tab, content: await expandContent(tab.content, path, taken) };
	const name = tab.layout;
	if (path.includes(name)) return { ...tab, content: notice(`the layout “${name}” cannot hold itself`) };
	const { layout: saved, shipped } = await meaning(name);
	if (!saved) return { ...tab, content: notice(`no layout named “${name}”`) };
	baselines.update((b) => ({ ...b, [name]: baselineOf(saved) }));
	// The id this tab has is the one the inner root pane took last time:
	// let it go, so it takes it again rather than minting a new one.
	taken.delete(tab.id);
	return tabHolding(name, loadDraft(name) ?? saved, [...path, name], taken, shipped);
}

// ---- the root ---------------------------------------------------------

/** Each open layout's save, in `baselineOf` form, by name. */
const baselines = writable<Record<string, Layout>>({});

/**
 * The names open as a PREVIEW (da-xamd): a root tab seeded from another
 * layout by `/<name>?from=<layout>` and WRITTEN NOWHERE — not this
 * device's open list, not a draft, not the store. A reload has never heard
 * of it, and `Keep tab` is what makes one real.
 *
 * A store rather than a plain set, because `settle` has to run again when
 * a name joins or leaves it — that is the whole of what "written nowhere"
 * means — and the root bar draws the italics from it.
 */
export const previews = writable<Set<string>>(new Set());

/** Whether `name` is open as a preview: what the italics and `Keep tab` read. */
export const isPreview = (name: string): boolean => get(previews).has(name);

const forget = (name: string) =>
	previews.update((open) => {
		const rest = new Set(open);
		rest.delete(name);
		return rest;
	});

export const rootTabs = (l: Layout): Tab[] => l.root.tabs;

/**
 * Every COPY in the tree — a tab carrying a layout — outermost first and
 * in document order, so the root bar's tabs come before anything nested
 * and "the first copy" is a thing a reader can point at.
 */
export function layoutTabs(l: Layout): Tab[] {
	const found: Tab[] = [];
	const walk = (pane: Pane) => {
		for (const tab of pane.tabs) {
			if (tab.layout !== undefined) found.push(tab);
			if (isSplit(tab.content)) tab.content.children.forEach(walk);
		}
	};
	walk(l.root);
	return found;
}

/** The copies whose subtree differs from their layout's save, BY TAB ID. */
export const modified = derived([layout, baselines], ([$layout, $baselines]) => {
	const ids = new Set<NodeId>();
	for (const tab of layoutTabs($layout)) {
		const base = $baselines[tab.layout as string];
		if (base && isModified(extract(tab), base)) ids.add(tab.id);
	}
	return ids;
});

/** The copy with this tab id, anywhere in the tree. */
const copyOf = (tabId: NodeId): Tab | undefined => layoutTabs(get(layout)).find((t) => t.id === tabId);

/** The first copy of `name`, anywhere in the tree. */
const tabOf = (name: string): Tab | undefined => layoutTabs(get(layout)).find((t) => t.layout === name);

/** A root tab for `name` — the draft this device has of it, else its save — or null for an unknown name. */
async function openTab(name: string, taken: Set<NodeId>): Promise<Tab | null> {
	const { layout: saved, shipped } = await meaning(name);
	if (!saved) return null;
	baselines.update((b) => ({ ...b, [name]: baselineOf(saved) }));
	return tabHolding(name, loadDraft(name) ?? saved, [name], taken, shipped);
}

let booted = false;
let booting: Promise<void> | null = null;

/**
 * Run `act` with `settle` held off, and nothing written until it is whole.
 *
 * The same device `fillRoot` uses for the same reason: there are moments
 * when the tree and what hangs off it — a baseline, a preview's name —
 * belong to two different pages, and a settle in the middle of one writes
 * that disagreement down.
 */
function quiet(act: () => void): void {
	const resting = booted;
	booted = false;
	try {
		act();
	} finally {
		booted = resting;
	}
}

/**
 * The page's tree, built from `state`: one root tab per open layout, each
 * from its draft or its save. The boot's own step, and the one a reset of
 * the defaults takes with `fresh()` (da-jbcv).
 *
 * `settle` is held off until the tree is whole, because a rebuild is the
 * one moment the tree and the baselines belong to two different pages:
 * `openTab` installs a name's baseline BEFORE the tab built from it lands,
 * so a settle in between reads the tree still on screen against the
 * baselines of the tree replacing it, calls it modified, and writes it
 * back out as a draft — which the very next `loadDraft` then builds the
 * new tab from. The boot never saw it, having nothing on screen yet.
 */
async function fillRoot(state: RootState): Promise<void> {
	const resting = booted;
	booted = false;
	try {
		// A name with another spelling opens under the one name, and only
		// once: a device with `now` and `nowview` both open gets one
		// nowview tab, not two tabs wearing the same name.
		const names = [...new Set(state.names.map(canonicalName))];
		const taken = new Set<NodeId>();
		const tabs: Tab[] = [];
		for (const name of names) {
			const tab = await openTab(name, taken);
			if (tab) tabs.push(tab);
		}
		if (tabs.length === 0) tabs.push((await openTab(home(), taken)) as Tab);
		layout.set(
			normalize({
				version: LAYOUT_VERSION,
				name: 'root',
				root: { id: 'shell', tabs, active: Math.min(state.active, tabs.length - 1), bar: BAR }
			})
		);
	} finally {
		booted = resting;
	}
	settle();
}

/** Fill the root from this device's open layouts (`day` or `mobile` alone at first). Once. */
export function boot(): Promise<void> {
	return (booting ??= (async () => {
		await initDb().catch(() => {});
		// Before the first `resolve`: a kit's layout is one of the trees a
		// name may mean, and `shippedTree` reads the cache synchronously.
		await refreshKitLayouts();
		const stored = loadRoot();
		await fillRoot(stored ?? fresh());
		booted = true;
		// A fresh device's home is decided here, once (da-shzs): written now,
		// so a phone turned or widened later does not reopen as a desktop.
		if (!stored) settle();
		loaded.set(true);
		onExternalChange(followSaves, '_internal');
		// A kit installed in another tab — into the data db — changes what
		// the names mean here too. This page's data db (da-vk1x): under
		// `?db=other` the kit layouts are `other`'s.
		onExternalChange(refreshKitLayouts, currentDb());
	})());
}

// Every change to the tree, and every move of a baseline (a save, a
// restore, another tab's save): the open list and the active tab to this
// device, and each NAME's draft — one per name, whatever its depth, taken
// from the first copy that differs from the save and cleared when no copy
// does. A name whose baseline has not landed is left alone: its draft is
// what the tab is about to be built from.
function settle(): void {
	if (!booted) return;
	const $layout = get(layout);
	const shown = get(previews);
	// A PREVIEW IS A NAME A TAB CARRIES (kimi-k3 on #23). `saveAs` renames a
	// copy, the chooser replaces one with another layout or with a bare
	// widget — every act that takes a name off a tab passes through here, so
	// the set is pruned here rather than at each of them, which is the same
	// argument the exclusion below rests on. A name left behind would make a
	// real layout that later took it a preview: italic, kept out of the open
	// list, and passed over by `followSaves`.
	const carried = new Set(layoutTabs($layout).map((t) => t.layout as string));
	if ([...shown].some((name) => !carried.has(name))) {
		// That write runs this again, with the set it leaves behind.
		previews.set(new Set([...shown].filter((name) => carried.has(name))));
		return;
	}
	// A PREVIEW is left out of what this device remembers (da-xamd) — here,
	// where the writing happens, and not merely at the moment it is opened:
	// every change to the tree comes back through this function.
	const kept = (t: Tab): boolean => t.layout !== undefined && !shown.has(t.layout);
	const tabs = rootTabs($layout);
	const names = tabs.filter(kept);
	// The tab a reload lands on, as an index into the names WRITTEN: the one
	// in front when it is written, and otherwise — a preview in front being
	// written nowhere — the last one before it. Worked:
	//
	//   [day, x*]        active 1 → none in front is kept, one before → 0 (day)
	//   [day, mine, x*]  active 2 → two before → 1 (mine)
	//   [x*, day]        active 0 → none before → 0 (day, the only name left)
	const front = names.indexOf(tabs[$layout.root.active]);
	const before = tabs.slice(0, $layout.root.active).filter(kept).length;
	saveRoot({
		names: names.map((t) => t.layout as string),
		active: front >= 0 ? front : Math.max(0, before - 1)
	});
	const bases = get(baselines);
	const drafted = new Set<string>();
	const clean = new Set<string>();
	for (const tab of layoutTabs($layout)) {
		const name = tab.layout as string;
		const base = bases[name];
		if (!base || drafted.has(name) || shown.has(name)) continue;
		const tree = extract(tab);
		if (isModified(tree, base)) {
			drafted.add(name);
			saveDraft(tree);
		} else clean.add(name);
	}
	for (const name of clean) if (!drafted.has(name)) clearDraft(name);
}
layout.subscribe(settle);
baselines.subscribe(settle);
previews.subscribe(settle);

/**
 * Every copy of `name`, at any depth, re-inlined from `saved`; `unless`
 * may veto one at the last instant. The copies' own ids are freed first,
 * so each takes back the id it had rather than minting a fresh one.
 */
async function replaceCopies(name: string, saved: Layout, unless?: (tab: Tab) => boolean): Promise<void> {
	const copies = layoutTabs(get(layout)).filter((t) => t.layout === name);
	if (copies.length === 0) return;
	const mine = new Set(copies.flatMap((t) => [...idsOf(t)]));
	const taken = new Set(allIds(get(layout)).filter((id) => !mine.has(id)));
	const next = new Map<NodeId, Tab>();
	for (const copy of copies) next.set(copy.id, await tabHolding(name, saved, ancestorsOf(copy.id), taken));
	apply((l) =>
		normalize({
			...l,
			root: mapTabs(l.root, (was) => {
				const to = was.layout === name ? next.get(was.id) : undefined;
				if (!to || unless?.(was)) return was;
				// A new tree, the same lock: the copy keeps whichever it wore.
				return { ...without(to, 'locked'), ...(was.locked ? { locked: true } : {}) };
			})
		})
	);
}

function idsOf(tab: Tab | undefined): Set<NodeId> {
	if (!tab) return new Set();
	return new Set(allIds({ version: LAYOUT_VERSION, name: '', root: { id: '', tabs: [tab], active: 0, bar: BAR } }).filter(Boolean));
}

/**
 * Another tab saved. Each open layout's baseline moves to the new save,
 * and every copy nobody edited here moves with it — a nested copy as much
 * as a root tab's. Whether a copy was edited is decided AFTER the load,
 * against the baseline it was edited from, and again at the instant of
 * the swap — so an edit made while the load was in flight is kept, dot
 * and all (the review thread on #168).
 */
async function followSaves(): Promise<void> {
	for (const name of new Set(layoutTabs(get(layout)).map((t) => t.layout as string))) {
		// A preview holds a COPY of the tree it was seeded from and answers to
		// nobody's save, not even one written under its name meanwhile
		// (da-xamd): it is neither replaced nor dropped here.
		if (isPreview(name)) continue;
		const next = await loadLayout(name).catch(() => null);
		if (!next) continue;
		const before = get(baselines)[name];
		baselines.update((b) => ({ ...b, [name]: baselineOf(next) }));
		const edited = (t: Tab) => !!before && isModified(extract(t), before);
		if (layoutTabs(get(layout)).filter((t) => t.layout === name).every(edited)) continue;
		await replaceCopies(name, next, edited);
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
	// Leaving the layout, whether to a tab that is already open or to a new
	// one: a maximize is not sticky (da-00am).
	unmaximize();
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

/**
 * `/<name>?from=<layout>` (da-xamd): `name` as a PREVIEW root tab, seeded
 * with `<layout>`'s tree — anything `open` answers to, saved, shipped or a
 * widget kind. It is drawn in italics and written nowhere, so a reload has
 * never heard of it and `keepTab` is what makes it real.
 *
 * AN EXISTING NAME WINS. Whatever `name` already means here — a tab in
 * front of you, a save, a built-in, a kind — opens exactly as it does
 * today and `from` is ignored, so a link can never seed over a layout
 * somebody has. A second link to a preview already open brings that one
 * forward rather than making a second copy.
 *
 * Nothing for a link that worked; the name that is NOTHING for one that
 * did not, which is what the page's notice says out loud.
 */
export async function openPreview(name: string, from: string): Promise<string | null> {
	await boot();
	const n = canonicalName(name);
	// Anything the name already MEANS wins — a tab in front of you (a
	// preview of it included, which is brought forward), a save, a kit's, a
	// built-in, a widget kind. `/day?from=day` lands here on `resolve`, so a
	// name that exists is never seeded over even when `from` names it too.
	if (rootTabs(get(layout)).some((t) => t.layout === n) || (await resolve(n))) {
		await open(n);
		return null;
	}
	const seed = await resolve(canonicalName(from));
	if (!seed) return from;
	// As `open` does: arriving at a layout is not arriving at a maximized pane.
	unmaximize();
	const tab = await tabHolding(n, seed, [n], new Set(allIds(get(layout))));
	// The name and the tab it belongs to land TOGETHER, with `settle` held
	// off between them: a preview is a name a tab carries, so a settle in
	// the middle would see one without the other and either prune the name
	// or write the tab into this device's open list.
	quiet(() => {
		previews.update((shown) => new Set(shown).add(n));
		baselines.update((b) => ({ ...b, [n]: baselineOf({ ...seed, name: n }) }));
		apply((l) => normalize({ ...l, root: { ...l.root, tabs: [...l.root.tabs, tab], active: l.root.tabs.length } }));
	});
	settle();
	return null;
}

/**
 * `Keep tab` (da-xamd): the preview becomes real — its tree saved as a
 * layout of the reader's own under the name it was opened with, and the
 * tab in this device's open list, where every later act finds it like any
 * other. Which is a SAVE, and `save` is where a saved name stops being a
 * preview: one act under two names, because what the reader is doing is
 * keeping the tab and what the shell is doing is writing the layout.
 */
export const keepTab = save;

export const activate = (index: number) => {
	// «coming back should unmaximize the pane» (vividn, da-00am): leaving the
	// layout drops the maximize, so the tab returned to is drawn whole.
	unmaximize();
	apply((l) => setActive(l, l.root.id, index));
};

/**
 * Close a root tab; the last one stays (tree.ts). Its draft, if any, is kept
 * for next time, and a PREVIEW closed writes nothing at all — `settle` gives
 * its name back, the tab no longer carrying one (da-xamd). A preview that
 * stayed, being the last tab, still carries its name and is still a preview.
 */
export const close = (index: number) => apply((l) => closeTab(l, l.root.id, index));

/**
 * Save this copy's tree as its layout's new version.
 *
 * The copy is then unmodified — and so is any other copy of the same
 * layout that happens to hold the same tree. One that does not now
 * differs from the save, and says so with its dot: the save is the
 * layout, not the tab it was made from.
 */
export async function save(tabId: NodeId): Promise<void> {
	const tab = copyOf(tabId);
	if (!tab) return;
	const name = tab.layout as string;
	const tree = extract(tab);
	await saveLayout(tree);
	baselines.update((b) => ({ ...b, [name]: baselineOf(tree) }));
	// A PREVIEW that has been saved is not a preview any more (da-xamd):
	// the name means something now, so the tab joins this device's open
	// list like any other. No-op for every other copy.
	forget(name);
}

/**
 * Back to the save: the last one, or `version` of it (a new version, as
 * `undo` makes one — the way forward again is `restore(tabId)` with none).
 * Every copy of the layout goes back with it; the name is what is being
 * restored.
 */
export async function restore(tabId: NodeId, version?: number): Promise<void> {
	const tab = copyOf(tabId);
	if (!tab) return;
	const name = tab.layout as string;
	// A PREVIEW has no save to go back to, and its BASELINE is the tree it
	// was seeded from — which is the only thing "back" can mean for one
	// (da-xamd), and what the dot beside the act is measured against. Its
	// baseline FIRST, not `resolve`: a layout saved under that name
	// somewhere else meanwhile is a different tree, and `followSaves` has
	// deliberately not moved this copy to it — restoring to it would be the
	// one write a preview must never make on the reader's behalf.
	//
	// `version` is a reader picking from the save's own trail, which a
	// preview has none of: `versions(name)` answers with nothing for one, so
	// the list under the bar says «not saved yet» and offers no button. The
	// arm below is therefore unreachable for a preview rather than silent.
	const seed = isPreview(name) ? (get(baselines)[name] ?? null) : await resolve(name);
	const saved = version === undefined && get(modified).has(tabId) ? seed : await restoreSaved(name, version);
	if (!saved) return;
	// A different tree arrives: whatever was filling the page is not it.
	unmaximize();
	baselines.update((b) => ({ ...b, [name]: baselineOf(saved) }));
	await replaceCopies(name, saved);
}

/**
 * This copy's tree under a new name, and this copy pointed at it; the
 * original keeps its save. A tab holding no layout — a split made by
 * splitting a tab (da-6ji5) — is saved the same way and becomes a copy of
 * the new layout, as a tab that holds one is: `Save as layout…`.
 */
export async function saveAs(tabId: NodeId, newName: string): Promise<void> {
	const tab = findTab(get(layout), tabId);
	if (!tab || !newName || newName === tab.layout) return;
	const name = tab.layout;
	const copy = await saveAsSaved(extract(tab), newName);
	// This copy first, then its baseline, then the old draft: `settle` runs
	// on each, and would write the edit back under the old name if the tab
	// still carried it when the new baseline landed.
	// A layout the person has just made opens unlocked (da-gshh), unless this
	// device has said otherwise of the name.
	const locked = !!loadLocked(newName);
	apply((l) => ({
		...l,
		root: mapTabs(l.root, (t) => (t.id === tabId ? { ...without(t, 'locked'), ...(locked ? { locked } : {}), layout: newName } : t))
	}));
	baselines.update((b) => ({ ...b, [newName]: baselineOf(copy) }));
	if (name !== undefined) clearDraft(name);
	await replaceCopies(newName, copy);
}

/**
 * The tree that SHIPPED under `name`: what a reset goes back to (da-luwe).
 *
 * Both arms now (da-gg0v). vividn, 18:15: "maybe if a layout was installed
 * through a kit, it could reset to the kit's definition" — so a kit's
 * layout is a shipped tree exactly as a built-in is, and takes the name
 * ahead of the built-in, being the more particular thing: it was installed
 * into THIS database. A name with no shipped tree — anything the reader
 * saved themselves, under a name nothing else claims — has no reset at all.
 */
export const shippedTree = (name: string): Layout | null =>
	pick(kitLayout(name), builtinLayout(name, phone()));

/**
 * Back to the shipped tree: this copy's DRAFT set to what `defaults.ts`
 * holds for its name, and NOTHING written (da-luwe).
 *
 * vividn, 16:29: "when reset is clicked it should restore the default
 * settings, but not actually save them to the db, so that the user can
 * restore to their other layout, then upon save it would sync." So the
 * baseline does not move, which is the whole difference from `restore`:
 * the copy now differs from its save and says so with its dot, `Restore
 * layout` beside it in the menu is the way back, and `Save layout` is what
 * writes the shipped tree over the save — and syncs it, as any save does.
 * With no save shadowing the built-in, the reset just discards this
 * device's draft and the dot clears.
 */
export async function resetLayout(tabId: NodeId): Promise<void> {
	const tab = copyOf(tabId);
	if (!tab) return;
	const shipped = shippedTree(tab.layout as string);
	if (!shipped) return;
	unmaximize();
	await replaceCopies(tab.layout as string, shipped);
}

/**
 * «Reset default layouts» (da-jbcv): every layout datum SHIPS back to the
 * tree it ships with, and the root bar back to the tabs a fresh install
 * shows — and nothing else touched.
 *
 * vividn, 18:16: "Reset default layouts, should reset all built in layouts,
 * as well as the tabs shown on the root. Other layouts are still
 * accessible to be added with the + though, and are not reset."
 *
 * A built-in goes back by having the save that SHADOWS it removed, which
 * is the only thing standing between the name and `defaults.ts`: after it,
 * `resolve` answers with the shipped tree again and the chooser lists the
 * name as `builtin` once more. So the names touched are exactly
 * `BUILTIN_NAMES` — a layout the reader saved under a name nothing ships,
 * and a layout a KIT installed (which lives in the data db, not here), are
 * neither read nor written, and stay in the `+`.
 *
 * This device's lock on each built-in is forgotten too, so each opens
 * locked again, as a first open does (da-gshh).
 *
 * This one WRITES, which is what separates it from `resetLayout`: a batch
 * that only moved the drafts would leave every save still shadowing its
 * built-in, and the next reload would undo the whole act. The confirm on
 * the gear's menu is what stands in for the undo (RootBar.svelte).
 */
export async function resetDefaults(): Promise<void> {
	unmaximize();
	// Which of the shipped names the reader has actually saved, asked once:
	// the rest have no entry to remove, and are the draft alone.
	const saved = new Set((await listLayouts()).filter((l) => l.source === 'user').map((l) => l.name));
	for (const name of BUILTIN_NAMES) {
		clearDraft(name);
		// Locked again, as a first open is (da-gshh).
		clearLocked(name);
		if (saved.has(name)) await deleteLayout(name);
	}
	// The baselines are a record of what the saves WERE; every one of them
	// is about to be read again from what the names now mean.
	baselines.set({});
	previews.set(new Set());
	await fillRoot(fresh());
}

/**
 * `Reset widget` (da-7ymx): the act that puts the kind's declared defaults
 * back on the widget `tab` holds, or undefined when there is nothing to
 * reset — a tab holding a LAYOUT or a SPLIT holds no widget of its own, and
 * a kind that declares no defaults has none to restore. The registry is the
 * only place the defaults are written down (`defaultsOfKind`); the menu
 * asks, and never keeps a copy.
 *
 * A DRAFT change, down the same `setWidget` path the widget's own `change`
 * takes (Layout.svelte): the layout holding the tab shows its dot, and Save
 * is what writes it.
 */
export function resetWidget(tab: Tab | undefined): (() => void) | undefined {
	if (!tab || tab.layout !== undefined || isSplit(tab.content)) return undefined;
	const widget = tab.content;
	const settings = defaultsOfKind(widget.kind);
	if (!settings) return undefined;
	return () => apply((l) => setWidget(l, tab.id, { ...widget, settings }));
}

/**
 * Lock or unlock a layout, on this device only — every copy of it, since
 * the lock is the name's (`draft.ts`) and a copy that ignored it would
 * unlock itself on the next reload.
 */
export function toggleLock(tabId: NodeId): void {
	const tab = copyOf(tabId);
	if (!tab) return;
	const name = tab.layout as string;
	const locked = !tab.locked;
	saveLocked(name, locked);
	apply((l) =>
		layoutTabs(l)
			.filter((t) => t.layout === name)
			.reduce((acc, t) => setLocked(acc, t.id, locked), l)
	);
}

/**
 * Fill a tab anywhere in the tree with layout `name`, inlined; the
 * chooser's other choice. The tab that lands is `inline`'s own — id and
 * all — so it is the same kind of thing as a root tab and its `extract`
 * reads back equal to the save.
 *
 * The tab is REPLACED, so a layout it HELD is not one of the names open
 * above it: the ancestors are what is above it, less its own. Without
 * that, filling a tab with the layout it already holds — or with the
 * built-in a chosen widget is (`widgetInto`) — would refuse itself.
 */
export async function inlineInto(tabId: NodeId, name: string): Promise<void> {
	const held = findTab(get(layout), tabId)?.layout;
	const above = ancestorsOf(tabId).filter((n) => n !== held);
	const taken = new Set(allIds(get(layout)));
	const filled = await expandTab({ id: tabId, content: notice(''), layout: name }, above, taken);
	apply((l) => normalize({ ...l, root: mapTabs(l.root, (t) => (t.id === tabId ? filled : t)) }));
}

/**
 * Fill a tab anywhere in the tree with a widget — the chooser's other
 * choice, and `inlineInto`'s opposite (da-8o07).
 *
 * A tab that HELD a layout sheds it (`shedLayout`): what the reader chose
 * is what the tab now IS, not a widget put inside a copy of that layout.
 * The save is untouched — the tab was a copy, and the copy is gone — and
 * with the last copy of a name goes its baseline, so no dot, no draft and
 * no act is left over for a copy that is not there.
 *
 * At the ROOT it is the built-in ONE-WIDGET LAYOUT the kind is instead
 * (da-y2f6): every built-in but `day` and `mobile` is exactly that widget alone, named
 * as its pane is titled, so the tab can be the widget AND have a name —
 * and it has to have one, because this device's open list is a list of
 * NAMES (`settle`, `saveRoot`) and a nameless root tab has nothing to
 * write there, so it would be gone after a reload. It is the same path
 * the root bar's `+` takes for a built-in name. A pickable kind with no
 * built-in still sheds, and then it is a root tab like any other nameless
 * one: it lives as long as the page does.
 */
export async function widgetInto(tabId: NodeId, kind: string): Promise<void> {
	const name = findTab(get(layout), tabId)?.layout;
	const builtin = canonicalName(kind);
	if (rootTabs(get(layout)).some((t) => t.id === tabId) && BUILTIN_NAMES.includes(builtin))
		return inlineInto(tabId, builtin);
	apply((l) => shedLayout(setWidget(l, tabId, newWidget(kind)), tabId));
	if (name !== undefined && !tabOf(name)) baselines.update(({ [name]: _gone, ...rest }) => rest);
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
