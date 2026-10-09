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
// root tabs made from one save (`main` and a `mine` saved from it) share
// every id, and the tree operations find a pane by id — a split on one
// tab's `tail` would split both. So `inline` re-mints an id that is
// already in the tree as `<id>@<name>`, and `extract` strips that suffix
// again, so the save keeps the ids it had and nothing has to remember a
// map. Only colliding ids are touched: the first `main` keeps `tail`,
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
// one guard: a name already open above it (`main` inside `main`) is a
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
// A PLACE (da-6du3o) is a preview of a name that already means something:
// on a phone, and from a desktop's modal by `Open as tab` (da-jozgv),
// `openPlace` puts a widget kind's built-in layout beside the tab you were
// on, and it closes itself when you leave it. `Keep tab` keeps it, and
// that is no save — the name is the built-in's already — only the tab
// joining this device's open list. «A modal asks; a place is a widget»
// (docs/web-conventions.md).
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
import { confirm } from '$lib/confirm.js';
import { currentDb } from '$lib/database.js';
import { initDb } from '$lib/db.js';
import type { Layout, NodeId, Pane, Split, Tab, Widget } from './types.js';
import { LAYOUT_VERSION, isSplit, isWidget } from './types.js';
import { apply, layout, loaded, replay } from './current.js';
import {
	ALIASES,
	BUILTIN_NAMES,
	DEBUG_DB,
	DEBUG_KIT,
	DEBUG_LAYOUT,
	DEBUG_LAYOUT_TREE,
	PHONE_WIDTH,
	builtinLayout,
	newDefaults
} from './defaults.js';
import {
	clearDraft,
	clearLocked,
	isModified,
	loadDefaults,
	loadDevice,
	loadDraft,
	loadLocked,
	loadRoot,
	loadShell,
	saveDefaults,
	saveDevice,
	saveDraft,
	saveLocked,
	saveRoot,
	saveShell,
	type RootState
} from './draft.js';
import {
	deleteLayout,
	installKitLayout,
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
import { KINDS, defaultsOfKind } from './registry.js';
import { clearSession, keyOf } from './session.js';

export { versions };

/** The widget kind a refused or missing layout is shown as. */
export const NOTICE_KIND = 'notice';

const BAR = { edge: 'top', align: 'start' } as const;

const phone = () =>
	typeof matchMedia === 'function' && matchMedia(`(max-width: ${PHONE_WIDTH}px)`).matches;

/**
 * The root bar a fresh install shows (da-u7vz8), `welcome` in front: what
 * datum is, the field widget to choose what to track in, and `collect`,
 * the page to log on — which is what a first visit opened alone before, as
 * `mobile` on a phone (da-shzs) and `main` elsewhere.
 */
const fresh = (): RootState => ({ names: ['welcome', 'track', 'collect'], active: 0 });

/**
 * This DEVICE, in the word a layout's `designedFor` uses: decided ONCE, by
 * the screen it first opens on, and kept (da-lsjs9). vividn, 2026-10-09:
 * «once per device i would say … I would definitely keep the layouts
 * consistent per device» — so a phone turned on its side and reloaded keeps
 * its trees, and a desktop window narrowed keeps its own. «Reset default
 * layouts» is a first visit again and decides again (`resetDefaults`). A
 * browser that refuses storage is asked its screen each time, as all were.
 */
export function device(): NonNullable<Layout['designedFor']> {
	const kept = loadDevice();
	if (kept === 'mobile' || kept === 'desktop') return kept;
	const now = phone() ? 'mobile' : 'desktop';
	saveDevice(now);
	return now;
}

/** Whether a shipped layout draws its phone tree here. */
const mobile = () => device() === 'mobile';

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
	const own = provisional ? null : await loadLayout(name).catch(() => null);
	const layout = pick(own, kitLayout(name), builtinLayout(name, mobile()));
	return { layout, shipped: !!layout && layout !== own };
}

/**
 * While the boot draws before the CLI has answered (da-bhudz), a name means
 * what this device can say without asking it: the kit's as last cached,
 * else the built-in. A save of the reader's own waits for the real pass.
 */
