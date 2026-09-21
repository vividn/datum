<!--
	The frame of one pane: its bar, its tabs, the menu its TITLE opens, and
	the swipe.

	A PANE is the holder (vividn's word); what it holds is in the slot. With
	one tab there is no bar at all: the title sits ON the frame's outline,
	small and quiet, straddling the line the way a legend does, and the
	`+` is a badge right after it, on the same line (vividn, 17:56 — "just
	overlay the title on the frame, with the nice ... button visible next
	to it", da-jgrn). With more tabs the tab strip sits there instead and
	the badges follow the LAST tab. The row reads maximize, title or tabs,
	`+`, `▾` (da-bzhq): the maximize BEFORE the names, and after them the
	pair that adds a tab and manages the pane.

	The line is the edge the pane's `bar`
	names — top or bottom running across, left or right running DOWN with
	the words rotated (`writing-mode: vertical-rl`), and on the left turned
	round so it reads bottom-to-top like a book's spine (vividn, 15:39) —
	with the title or the tabs gathered along it by `bar.align`. The title,
	the tabs and the badge are centred ON the outline, half out and half
	in, and the content is padded by nothing: the row lies over the top
	strip of what is underneath (vividn's third look, da-13bc — "just overlay
	it directly on the frame […] it can overlap slightly with the underlying
	display"). The row is as tall as its words and outlined like the frame
	(vividn's fourth look, da-om9v — "the widget titles bleed too much into
	other frames"): `ROW`, 16px, so the 8px that hang out fit the 8px gap
	and nothing lies over the neighbour. The badge is the row's height too,
	except under a coarse pointer, where a finger needs 24px and 4px of it
	still reach over. The bar itself takes no pointer, so the border
	between two panes is still grabbed right beside it.

	A widget with title controls of its OWN — the custom pane's ✎ and the
	editor it opens, which is the only place its command can be written —
	draws them here: `Pane.svelte` moves them into the h2. With tabs there
	is no h2, the strip carrying the names, so the bar keeps a box of the
	same shape — `.own`, a heading of nothing, so not a heading — for the
	ACTIVE widget's controls right after the strip (da-ndjo).
	A widget with no controls of its own hosts nothing and is named by the
	frame, which is what keeps a renamed tab renamed and its modified dot
	drawn.

	A pane whose tab holds a split rather than a widget is a HOLDER: no
	border, no background, and — with one tab — no bar at all: a box that
	is not there, since the children's own titles can split them (vividn's
	second look, da-2a5y: "they have their own ... button floating there,
	but aren't really necessary"). With two or more tabs it keeps its tab
	strip, as a row of its own, and the `+` after the last tab. It is
	`section.holder` rather than `section.pane` so that "the panes" — as
	the test harness and the rest of the app find them — remain exactly
	the ones with a thing in them.

	A tab carrying a LAYOUT is such a thing, whatever shape its content
	has (da-9puz): the layout is the boundary vividn drew, so it draws the
	frame and wears the layout's name (`labelOf`, registry.ts — the rule
	the root bar's tabs read too), where a holder made by splitting is
	scaffolding with no name of its own. Without that, choosing a layout
	of several panes for a tab spilled them loose into its space,
	indistinguishable from the panes around them. The panes inside keep
	their own frames, so the layout's is a second, outer line — which is
	what says where it begins and ends. The ROOT pane is the exception at
	the other end: it draws no bar of its own at all, however many tabs
	it has, and no box, because the shell draws the root's tabs as the
	page's top bar (da-0a5o). And a LOCKED frame
	draws no `+` and no `▾` — nothing under a lock can be split, tabbed or
	resized, and those two are how it would be (vividn, da-bzhq: "Locking
	hides both the + and this down arrow icon menu") — and the tabs still
	switch. What the title still opens is the tab's own acts, which is
	where `Unlock layout` is.

	A layout tab's frame is also where that layout's identity is worked
	(da-crp5): the acts the root bar draws for a root tab — save, restore,
	save as, lock, and `Reset layout` where the layout is one that SHIPPED
	with datum (da-luwe) — are in this menu when the tab holds a layout,
	whatever its depth, and the orange modified dot follows the title or
	the tab. The one exception to the lock above: a layout tab that is
	itself locked keeps a menu holding those acts alone, so `Unlock
	layout` can be reached.

	Swiping across the content moves between tabs along the bar's axis:
	left/right under a horizontal bar, up/down under a vertical one. Touch
	only, and with the motion under the finger (da-kcmp). Whose the gesture
	is, is settled at the first move that commits an axis, by DIRECTION and
	not by what merely exists under the finger:

	  - a scroller between the finger and this frame's content box — or an
	    element declaring the axis its own with `data-pans` (the nowview's
	    band) — keeps the gesture while it can still go that way. At its
	    end it yields, which is vividn's rule: "when they reach the end then
	    allow swiping". Read once, at the start: a scroll that runs out
	    under the finger stays a scroll, and the NEXT swipe moves the tab.
	  - else the innermost frame with a tab in the finger's direction takes
	    it and stops the event, so no frame above it moves too. A frame on
	    its LAST tab lets it through — which is how a swipe over a tabbed
	    pane reaches the root and moves the whole layout, the root's tabs
	    being the page's layouts (da-0a5o).
	  - a touch on the frame itself — the nested inset, the root's page
	    margin — has nothing under it and is always the frame's: "always
	    allow swiping when swiping in the margins or along the frame".

	Then the content follows the finger, and the release slides it the rest
	of the way off and brings the next tab in from the far side — two
	phases through the one box. Every tab is mounted and drawn already
	(da-rjmg, `TabBody.svelte`), so what moves is a picture, not a mount.
-->
<script lang="ts" context="module">
	/**
	 * The bar's row, in pixels: the title's words (0.75rem at a line of
	 * 14px) and its 1px outline. The frame's `--row` and Layout.svelte's
	 * chrome both read it, so the floors follow the row drawn. Half of it
	 * hangs outside the outline, and must fit the gap between panes
	 * (Layout.svelte's `BORDER`).
	 */
	export const ROW = 16;
</script>

<script lang="ts">
	import { setContext, tick } from 'svelte';
	import { ask } from '$lib/confirm.js';
	import { writable } from 'svelte/store';
	import type { BarAlign, BarEdge, Pane, Tab } from './types.js';
	import { isSplit } from './types.js';
	import { apply, layout } from './current.js';
	import { choose, fillTab, put } from './chooser.js';
	import { ancestorsOf, modified, resetLayout, resetWidget, restore, save, saveAs, shippedTree, toggleLock } from './shell.js';
	import { addTab, breakOut, canBreakOut, closeTab, newTab, setActive, setBar, setTitle, splitPane, splitTab } from './tree.js';
	import { inForce, toggleMaximize } from './maximize.js';
	import { labelOf, titleOf } from './registry.js';
	import { layoutActs, managed, menuItems, named, optionsItem } from './menu.js';
	import { widgetOptions } from './options.js';
	import PaneMenu from './PaneMenu.svelte';

	export let pane: Pane;
	/** False on the last pane: the close item is greyed, not hidden. */
	export let canClose = true;
	/** The layout's root: the one pane whose content box is the scroller. */
	export let root = false;
	/**
	 * Whether a tab ABOVE this pane is locked. The active tab's own lock is
	 * added to it below: under either there is no `+` and none of the pane
	 * items, and the tab strip still switches — but a layout tab that is
	 * itself locked keeps the menu holding its `Unlock layout` (da-crp5),
	 * or a lock would be a thing nobody could undo.
	 */
	export let locked = false;
	/**
	 * The active widget's kind. Its title goes on a wrapper round the frame
	 * as a class — `.repl`, `.tail`, `.dayview` — which is how the old
	 * grid's areas were named and how the rest of the app and its tests
	 * still find a pane's insides.
	 */
	export let kind: string | undefined = undefined;

	const EDGES: BarEdge[] = ['top', 'bottom', 'left', 'right'];
	const ALIGNS: BarAlign[] = ['start', 'center', 'end'];
	/**
	 * Where `start`, `center` and `end` put the title along each edge, as
	 * the eye sees it: the left-hand bar is turned round, so its start is
	 * at the bottom.
	 */
	const SEEN: Record<BarEdge, string[]> = {
		top: ['left', 'center', 'right'],
		bottom: ['left', 'center', 'right'],
		left: ['bottom', 'middle', 'top'],
		right: ['top', 'middle', 'bottom']
	};
	const AT: Record<string, number> = { left: 3, top: 3, center: 5.5, middle: 5.5, right: 8, bottom: 8 };
	/** The Bar submenu's icon: a thick bar `long` px on `edge` of a 16px square, `at` px along it. */
	function mark(edge: BarEdge, at = 2, long = 12): number[] {
		const across = edge === 'top' || edge === 'left' ? 2 : 11;
		return edge === 'top' || edge === 'bottom' ? [at, across, long, 3] : [across, at, 3, long];
	}
	/** A swipe shorter than this is a touch, not a swipe. */
	const SWIPE_PX = 40;
	/**
	 * How long the content takes to slide off, and the next tab's to
	 * arrive. The transition reads it as `--slide`, so the wait after the
	 * transform and the transition itself are the one number.
	 */
	const SLIDE_MS = 150;

	// What `Pane.svelte` looks for: it renders body-only inside a frame and
	// reports its alarm here, so the orange reaches whichever pane holds
	// the REPL.
	const alarm = writable(false);
	/**
	 * Where a widget that draws its own title puts it (the custom pane's
	 * ✎): the h2 with one tab, and the box after the strip with more
	 * (da-ndjo). Null while there is nothing hosting, which is what keeps
	 * those controls out of the content.
	 */
	const titleHost = writable<HTMLElement | null>(null);
	const hosted = writable(false);
	setContext('layout-frame', { alarm, titleHost, hosted });
	let h2: HTMLElement | null = null;
	$: titleHost.set(h2);

	$: active = pane.tabs[pane.active];
	/**
	 * Scaffolding, not a thing: a split someone made by splitting. A tab
	 * carrying a LAYOUT is not one however its content is shaped (da-9puz)
	 * — the layout is the boundary, so it draws the frame and wears the
	 * name, instead of spilling its panes loose among the panes around it.
	 * The page root is the exception at the other end: the shell draws its
	 * identity (its tabs, its name, its acts), so the root is never a box.
	 */
	$: holder = isSplit(active.content) && (root || active.layout === undefined);
	/** A frame round a LAYOUT's panes: they are inset from it, so the two outlines read as two. */
	$: nested = isSplit(active.content) && !holder;
	$: many = pane.tabs.length > 1;
	/** What this frame's menu calls the thing it acts on: with one tab, the tab is the pane. */
	$: thing = many ? 'tab' : 'pane';
	$: vertical = pane.bar.edge === 'left' || pane.bar.edge === 'right';
	/**
	 * `labelOf` (registry.ts), the one rule — except at the root, which
	 * wears no name of its own: its tab always carries a layout, and the
	 * shell already wears that name on the root bar (da-0a5o). So the root
	 * is called what it SHOWS, which is what `data-title` means everywhere
	 * the shell is not the one naming the pane.
	 */
	$: title = labelOf(root ? { ...active, layout: undefined } : active);
	/** No bar at all: the root's is the shell's, a lone holder's is its children's. */
	$: bare = root || (!many && holder);
	/** Whether this pane has a group to leave: everything but the root's own pane. */
	$: canBreak = canBreakOut($layout, pane.id);

	/** This pane's own lock: one above it, or its active tab's own. */
	$: lockedHere = locked || !!active.locked;

	/**
	 * Whether this pane is the one filling the root (da-00am) — which is
	 * what turns the badge round into its restore state. The store alone is
	 * not the answer: a maximize belongs to the root tab it was made in, and
	 * stops applying when another is in front (`isMaximized`).
	 */
	$: big = $inForce?.pane === pane.id;

	/**
	 * A failed act: reported where a developer sees it. The root bar has a
	 * line for the active layout's errors; a pane's bar is one row of words
	 * with no room for one, and the act is the reader's own press, so it
	 * must not throw into the page either.
	 */
	const attempt = (job: Promise<unknown>) => void job.catch((e) => console.error(e));

	/**
	 * The acts on the layout this tab HOLDS (da-crp5) — the same ones the
	 * root bar offers, since a nested copy is no different in kind: `Reset
	 * layout` among them where the name has a shipped tree (da-luwe), which
	 * is a fact about the LAYOUT and so is the same in either menu. Not
	 * offered under a lock from above; the tab's OWN lock leaves them, so
	 * that `Unlock layout` can be reached.
	 */
	$: acts =
		active.layout === undefined || locked
			? undefined
			: {
					dirty: $modified.has(active.id),
					locked: !!active.locked,
					save: () => attempt(save(active.id)),
					restore: () => attempt(restore(active.id)),
					reset: shippedTree(active.layout) ? () => attempt(resetLayout(active.id)) : undefined,
					saveAs: () => {
						const to = prompt('Save this layout as:', '')?.trim();
						if (to) attempt(saveAs(active.id, to));
					},
					toggleLock: () => toggleLock(active.id)
				};

	/** The pane's `Bar ▸` (da-nk4k): four placements, then three alignments. */
	$: bar = {
		label: 'Bar',
		sub: [
			EDGES.map((edge) => ({
				label: edge,
				title: `bar on the ${edge}`,
				icon: mark(edge),
				checked: pane.bar.edge === edge,
				run: () => apply((l) => setBar(l, pane.id, { ...pane.bar, edge }))
			})),
			ALIGNS.map((align, i) => {
				const where = SEEN[pane.bar.edge][i];
				return {
					label: align,
					title: `align to the ${align} (${where})`,
					icon: mark(pane.bar.edge, AT[where], 5),
					checked: pane.bar.align === align,
					run: () => apply((l) => setBar(l, pane.id, { ...pane.bar, align }))
				};
			})
		]
	};

	/**
	 * The menu, from the one builder the root bar uses too (`menu.ts`). A
	 * locked pane keeps only the acts, so `Unlock layout` is reachable.
	 */
	$: items = lockedHere
		? [...optionsItem(widgetOptions(active)), ...(acts ? layoutActs(acts) : [])]
		: menuItems({
				choose: () => void fillTab(active.id),
				// Beside Change Widget, and about the same thing: the widget in front.
				reset: resetWidget(active),
				options: widgetOptions(active),
				// A PANE IS RENAMEABLE TOO (da-s7sdm), not just a tab of a
				// strip: with one tab the tab IS the pane, and the name on the
				// frame is that tab's. Named after whichever it is, as `close`
				// is. A tab holding a LAYOUT is still named by that layout.
				rename: active.layout === undefined ? { label: `Rename ${thing}…`, run: rename(active) } : undefined,
				// In a strip, Split splits the TAB in front (da-6ji5). A lone
				// holder's children split themselves; its own split is a tab's.
				split: many
					? { what: 'split: a new pane', run: (t) => apply((l) => splitTab(l, pane.id, t)) }
					: holder
						? undefined
						: { what: 'split: a new pane', run: (t) => apply((l) => splitPane(l, pane.id, t)) },
				breakOut: canBreak ? { what: 'break out: this widget', run: (t) => apply((l) => breakOut(l, pane.id, t)) } : undefined,
				close: {
					label: many ? 'Close tab' : 'Close pane',
					disabled: !canClose,
					run: () => apply((l) => closeTab(l, pane.id, pane.active))
				},
				bar,
				acts,
				saveAs: holder && !root ? saveAsLayout(active.id) : undefined
			});

	/**
	 * The two slices of that one list (da-bzhq, `menu.ts`): the TITLE — or
	 * the tab in front — opens what this pane shows and what it is saved
	 * as; the `▾` beside the `+` opens what is done to the pane and its
	 * tabs. Under a lock `items` is the acts alone, none of them a
	 * management act, so `manage` is empty and the `▾` is not drawn — the
	 * same answer the `{#if !lockedHere}` round the pair gives, by the
	 * other road.
	 */
	$: own = named(items);
	$: manage = managed(items);

	/**
	 * `Rename tab…` / `Rename pane…` (da-6ji5, da-s7sdm): the answer is the
	 * tab's `title`. The field starts at the name it wears now; emptied,
	 * the title goes — the key with it, as `setLocked` drops `locked` — and
	 * the thing wears the name it is given again (`nameOf`). A user title
	 * OVERRIDES the default name and never replaces it as the source of one.
	 *
	 * It edits the tree, so it dirties the layout like any other edit
	 * (`isModified`, draft.ts, which compares everything but the lock).
	 */
	const rename = (tab: Tab) => () =>
		void ask(`Rename this ${thing} (empty: its own name again):`, 'rename', labelOf(tab), true).then((to) => {
			if (to !== null && to !== labelOf(tab)) apply((l) => setTitle(l, tab.id, to || undefined));
		});

	/** `Save as layout…` (da-6ji5): only when asked — a split is never saved by splitting. */
	const saveAsLayout = (tabId: string) => () =>
		void ask('Save this tab as the layout:', 'save').then((to) => {
			if (to) attempt(saveAs(tabId, to));
		});

	/**
	 * The `+`: the chooser, then a tab holding what it came back with —
	 * dismissed, it creates nothing. The names open above the new tab are
	 * the ones above its sibling, less that sibling's own.
	 */
	async function addChosen(): Promise<void> {
		const picked = await choose(ancestorsOf(active.id).filter((name) => name !== active.layout));
		if (!picked) return;
		const tab = newTab();
		apply((l) => addTab(l, pane.id, tab));
		await put(tab.id, picked);
	}

	const activate = (index: number) => apply((l) => setActive(l, pane.id, index));

	/** The tab strip, to put the focus back on the tab switched to. */
	let strip: HTMLElement | null = null;

	/**
	 * Arrow keys walk the tab strip; the tab focused is the tab shown.
	 *
	 * After the tick, since switching swaps which tab is the menu's button
	 * (da-9vtw) and so replaces two of the strip's elements: the tab to
	 * focus is the one that is there once it has.
	 */
	async function onTabKey(event: KeyboardEvent, index: number) {
		const forward = vertical ? 'ArrowDown' : 'ArrowRight';
		const back = vertical ? 'ArrowUp' : 'ArrowLeft';
		const next = event.key === forward ? index + 1 : event.key === back ? index - 1 : null;
		if (next === null || next < 0 || next >= pane.tabs.length) return;
		event.preventDefault();
		activate(next);
		await tick();
		strip?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
	}

	// ---- swipe -----------------------------------------------------------

	/**
	 * A gesture in hand (da-kcmp).
	 *
	 * `panners` is the walk from the finger to this frame's content box,
	 * done ONCE at the start — a computed style per ancestor is the cost,
	 * and it does not change under the finger. `state` is decided at the
	 * first move that commits an axis and never revisited: a scroll that
	 * runs out mid-gesture stays the browser's, and the NEXT swipe moves
	 * the tab.
	 */
	type Swipe = { x: number; y: number; panners: HTMLElement[]; state: 'pending' | 'ours' | 'theirs' };
	let touch: Swipe | null = null;

	/** The content box: what follows the finger, and what says how far off is off. */
	let contentEl: HTMLElement | null = null;
	/** How far the content is from home along the bar's axis, in px. */
	let offset = 0;
	/** Whether that offset is moving on its own — the slide, or the snap back. */
	let gliding = false;

	const inRange = (index: number) => index >= 0 && index < pane.tabs.length;
	/** The reader asked for no motion: the swap is instant, as it was. */
	const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

	/**
	 * Everything between `target` and `root` that may take motion along
	 * `axis`: a scroller with somewhere to go, and any element that says it
	 * takes the axis itself with `data-pans` — the nowview's band, and a
	 * dayview pan when it comes. One attribute, and no import either way.
	 */
	function pannersUnder(target: EventTarget | null, root: HTMLElement, axis: 'x' | 'y'): HTMLElement[] {
		const found: HTMLElement[] = [];
		for (let el = target as HTMLElement | null; el && el !== root; el = el.parentElement) {
			if ((el.dataset.pans ?? '').includes(axis)) {
				found.push(el);
				continue;
			}
			const style = getComputedStyle(el);
			const overflow = axis === 'x' ? style.overflowX : style.overflowY;
			if (overflow !== 'auto' && overflow !== 'scroll') continue;
			if (axis === 'x' ? el.scrollWidth > el.clientWidth : el.scrollHeight > el.clientHeight) found.push(el);
		}
		return found;
	}

	/**
	 * Can `el` still go the way the finger is going, from where it is? A
	 * finger moving one way drags the content that way, so the scroller
	 * goes the other. One pixel of slack: a fractional `scrollWidth` never
	 * quite reaches its end. An element that declares the axis always can.
	 */
	function takes(el: HTMLElement, axis: 'x' | 'y', along: number): boolean {
		if ((el.dataset.pans ?? '').includes(axis)) return true;
		const at = axis === 'x' ? el.scrollLeft : el.scrollTop;
		const box = axis === 'x' ? el.clientWidth : el.clientHeight;
		const whole = axis === 'x' ? el.scrollWidth : el.scrollHeight;
		return along > 0 ? at > 0 : at + box < whole - 1;
	}

	function onTouchStart(event: TouchEvent) {
		const t = event.touches[0];
		touch = {
			x: t.clientX,
			y: t.clientY,
			panners: pannersUnder(event.target, event.currentTarget as HTMLElement, vertical ? 'y' : 'x'),
			state: event.touches.length > 1 ? 'theirs' : 'pending'
		};
	}

	/** Until a gesture has moved this far it has not said which way it is going. */
	const COMMIT_PX = 10;

	/**
	 * The first move that commits an axis decides whose the gesture is, and
	 * the innermost frame that CAN move takes it: a scroller (or a panner)
	 * with room in the finger's direction keeps it, else this frame claims
	 * it if it has a tab that way — and stops it, so no frame above moves
	 * too. A frame on its last tab lets it through, which is how a swipe on
	 * the last tab of a nested pane reaches the root and moves the layout.
	 *
	 * The listener is non-passive so that a claimed gesture can be kept
	 * from the page's own scroller as well. `touch-action` would not do:
	 * it is read per element when the touch starts, and would forbid the
	 * inner scrollers their own pans.
	 */
	function onTouchMove(event: TouchEvent) {
		const was = touch;
		if (!was || was.state === 'theirs') return;
		if (event.touches.length > 1) return void cancel();
		const t = event.touches[0];
		const dx = t.clientX - was.x;
		const dy = t.clientY - was.y;
		const along = vertical ? dy : dx;
		const across = vertical ? dx : dy;
		if (was.state === 'pending') {
			if (Math.max(Math.abs(along), Math.abs(across)) < COMMIT_PX) return;
			const axis = vertical ? 'y' : 'x';
			const mine =
				Math.abs(along) > Math.abs(across) &&
				!was.panners.some((el) => takes(el, axis, along)) &&
				inRange(pane.active + (along < 0 ? 1 : -1));
			was.state = mine ? 'ours' : 'theirs';
			if (!mine) return;
		}
		event.stopPropagation();
		event.preventDefault();
		if (!still()) offset = along;
	}

	function onTouchEnd(event: TouchEvent) {
		const was = touch;
		touch = null;
		if (!was || was.state !== 'ours') return;
		event.stopPropagation();
		const t = event.changedTouches[0];
		const along = vertical ? t.clientY - was.y : t.clientX - was.x;
		// Pushing the content away from the bar's start is "next".
		const next = pane.active + (along < 0 ? 1 : -1);
		if (Math.abs(along) < SWIPE_PX || !inRange(next)) void glide(0);
		else void slide(next, along < 0 ? -1 : 1);
	}

	function cancel() {
		touch = null;
		void glide(0);
	}

	/**
	 * The swap, with the motion: the content goes the rest of the way off
	 * the way the finger went, the tab is switched under it, and what the
	 * box holds now comes in from the far side. Two phases, ONE box — the
	 * tab behind is already drawn in it (da-rjmg), so the switch runs
	 * nothing. A tab pressed in the strip, or reached by an arrow, still
	 * switches instantly: the motion belongs to the swipe.
	 */
	async function slide(next: number, way: number) {
		if (still()) return void activate(next);
		const extent = (vertical ? contentEl?.clientHeight : contentEl?.clientWidth) ?? 0;
		await glide(way * extent);
		activate(next);
		gliding = false;
		offset = -way * extent;
		await tick();
		// Read the layout back, so the far side is a frame of its own: two
		// writes in one frame are folded together and nothing would move.
		void contentEl?.clientWidth;
		await glide(0);
	}

	/** Move the content to `to` with the transition on, and wait it out. */
	async function glide(to: number): Promise<void> {
		if (offset === to) return;
		gliding = true;
		offset = to;
		await tick();
		await new Promise<void>((resolve) => {
			const el = contentEl;
			const done = () => {
				clearTimeout(timer);
				el?.removeEventListener('transitionend', onEnd);
				resolve();
			};
			const onEnd = (e: TransitionEvent) => {
				if (e.target === el && e.propertyName === 'transform') done();
			};
			const timer = setTimeout(done, SLIDE_MS + 50);
			el?.addEventListener('transitionend', onEnd);
		});
		gliding = false;
	}
</script>

<div class="host {kind === undefined ? '' : titleOf(kind)}">
<section
	class="{holder ? 'holder' : 'pane'} edge-{pane.bar.edge}"
	class:alarm={$alarm}
	class:vertical
	class:root
	class:nested
	class:bare
	class:clip={!root && kind !== undefined}
	data-pane={pane.id}
	data-title={title}
	style:--row="{ROW}px"
>
	{#if !bare}
	<div class="titlebar">
		<div class="along align-{pane.bar.align}">
			<!-- Maximize (da-00am) FIRST, before the tab names (da-bzhq).
			     vividn, 2026-09-21: «move the maximize button to be before the
			     tab names, rather than on the opposite side of the bar» —
			     which retires the far-corner placement da-jlgp asked for, so
			     the CSS that dealt it out to a corner is gone and the DOM
			     order is the drawn order again. Not under the lock, since a
			     lock keeps the LAYOUT from changing and this changes nothing
			     saved. -->
			<button
				type="button"
				class="control badge maximize"
				aria-label="{big ? 'restore' : 'maximize'} the {title} pane"
				title={big ? 'restore' : 'maximize'}
				aria-pressed={big}
				on:click={() => toggleMaximize(pane.id)}
			>
				<!-- No `width`/`height` of its own: the CSS insets it inside
				     the button, which a fixed 16px could not be. -->
				<svg viewBox="0 0 16 16" aria-hidden="true">
					{#if big}
						<!-- Two offset boxes: this one comes back among the others. -->
						<path d="M5.5 5.5h7v7h-7z" />
						<path d="M3.5 10.5v-7h7" />
					{:else}
						<!-- Four corner ticks: the box opens out to the page's. -->
						<path d="M2.5 6v-3.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10" />
					{/if}
				</svg>
			</button>
			{#if many}
				<div
					class="tabs"
					role="tablist"
					aria-orientation={vertical ? 'vertical' : 'horizontal'}
					bind:this={strip}
				>
					{#each pane.tabs as tab, i (tab.id)}
						{#if i === pane.active && own.length > 0}
							<!-- The tab in front is the menu's button; the others switch. -->
							<PaneMenu
								items={own}
								look="tab"
								label="menu of the {labelOf(tab)} tab"
								title={labelOf(tab)}
								role="tab"
								aria-selected="true"
								tabindex="0"
								on:keydown={(e) => onTabKey(e, i)}
								>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</PaneMenu
							>
						{:else}
							<button
								type="button"
								role="tab"
								class="control tab"
								class:modified={$modified.has(tab.id)}
								title={labelOf(tab)}
								aria-selected={i === pane.active}
								tabindex={i === pane.active ? 0 : -1}
								on:click={() => activate(i)}
								on:keydown={(e) => onTabKey(e, i)}
								>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</button
							>
						{/if}
					{/each}
				</div>
				{#if $hosted}
					<!-- A widget's OWN title controls — the custom pane's ✎ and
					     its editor — with no h2 of the frame's to be moved into
					     (da-ndjo): the strip carries the names, so the bar keeps
					     a box of the title's shape for the ACTIVE widget's
					     controls right after it. In a tabbed pane it is the only
					     place they can be. Not an h2: the tab is this pane's
					     heading, and this box has no words of its own. -->
					<div class="own" bind:this={h2}></div>
				{/if}
			{:else}
				<!-- The title's own box, and the menu's button laid over the
				     whole of it — beside the h2 rather than inside it, so that
				     the words may be clipped to their box and the button not. -->
				<div class="titlebox">
					<h2 bind:this={h2}>{#if !$hosted}{title}{#if $modified.has(active.id)}<span class="dot" title="modified">•</span>{/if}{/if}</h2>
					{#if own.length > 0}
						<PaneMenu items={own} look="title" label="menu of the {title} pane" />
					{/if}
				</div>
			{/if}
			<!-- Inside `.along`, after the title or the last tab: the `+`
			     and, beside it, the `▾` that holds what is done to this pane
			     and its tabs (da-bzhq). The DOM order is the drawn order and
			     the keyboard's both — maximize, title/tabs, `+`, `▾` — so it
			     is asserted rather than assumed (layout-maximize.test.mjs).

			     Both go under the lock together. vividn: "Locking hides both
			     the + and this down arrow icon menu" — nothing under a lock
			     can be split, tabbed or resized, and these two are how it
			     would be. The title's own menu stays, which is where
			     `Unlock layout` is (da-crp5). -->
			{#if !lockedHere}
				<button
					type="button"
					class="control badge add"
					aria-label="add a tab to the {title} pane"
					title="add a tab"
					on:click={() => void addChosen()}>+</button
				>
				{#if manage.length > 0}
					<PaneMenu
						items={manage}
						look="badge"
						label="manage the {title} pane"
						title="manage this pane"
						class="manage"
						aria-label="manage the {title} pane">▾</PaneMenu
					>
				{/if}
			{/if}
		</div>
	</div>
	{/if}
	<!-- svelte-ignore a11y-no-static-element-interactions -->
	<div
		class="content"
		class:gliding
		class:moving={offset !== 0 || gliding}
		bind:this={contentEl}
		style:transform={offset === 0 ? null : `translate${vertical ? 'Y' : 'X'}(${offset}px)`}
		style:--slide="{SLIDE_MS}ms"
		on:touchstart={many ? onTouchStart : undefined}
		on:touchmove|nonpassive={many ? onTouchMove : undefined}
		on:touchend={many ? onTouchEnd : undefined}
		on:touchcancel={many ? cancel : undefined}
	>
		<slot />
	</div>
</section>
</div>

<style>
	/* A name for the pane, not a box: the section is the box. */
	.host {
		display: contents;
	}

	section {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		/* The title, the tabs and the badge are all `--row` tall (`ROW`, set
		   on the element) and hang half of it outside the outline and half
		   over the content (Layout.svelte's chrome). The badge alone keeps a
		   finger's 24px where the pointer is one (reachable.test.mjs). */
		--badge: var(--row);
	}
	@media (pointer: coarse) {
		section {
			--badge: 24px;
		}
	}

	.pane {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 6px;
	}

	/* A holder has no outline to sit on, so its bar stays a row of its own.
	   The bar's edge is the flex direction: the bar is always first in the
	   DOM (so it is first for the keyboard) and the direction puts it on
	   the edge asked for. */
	.holder.edge-bottom {
		flex-direction: column-reverse;
	}
	.holder.edge-left {
		flex-direction: row;
	}
	.holder.edge-right {
		flex-direction: row-reverse;
	}

	.titlebar {
		display: flex;
		align-items: center;
		flex: none;
		min-width: 0;
		min-height: 0;
	}

	/* A holder's bar is a row of its own, and the titles of the panes under
	   it straddle the top of the content right below it (da-om9v): the row
	   stays above them, or the tab in front — the button that opens the
	   menu (da-9vtw) — is half covered by the first child's title. */
	.holder > .titlebar {
		position: relative;
		z-index: 3;
	}

	/* On a pane the bar is a LINE, not a box: zero thick, laid down the
	   middle of the 1px outline (half a pixel out from the padding box),
	   with its items centred on it — so they straddle the outline, as much
	   out as in. It takes no pointer itself (the border beside it must
	   stay grabbable); its items do. */
	.pane > .titlebar {
		position: absolute;
		z-index: 2;
		pointer-events: none;
	}
	.pane.edge-top > .titlebar {
		top: -0.5px;
		left: 0;
		right: 0;
		height: 0;
	}
	.pane.edge-bottom > .titlebar {
		bottom: -0.5px;
		left: 0;
		right: 0;
		height: 0;
	}
	.pane.edge-left > .titlebar {
		left: -0.5px;
		top: 0;
		bottom: 0;
		width: 0;
	}
	.pane.edge-right > .titlebar {
		right: -0.5px;
		top: 0;
		bottom: 0;
		width: 0;
	}

	/* How far the title lies over the content on its edge, for a widget
	   whose own controls sit in that corner to keep clear of it (the
	   nowview's steps). The display underneath is not moved. */
	.pane.edge-top:not(.bare) {
		--under-top: calc(var(--row) / 2);
	}
	.pane.edge-left:not(.bare) {
		--under-left: calc(var(--row) / 2);
	}

	/* Along the left and the right the words are rotated. `vertical-rl`
	   reads top-to-bottom, which is right for the right-hand edge; the
	   left-hand bar is turned round so it reads bottom-to-top, like the
	   spine of a book on a shelf (vividn, 15:39). A half turn maps the bar's
	   box onto itself, so nothing else moves. */
	.vertical > .titlebar {
		writing-mode: vertical-rl;
	}
	.edge-left > .titlebar {
		transform: rotate(180deg);
	}

	.along {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		min-height: 0;
		pointer-events: none;
		/* Off the rounded corners. Logical: along the bar, whichever way it runs. */
		padding-inline: 0.5rem;
	}
	.holder > .titlebar > .along {
		padding-inline: 0;
	}
	.align-start {
		justify-content: flex-start;
	}
	.align-center {
		justify-content: center;
	}
	.align-end {
		justify-content: flex-end;
	}
	/* The box the title is in: the words, and over the whole of them the
	   button that opens the menu (`.title`, PaneMenu.svelte). It is what
	   positions that button, and is exactly the h2 — which is why the
	   button is not inside the h2, whose overflow is hidden to clip the
	   words and would clip the button's outer pixels with them. */
	.titlebox {
		position: relative;
		display: flex;
		flex: 0 1 auto;
		min-inline-size: 0;
		min-block-size: 0;
	}

	/* The words and a line round them, the frame's own: nothing across
	   the bar but the outline, whichever way the bar runs. A widget's own
	   controls in here (the custom pane's ✎) are lifted above the button
	   laid over them, and are `:global` because the widget draws them. */
	h2,
	.own {
		box-sizing: border-box;
		margin: 0;
		padding-block: 0;
		padding-inline: 0.4rem;
		/* The row's height, and a finger's where the pointer is coarse — the
		   title is a button now (da-9vtw), and a button is on the floor
		   `reachable.test.mjs` holds the badges to. */
		block-size: var(--badge);
		line-height: calc(var(--badge) - 2px);
		min-inline-size: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.75rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: var(--accent);
		white-space: nowrap;
		pointer-events: auto;
		/* Its own ground, so the outline stops at the words rather than
		   running through them. */
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 4px;
	}

	h2 :global(button),
	h2 :global(input),
	.own :global(button),
	.own :global(input) {
		position: relative;
		z-index: 1;
	}

	/* The modified dot (da-crp5): the same mark the root bar puts on a
	   modified layout's tab, in the same orange, wherever the copy is
	   drawn. `:global` on the tabs, since the tab in front is PaneMenu's
	   button and carries no scope of this file's. */
	.dot,
	.tabs :global(.dot) {
		margin-inline-start: 0.3rem;
		color: var(--warn);
	}

	/* The hosted box (da-ndjo) is the widget's controls and nothing else —
	   no words of its own, so not a heading, and no menu laid over it:
	   the tab in front is that. It gives its room back before the tabs do,
	   and it does not say it can be pressed. */
	.own {
		flex: 0 1 auto;
		min-inline-size: 0;
	}

	/* The whole title says it can be pressed. */
	h2:hover {
		color: var(--accent-bright);
		border-color: var(--accent);
	}

	/* The three badges do not stretch, and take the pointer where the line
	   they sit on does not — and nothing here MOVES one. The maximize was
	   dealt out to the bar's far corner from da-jlgp until vividn,
	   2026-09-21: «move the maximize button to be before the tab names,
	   rather than on the opposite side of the bar» (da-bzhq). So the
	   `order` and the auto margins that did it are gone, and the row is
	   drawn in the order it is written, at every alignment and on a
	   rotated bar too — which is the order the Tab walk reads.

	   The `▾` needs nothing else of its own: it is the same
	   `.control.badge` (app.css) as the `+`, one character centred in a
	   square, and its name is its `aria-label` as the `+`'s is. */
	.add,
	.maximize,
	.along :global(button.manage) {
		flex: none;
		pointer-events: auto;
	}

	/* «let's push the maximize button into the corner more» (vividn,
	   2026-09-21, da-fphjw). The row is held 0.5rem off the rounded
	   corners; the maximize alone gives 6px of that back, so it sits 2px
	   from the corner the bar STARTS at while the `+` and the `▾` keep
	   their clearance at the other end. Logical, so it is the same corner
	   whichever edge the bar is on and on the left-hand bar's half turn
	   too — the corner alignment calls `start`. A holder's row has no
	   inset to give back, so the rule is the pane's alone. */
	.pane > .titlebar > .along > .maximize {
		margin-inline-start: -0.375rem;
	}

	/* A flat mono-colour mark in `currentColor` (da-tnv3's rule for the
	   lock): strokes, so the two states are the same weight, and `block`
	   so a rotated bar does not give the box a baseline's slack.

	   The box is INSIDE the button, not the 16px the viewBox is: a badge is
	   `--badge` square with a 1px outline, so a 16px glyph on a 16px badge
	   hung a pixel out of each side (da-jlgp). FOUR pixels in from the
	   border box at either badge size — three until «it can be a bit
	   smaller as well» (vividn, 2026-09-21, da-fphjw) — and square, so which
	   way the bar runs makes no difference to it. The BUTTON is not
	   touched: the mark shrinks and the hit area stays the 16px/24px floor
	   `reachable.test.mjs` holds it to. The stroke does not scale with the
	   box — a hairline at 8px and a slab at 16px are the same mark drawn
	   twice — so it stays the 1.25px it was drawn as. */
	.maximize svg {
		display: block;
		inline-size: calc(var(--badge) - 8px);
		block-size: calc(var(--badge) - 8px);
		fill: none;
		stroke: currentColor;
		stroke-width: 1.25;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.maximize svg path {
		vector-effect: non-scaling-stroke;
	}

	/* Filling the page is a state, so it is drawn as one. */
	.maximize[aria-pressed='true'] {
		color: var(--accent);
		border-color: var(--accent);
	}

	.tabs {
		display: flex;
		gap: 4px;
		min-width: 0;
		min-height: 0;
		/* A strip longer than its bar is CUT at the bar, never drawn past
		   it: a pane's bar lies along that pane's outline, so anything of
		   it outside the pane is inside a neighbour (da-om9v's rule). It
		   was four pixels from happening on a phone with three tabs on a
		   vertical bar, and the taller bottom bar of da-xfa4 spent those
		   four; the tabs give way instead, which is what everything else
		   in this app does when it runs out of room. */
		overflow: hidden;
		pointer-events: auto;
	}

	/* `.control` (app.css) is the pill. On the line a tab is the title's
	   shape and height, and a smaller radius than a control gets elsewhere
	   — the line it straddles is 1px, and 6px of round on a 16px box reads
	   as a lozenge. `:global`, and under `.tabs` so that it reaches no
	   further: the tab in front is drawn by PaneMenu, being the button
	   that opens the menu (da-9vtw), and carries no scope of this file's. */
	.tabs :global(.tab) {
		block-size: var(--row);
		padding-block: 0;
		padding-inline: 0.5rem;
		border-radius: 4px;
		font-size: 0.75rem;
		line-height: 1;
		letter-spacing: 0.06em;
		white-space: nowrap;
		/* `min-inline-size: auto` is a flex item's own words, so a strip
		   of nowrap tabs cannot shrink at all without this. With it they
		   share what the bar has and each cuts to an ellipsis. */
		min-inline-size: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* The tab in front says so with its outline, which is why the hover
	   below is here and not on `.control`: it has to be able to argue with
	   this rule, and it can only do that from inside the component. */
	.tabs :global(.tab[aria-selected='true']) {
		color: var(--accent);
		border-color: var(--accent);
	}

	.tabs :global(.tab:hover),
	.tabs :global(.tab:focus-visible) {
		color: var(--accent-bright);
	}

	/* Orange rather than red: this is "your data is not safe yet", which
	   is a warning about the future, not a report of a failure that
	   already happened. It has to be visible at a glance from across a
	   desk. (From Pane.svelte, which drew it before there was a frame.) */
	.pane.alarm {
		border-color: var(--alarm);
		box-shadow: 0 0 0 1px var(--alarm) inset;
	}
	.pane.alarm h2 {
		background: var(--alarm);
		border-color: var(--alarm);
		/* Not a token: the almost-black the alarm's orange is legible on,
		   and nothing else in the app is painted on orange. */
		color: #1a1000;
	}

	/* The content box: `Pane.svelte` inside a frame is `display:
	   contents`, so its scrolling body is this box's own flex item and
	   takes the room. */
	.content {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	/* The swipe's motion (da-kcmp): under the finger the content follows
	   it with no transition at all; the slide the release finishes and the
	   snap back of one too short are the same box, moved with one. Under
	   `prefers-reduced-motion` no transform is ever set (the handlers), so
	   the swap is the instant one it was. */
	.content.gliding {
		transition: transform var(--slide) ease-out;
	}
	/* A pane dragged down to its frame (da-zh2o) shows nothing of its
	   widget outside it — the body's own padding included. A widget with
	   more in it than the box holds scrolls inside its own body. */
	.clip > .content {
		overflow: hidden;
	}

	/* Only the root scrolls, of the frames: a split whose children's
	   floors do not fit overflows into it and the page scrolls to the
	   rest. A widget's own body may scroll inside its pane. */
	.root > .content {
		overflow: auto;
	}
	/* A transformed box widens the overflow of whatever scrolls it — and
	   the root's content IS the page's scroller — so while it is moving,
	   nothing scrolls to the part hanging out. */
	.content.moving {
		overflow: hidden;
	}
	/* A layout's panes are inset from its frame: the page's own gap all
	   round (`BORDER`, Layout.svelte — half a row), so the two outlines
	   read as two rather than as one thick line, and the WHOLE row on the
	   bar's edge, where both the layout's title and its first pane's
	   straddle their own outline and each reaches half a row over the
	   other's. Without it they land on each other and the layout's name is
	   not there at all (da-9puz). `chromeOf` counts this. */
	.pane.nested > .content {
		padding: calc(var(--row) / 2);
	}
	.pane.nested.edge-top > .content {
		padding-top: var(--row);
	}
	.pane.nested.edge-bottom > .content {
		padding-bottom: var(--row);
	}
	.pane.nested.edge-left > .content {
		padding-left: var(--row);
	}
	.pane.nested.edge-right > .content {
		padding-right: var(--row);
	}

	/* A holder's strip is a row of its own, and the panes under it hang
	   their titles — and their badges — half a row above their outlines:
	   the content keeps that half clear on the strip's edge, or the strip
	   lies over them (da-cesj). Only a holder with a strip; one without
	   draws none and moves nothing. `chromeOf` counts this. */
	.holder.edge-top:not(.bare) > .content {
		padding-top: calc(max(var(--row), var(--badge)) / 2);
	}
	.holder.edge-bottom:not(.bare) > .content {
		padding-bottom: calc(max(var(--row), var(--badge)) / 2);
	}
	.holder.edge-left:not(.bare) > .content {
		padding-left: calc(max(var(--row), var(--badge)) / 2);
	}
	.holder.edge-right:not(.bare) > .content {
		padding-right: calc(max(var(--row), var(--badge)) / 2);
	}

	/* The titles along the page's own edges hang half their row outside
	   their panes — their badges half theirs — and the scroller would cut
	   that half off: it reaches out over the page's margin by that much
	   and pads it back, so the panes stay where they were and their titles
	   are whole. */
	.holder.root.bare > .content {
		margin: calc(max(var(--row), var(--badge)) / -2);
		padding: calc(max(var(--row), var(--badge)) / 2);
	}
</style>