let provisional = false;

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
 * `input` was the first entry (da-22vg, `input` → `repl`) and the input
 * widget is back under that very id (da-3zon), so a layout saved with one
 * opens as one again — which is the whole point of a kind's id being a
 * saved-layout key. `spec` is the field widget's old half (da-fu29p); the
 * `field` setting a saved one carries is read by nobody now, and kept as
 * it is — `RETIRED_SETTINGS` drops a known value, not a key.
 *
 * Here rather than in `tree.ts` because this is where layouts are READ:
 * `inline` is the one door every save, draft and built-in comes through,
 * and `extract` writes the new name back out — so a layout migrates once,
 * against its own baseline, and stays migrated. `normalize` would run it
 * on every drag of a border instead, and would give the tree's shape
 * module an opinion about kinds, which it has never had.
 */
const RETIRED_KINDS: Record<string, string> = { spec: 'field' };

/**
 * A setting a saved layout may carry that no reader ever chose, dropped so
 * the widget goes back to deriving it (da-r0ydu).
 *
 * The day strip's height is the one entry. `defaults.ts` used to write
 * `dayHeight: DEFAULT_DAY_HEIGHT` into every shipped tree, so every layout
 * saved before the day began following its widget carries that 85 — and a
 * number in the settings is what HOLDS the day against its box, for good,
 * at every screen size. That is exactly the state registry.ts says must
 * not exist: «a default here would be a number nobody chose standing in
 * the way of that». Left alone, the reader who saved a layout last week
 * would be the one reader the feature never reaches.
 *
 * ⚑ It drops the OLD DEFAULT and nothing else. A day height the reader
 * actually dragged to is some other number and is kept; 85 is
 * indistinguishable from a deliberate 85, and that is the trade — it was
 * the shipped default in every tree, so it is un-chosen far more often
 * than chosen, and one drag of the `+` handle puts it back.
 *
 * ⚑ SO A CHOSEN 85 CANNOT BE SAVED AS HELD. A reader whose box derives
 * some other height may drag the day onto 85 and «Save options with
 * layout»; the number lives in the tree for this session, and the save —
 * which goes through `extract`, dropping it the same way — does not carry
 * it, so a reload has the day following again. That is the same trade
 * read from the other end, and it is why `extract` drops it too: the
 * alternative is a tab that lights its modified dot the instant the save
 * lands, over a key no reload would keep.
 *
 * ⚑ THE NUMBER IS A LITERAL, NOT `DEFAULT_DAY_HEIGHT`. A migration key is
 * a fact about HISTORY — «what trees written before da-r0ydu carry» — and
 * that constant has stopped meaning it: it is now only the strip's
 * stand-in before its box is measured (`derived` in DayStrip.svelte). The
 * two coincide at 85 today and are free to part tomorrow, and the day they
 * part this arm would quietly stop matching, every layout saved before
 * da-r0ydu would go back to holding a height nobody chose, and no test
 * would say so — the tests pin the same history, as they should.
 */
const RETIRED_SETTINGS: Record<string, Record<string, unknown>> = {
	// 85 was the ONLY value that constant ever had: `DEFAULT_DAY_HEIGHT`
	// arrived at 85 with the strip's two handles (da-07ij, 2026-09-09) and
	// was never moved — `git log -S"DEFAULT_DAY_HEIGHT =" -- web/src/lib/`
	// finds that one introduction and nothing else. So there is no older
	// tree carrying a different default for this arm to miss.
	daystrip: { dayHeight: 85 }
};

/** A tab whose widget is a retired kind, as the kind it became — and without a retired setting. */
const migrated = (tab: Tab): Tab => {
	if (isSplit(tab.content)) return tab;
	const kind = RETIRED_KINDS[tab.content.kind];
	const content = kind === undefined ? tab.content : { ...tab.content, kind };
	const retired = RETIRED_SETTINGS[content.kind];
	if (!retired) return kind === undefined ? tab : { ...tab, content };
	const drop = Object.keys(retired).filter((key) => content.settings?.[key] === retired[key]);
	if (drop.length === 0) return kind === undefined ? tab : { ...tab, content };
	const settings = { ...content.settings };
	for (const key of drop) delete settings[key];
	return { ...tab, content: { ...content, settings } };
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
		// BOTH WAYS. `inline` drops a retired setting on the way in; this
		// drops one that arrived AFTER — «Save options with layout» writes
		// whatever the session holds into the tree, and the reader is free
		// to drag the day height onto the retired literal. Without this the
		// two sides of `modified` would disagree the instant a save landed
		// (the tab's own `extract` keeping the key, its fresh `baselineOf`
		// dropping it) and the tab would light its dot and offer «Restore
		// layout» for a difference that survives no reload. A retired kind
		// needs no arm here: `inline` has already rewritten it and nothing
		// can put one back.
		const bare = migrated(without(t, 'locked'));
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

/** The previews that are PLACES (`openPlace`): closed when left, kept without a save. */
const places = new Set<string>();

const forget = (name: string) => {
	places.delete(name);
	if (pushedFor === name) pushedFor = null;
	previews.update((open) => {
		const rest = new Set(open);
		rest.delete(name);
		return rest;
	});
};

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
		if (tabs.length === 0) tabs.push((await openTab('collect', taken)) as Tab);
		const built = normalize({
			version: LAYOUT_VERSION,
			name: 'root',
			root: { id: 'shell', tabs, active: Math.min(state.active, tabs.length - 1), bar: BAR }
		});
		const tree = provisional ? built : replay(built);
		// The tree already drawn from last time is left mounted when it is
		// this one (da-bhudz): its panes are running their commands.
		if (JSON.stringify(tree) !== JSON.stringify(get(layout))) layout.set(tree);
	} finally {
		booted = resting;
	}
	settle();
}

/**
 * The DEBUG database carries the debug layout (da-o2wm8).
 *
 * vividn asked for "A db named DEBUG, which has a layout preloaded into it
 * this DEBUG widget", and a preloaded layout is a KIT LAYOUT — a row in
 * the DATA database (store.ts's note, da-gg0v), so it is that database's
 * by construction and reaches every device of it, because the data syncs.
 * There is no kit file to install, so the app writes the row itself the
 * first time it opens a database of that name without one.
 *
 * WHO CREATES THE DATABASE is the reader: the logo menu's `new database`
 * already does it, and `?db=DEBUG` on its own is enough. Nothing new.
 *
 * A failure is not fatal and is not retried in a loop: the write is
 * attempted once per load, and until it lands the name simply means
 * nothing — which is what it means in every other database anyway.
 */
async function shipDebugLayout(): Promise<void> {
	if (currentDb() !== DEBUG_DB) return;
	// A tree shipped by an older app (the knobs alone, before da-6ecm5) is
	// brought up to this one: the kit row is the app's, never the reader's
	// — a save of the name goes to `_internal` and shadows it.
	const shipped = kitLayout(DEBUG_LAYOUT);
	if (shipped && !isModified(DEBUG_LAYOUT_TREE, shipped)) return;
	try {
		await installKitLayout(DEBUG_LAYOUT_TREE, DEBUG_KIT, !!shipped);
	} catch {
		// The slug is taken by a row that is not a kit's, or the write
		// failed. Either way there is nothing here to put right.
		return;
	}
	await refreshKitLayouts();
}

/**
 * In the DEBUG database, the debug layout is open and in front.
 *
 * "It only gets loaded for the DEBUG db" — so it is added HERE, at the
 * boot, rather than written into this device's open list: the list is one
 * per device and not per database (layouts are the device's, da-vk1x), and
 * a `debug` written into it would follow him back to his own database. It
 * cannot: the name means nothing there, so `openTab` passes over it and
 * `settle` writes the list back without it.
 */
const withDebug = (state: RootState): RootState =>
	currentDb() === DEBUG_DB && !state.names.includes(DEBUG_LAYOUT)
		? { names: [DEBUG_LAYOUT, ...state.names], active: 0 }
		: state;

/** Fill the root from this device's open layouts (`fresh`'s at first). Once. */
export function boot(): Promise<void> {
	return (booting ??= (async () => {
		const stored = loadRoot();
		// Drawn before the CLI is asked anything (da-bhudz, vividn: «none of
		// the tabs are even there in header bar»): the tree as this device
		// last drew it, else what it can build without the database. The
		// real pass below replaces it only where it differs.
		// A copy written by another version of the tree is not drawn: the
		// provisional tree is, and the real pass writes a new copy.
		const last = loadShell(currentDb());
		if (last?.version === LAYOUT_VERSION) layout.set(last);
		else {
			provisional = true;
			await fillRoot(withDebug(stored ?? fresh())).finally(() => (provisional = false));
		}
		loaded.set(true);
		await initDb().catch(() => {});
		// Before the first `resolve`: a kit's layout is one of the trees a
		// name may mean, and `shippedTree` reads the cache synchronously.
		await refreshKitLayouts();
		await shipDebugLayout();
		await fillRoot(withDebug(stored ?? fresh()));
		booted = true;
		// A fresh device's tabs are written now, and so is a change made to
		// the tree drawn early (da-bhudz), rather than at the next change
		// after it. The list is the same on every device (da-u7vz8): what
		// follows the device is `collect`'s tree (`device`, da-lsjs9).
		settle();
		addEventListener('popstate', onPopState);
		onExternalChange(followSaves, '_internal');
		// A kit installed in another tab — into the data db — changes what
		// the names mean here too. This page's data db (da-vk1x): under
		// `?db=other` the kit layouts are `other`'s.
		onExternalChange(refreshKitLayouts, currentDb());
		// Every tab takes part; one per database leads it (da-6kpj5).
		void import('$lib/sync.js').then(({ startSync }) => startSync(currentDb()));
		void offerDefaults(stored === null);
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
	for (const name of places) if (!carried.has(name)) places.delete(name);
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
	//   [main, x*]        active 1 → none in front is kept, one before → 0 (main)
	//   [main, mine, x*]  active 2 → two before → 1 (mine)
	//   [x*, main]        active 0 → none before → 0 (main, the only name left)
	const front = names.indexOf(tabs[$layout.root.active]);
	const before = tabs.slice(0, $layout.root.active).filter(kept).length;
	const active = front >= 0 ? front : Math.max(0, before - 1);
	saveRoot({ names: names.map((t) => t.layout as string), active });
	// What the next load draws before the CLI answers: the tree as written.
	saveShell(currentDb(), { ...$layout, root: { ...$layout.root, tabs: names, active } });
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
	// built-in, a widget kind. `/main?from=main` lands here on `resolve`, so a
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
export async function keepTab(tabId: NodeId): Promise<void> {
	const name = copyOf(tabId)?.layout;
	// A place's name means its built-in already: keeping it is joining the open list.
	if (name !== undefined && places.has(name)) forget(name);
	else await save(tabId);
}

/**
 * Bring root tab `index` to the front. From a PLACE (da-6du3o) it is not
 * immediate: the root frame slides there, as a swipe back does, and the
 * place closes when the slide lands.
 */
export const activate = (index: number) => {
	// «coming back should unmaximize the pane» (vividn, da-00am): leaving the
	// layout drops the maximize, so the tab returned to is drawn whole.
	unmaximize();
	// Leaving a place slides, as a swipe back does.
	if (inPlace()) sliding.set(get(layout).root.tabs[index]?.id ?? null);
	else apply((l) => setActive(l, l.root.id, index));
};

// ---- places (da-6du3o) ------------------------------------------------

/**
 * The root tab, BY ID, the root frame is asked to SLIDE to
 * (PaneFrame.svelte): in from the right when a place opens, back to the
 * left when it is left. Under `prefers-reduced-motion` the frame
 * crossfades instead. An id and not an index, because a request made
 * while another slide is landing is queued behind it, and that slide's
 * close moves every index right of the place.
 */
export const sliding = writable<NodeId | null>(null);

/** Whether the root tab in front is an open place. */
const inPlace = (): boolean => {
	const l = get(layout);
	const name = l.root.tabs[l.root.active]?.layout;
	return name !== undefined && places.has(name);
};

/**
 * The place whose history entry is the one the browser is on, if any: the
 * entry `openPlace` pushed. Named, not a flag, so that leaving one place for
 * another — or keeping it — never pops an entry that is not its own.
 */
let pushedFor: string | null = null;

/**
 * The place open as a MODAL (da-jozgv), or null. vividn, 2026-09-29: «I think
 * this is good on mobile, but the experience on desktop is not great … I
 * would still have it be a modal on desktop». So on a desktop `openPlace`
 * sets this and `PlaceModal.svelte` draws it over the page; on a phone it
 * stays a tab. It carries the place's NAME, which is what `pushedFor` and
 * the tab path mean by it, and the one widget that name resolves to — a
 * place need not be named after its kind (`dayview` is a `daystrip`).
 */
export const modalPlace = writable<{ name: string; widget: Widget } | null>(null);

/**
 * Open the place `kind` is: its root tab brought to the front if one holds
 * it; else, on a desktop, the widget in a modal (`modalPlace`); else, or
 * with `asTab`, a TRANSIENT tab — its built-in layout, italic, written
 * nowhere — put immediately right of the tab you are on, and slid onto.
 * Leaving it closes it (below); `Keep tab` keeps it. The browser's back is
 * the way back from either, so each pushes a history entry — the modal's
 * handed on to the tab its `Open as tab` opens.
 */
export async function openPlace(kind: string, asTab = false): Promise<void> {
	await boot();
	unmaximize();
	const name = canonicalName(kind);
	const tabs = rootTabs(get(layout));
	const at = tabs.findIndex((t) => t.layout === name || (isWidget(t.content) && t.content.kind === kind));
	if (at >= 0) {
		closeModalPlace();
		return activate(at);
	}
	// The widget the tab would hold, resolved as the tab resolves it; a place
	// that is more than one widget is a tab on a desktop too.
	const widget = asTab || phone() ? null : soleWidget(loadDraft(name) ?? (await meaning(name)).layout);
	const { pushState, replaceState } = await import('$app/navigation');
	// A modal open already has the entry; what opens next takes it over.
	const handed = get(modalPlace)?.name === pushedFor && pushedFor !== null;
	const enter = (place: string) => (handed ? replaceState : pushState)('', { place });
	modalPlace.set(null);
	if (widget) {
		enter(name);
		pushedFor = name;
		modalPlace.set({ name, widget });
		return;
	}
	const tab = await openTab(name, new Set(allIds(get(layout))));
	if (!tab) return;
	const from = get(layout).root.active;
	// The name and its tab land together, as `openPreview`'s do.
	quiet(() => {
		places.add(name);
		previews.update((shown) => new Set(shown).add(name));
		apply((l) => {
			const tabs = [...l.root.tabs.slice(0, from + 1), tab, ...l.root.tabs.slice(from + 1)];
			return normalize({ ...l, root: { ...l.root, tabs } });
		});
	});
	settle();
	// The entry is this place's BEFORE the tab switches: under reduced motion
	// the switch is at once, and the closer of the place it leaves must not
	// take this entry for its own (vremya on #349).
	enter(name);
	pushedFor = name;
	sliding.set(tab.id);
}

/**
 * The modal place closed — ×, Escape, the backdrop. Its history entry goes
 * with it, and the pop that follows is what clears `modalPlace`, so that
 * pop is never read as a back out of a place tab underneath.
 */
export function closeModalPlace(): void {
	const open = get(modalPlace);
	if (open === null) return;
	if (pushedFor !== open.name) return modalPlace.set(null);
	pushedFor = null;
	history.back();
}

/**
 * The one widget a layout is, if it is one and this build draws its kind —
 * `migrated` as `inline` migrates the tab's, so a save carrying a retired
 * kind or setting draws in the modal as it would in the tab.
 */
function soleWidget(l: Layout | null): Widget | null {
	const tabs = l?.root.tabs ?? [];
	const content = tabs.length === 1 ? migrated(tabs[0]).content : null;
	return content && isWidget(content) && KINDS[content.kind] ? content : null;
}

// A place closes when it is left, however: a swipe, a tap on another tab,
// the browser's back. The history entry it pushed goes with it.
let front: NodeId | undefined;
layout.subscribe(($layout) => {
	const was = front;
	front = $layout.root.tabs[$layout.root.active]?.id;
	if (was === undefined || was === front) return;
	const left = $layout.root.tabs.findIndex((t) => t.id === was);
	const name = $layout.root.tabs[left]?.layout;
	if (name === undefined || !places.has(name)) return;
	apply((l) => closeTab(l, l.root.id, left));
	if (pushedFor === name) {
		pushedFor = null;
		history.back();
	}
});

/**
 * Leave the place in front (da-hbrfg: the field widget's Done and Cancel):
 * the modal, closed as its × closes it, or else a place tab, for the tab it
 * opened beside, as a tap on that tab would. Anywhere else, nothing.
 */
export function leavePlace(): void {
	if (get(modalPlace) !== null) closeModalPlace();
	else if (inPlace()) activate(Math.max(0, get(layout).root.active - 1));
}

/** The browser's back, from a place: back to the tab it opened beside. */
function onPopState(): void {
	pushedFor = null;
	if (get(modalPlace) !== null) return modalPlace.set(null);
	const { tabs, active } = get(layout).root;
	if (inPlace()) sliding.set(tabs[Math.max(0, active - 1)].id);
}

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
 * the new layout, as a tab that holds one is: `Save as layout`.
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
	pick(kitLayout(name), builtinLayout(name, mobile()));

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
 * locked again, as a first open does (da-gshh) — and so is which device it
 * said it was, so the trees it goes back to are the ones a first visit on
 * the screen as it is NOW would draw (`device`, da-lsjs9).
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
	// A first visit decides which device this is, so this does again (da-lsjs9).
	saveDevice(null);
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
 * The one prompt that offers a reset by itself (da-cr0w2). vividn: «Do we
 * have a way to trigger a prompt to reset to default views? … That way
 * users can decide».
 *
 * A device whose open list was written under an earlier default is asked
 * ONCE per new one, with what changed, and either answer — Escape is «Keep
 * mine» — is remembered, so that default is never offered here again. A
 * `fresh` device starts on the newest and is only marked as having it.
 */
async function offerDefaults(fresh: boolean): Promise<void> {
	const news = newDefaults();
	const seen = fresh ? news.length : loadDefaults();
	if (fresh) saveDefaults(seen);
	if (seen >= news.length) return;
	const yes = await confirm(
		`datum has a new default layout. ${news.slice(seen).join(' ')}\n\n` +
			'Using it puts every layout that ships with datum back the way it comes, and the tabs at the top of the page back to the ones a first visit opens. ' +
			'Layouts you saved yourself, and layouts a kit installed, are not touched.\n\n' +
			'Keep yours and this is not asked again; «Reset default layouts» in the ⚙ menu does the same at any time.',
		'Use the new layout',
		'',
		'Keep mine'
	);
	// Remembered once it is done: a reset that failed is offered again.
	if (yes) await resetDefaults();
	saveDefaults(news.length);
}

/**
 * `Reset widget` (da-7ymx): the act that puts the kind's declared defaults
 * back on the widget `tab` holds, or undefined when there is nothing to
 * reset — a tab holding a SPLIT holds no widget of its own, and a kind that
 * declares no defaults has none to restore. The registry is the only place
 * the defaults are written down (`defaultsOfKind`); the menu asks, and never
 * keeps a copy.
 *
 * SHAPE, not name (da-ze1x): a tab that HOLDS a layout of exactly one
 * widget holds that widget too, because `inline` lifts a one-tab root pane
 * into the tab — so `/repl` and `/nowview` answer here, and a layout of
 * several widgets, which arrives under a split, does not.
 *
 * A DRAFT change, down the same `setWidget` path the widget's own `change`
 * takes (Layout.svelte): the layout holding the tab shows its dot, and Save
 * is what writes it — and for a one-widget layout that is the layout the
 * tab names, so its dot lights and `Restore layout` is the way back.
 *
 * Whatever this SESSION set on the tab goes with it (da-xhio, da-vhwq):
 * «defaults» means the widget as it starts, and a session setting left
 * standing over the settings just written would be exactly the thing it
 * did not go back to. It is also the one act that writes the tree from the
 * widget's side — an explicit one, and already out of a locked menu.
 */
export function resetWidget(tab: Tab | undefined): (() => void) | undefined {
	if (!tab || isSplit(tab.content)) return undefined;
	const widget = tab.content;
	const settings = defaultsOfKind(widget.kind);
	if (!settings) return undefined;
	return () => {
		clearSession(keyOf(get(layout), tab.id));
		apply((l) => setWidget(l, tab.id, { ...widget, settings }));
	};
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
 * (da-y2f6): every kind has a built-in that is exactly that widget alone, named
 * as its pane is titled — the others are `main`, `mobile` and `collect`, which are
 * pages, and `track`, the field widget's a second time under its tab's name
 * (da-u7vz8) — so the tab can be the widget AND have a name —
 * and it has to have one, because this device's open list is a list of
 * NAMES (`settle`, `saveRoot`) and a nameless root tab has nothing to
 * write there, so it would be gone after a reload. It is the same path
 * the root bar's `+` takes for a built-in name. A KIT's layout of that
 * name serves the same turn (da-o2wm8): a kind offered in one database
 * alone has no built-in — the built-ins are every database's — and its
 * layout is the one its own database carries. A pickable kind with
 * neither still sheds, and then it is a root tab like any other nameless
 * one: it lives as long as the page does.
 */
export async function widgetInto(tabId: NodeId, kind: string): Promise<void> {
	const name = findTab(get(layout), tabId)?.layout;
	const builtin = canonicalName(kind);
	if (
		rootTabs(get(layout)).some((t) => t.id === tabId) &&
		(BUILTIN_NAMES.includes(builtin) || !!kitLayout(builtin))
	)
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
