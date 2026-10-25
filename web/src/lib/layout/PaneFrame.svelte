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
	`+`, `×` (da-bzhq, da-efr4i): the maximize BEFORE the names, and after
	them the badges that add a tab and close what the title names — the
	destructive one last, at the far end from the `+`, the way `Close` is
	the far end of the menu.

	TWO MODES (da-vvcxc). In normal mode a press on the title — or on the
	tab in front — opens what the pane IS (`named`, menu.ts), there is no
	`+` and no `×`, and nothing moves the tree. In LAYOUT MODE (`mode.ts`,
	entered from the open root tab) the same press opens what is DONE to
	the pane (`arranged`): Change Widget, Rename, Split, Break out, `Widget
	options ▸` holding what normal mode opens, and Close; the outline is
	the accent, the `+` and `×` are back, and the maximize is the bar's
	HANDLE, dragged to the edge the bar should be on. A HOLD on the name
	in layout mode opens `Change Widget` (da-efr4i, vividn: «Long pressing or
	right clicking on a widget title opens the change widget menu»). It is
	the app's ONE hold ($lib/hold.ts), and the `contextmenu` it swallows
	is the title's alone: a reader right-clicking a pane's CONTENT still
	gets the browser's menu, which is where text is selected and copied.

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

	The MAXIMIZE is the one item that straddles BOTH borders: its centre
	is on the corner point itself, as much of it outside the pane as in on
	either axis (da-lihui). And it is the one whose drawn box is smaller
	than the box a finger lands on — the old beta ⓘ's pattern, so that «a bit
	smaller» could be answered without taking the fingertip down with it.

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
	page's top bar (da-0a5o).

	A layout tab's frame is also where that layout's identity is worked
	(da-crp5): the acts the root bar draws for a root tab — save, restore,
	save as, and `Reset layout` where the layout is one that SHIPPED with
	datum (da-luwe) — are in this menu when the tab holds a layout,
	whatever its depth, and the orange modified dot follows the title or
	the tab.

	Swiping across the content moves between tabs along the bar's axis:
	left/right under a horizontal bar, up/down under a vertical one. Touch
	only, and with the motion under the finger (da-kcmp). Whose the gesture
	is, is settled at the first move that commits an axis, by DIRECTION and
	not by what merely exists under the finger:

	  - a scroller between the finger and this frame's content box — or an
	    element declaring the axis its own with `data-pans` (the nowview's
	    band; a day strip's, but not at an end its `data-pan-stop` names) —
	    keeps the gesture while it can still go that way. At its
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
	  - for `AGAIN_MS` after a frame's tab switch lands, a swipe along its
	    axis that has a tab to go to is that frame's wherever it starts,
	    ahead of scrollers, panners and tabbed panes inside — the root's
	    after a layout switch (da-d9p5m), vividn: "a swipe anywhere on the
	    new tab keeps switching tabs, even if otherwise a widget would have
	    taken that swipe internally".

	Then the tab and its neighbour move as ONE STRIP (da-24fgb): the tab
	the finger is bringing on is drawn beside the one in front, a box away,
	and the two follow the finger together; the release slides the strip
	the rest of the way and the neighbour lands where the tab was. At no
	instant is the box empty — vividn: "swiping between tabs or widgets
	still has an awkward nomans land between them, the swiping should go
	immediately between the two widgets without the blank space". The
	content box itself stays still and clips the strip, and nothing moves
	until the finger is past `SWIPE_PX`, so a short drag is still a touch.
	Every tab of a pane is mounted and drawn already (da-rjmg,
	`TabBody.svelte`), so what moves is a picture, not a mount; the root
	mounts only its layout in front, and the neighbour for as long as a
	gesture is in hand (`peek`).
-->
<script lang="ts" context="module">
	import { readable } from 'svelte/store';

	/**
	 * The bar's row, in pixels: the title's words (0.75rem at a line of
	 * 14px) and its 1px outline. The frame's `--row` and Layout.svelte's
	 * chrome both read it, so the floors follow the row drawn. Half of it
	 * hangs outside the outline, and must fit the gap between panes
	 * (Layout.svelte's `BORDER`).
	 */
	export const ROW = 16;

	/**
	 * The badge's box, in pixels — the frame's `--badge`: the row where the
	 * pointer is fine, a finger's 24px where it is coarse. Layout.svelte's
	 * chrome reads it too, so a tabbed holder's floor counts the strip a
	 * finger is given and not the one a mouse is (da-kpjn).
	 */
	export const badge = readable(ROW, (set) => {
		if (typeof matchMedia !== 'function') return;
		const coarse = matchMedia('(pointer: coarse)');
		const read = () => set(coarse.matches ? 24 : ROW);
		read();
		coarse.addEventListener('change', read);
		return () => coarse.removeEventListener('change', read);
	});

	/**
	 * The frame whose tab switched last, and until when a swipe across it
	 * is its own (da-d9p5m). One for the page, so a switch anywhere else
	 * ends it.
	 */
	let switched: { frame: object; until: number } | null = null;
</script>

<script lang="ts">
	import { setContext, tick } from 'svelte';
	import { act } from '$lib/act.js';
	import { ask } from '$lib/confirm.js';
	import { holds } from '$lib/hold.js';
	import { writable } from 'svelte/store';
	import type { BarAlign, BarEdge, Pane, Tab } from './types.js';
	import { isSplit } from './types.js';
	import { apply, layout } from './current.js';
	import { choose, fillTab, put } from './chooser.js';
	import { ancestorsOf, modified, resetLayout, resetWidget, restore, save, saveAs, shippedTree, sliding } from './shell.js';
	import { addTab, breakOut, canBreakOut, closeTab, newTab, setActive, setBar, setTitle, splitPane, splitTab } from './tree.js';
	import { inForce, toggleMaximize } from './maximize.js';
	import { labelOf, titleOf } from './registry.js';
	import { CLOSE, arranged, menuItems, named } from './menu.js';
	import { offers, widgetOptions } from './options.js';
	import { keyOf, session } from './session.js';
	import PaneMenu from './PaneMenu.svelte';

	export let pane: Pane;
	/** False on the last pane: the close item is greyed, not hidden. */
	export let canClose = true;
	/** The layout's root: the one pane whose content box is the scroller. */
	export let root = false;
	/**
	 * NORMAL MODE (da-vvcxc): the page is not in layout mode. Then there is
	 * no `+` and no `×`, the name opens what the pane IS, and the tab strip
	 * still switches; in layout mode the name opens what is DONE to the
	 * pane, the outline is drawn in the accent, and the maximize is the
	 * bar's handle.
	 */
	export let locked = false;
	/**
	 * The active widget's kind. Its title goes on a wrapper round the frame
	 * as a class — `.repl`, `.tail`, `.dayview` — which is how the old
	 * grid's areas were named and how the rest of the app and its tests
	 * still find a pane's insides.
	 */
	export let kind: string | undefined = undefined;

	/** A swipe shorter than this is a touch, not a swipe. */
	const SWIPE_PX = 40;
	/**
	 * How long the strip takes to finish the swipe the finger started. The
	 * transition reads it as `--slide`, so the wait after the transform and
	 * the transition itself are the one number.
	 */
	const SLIDE_MS = 150;
	/**
	 * How long after this frame's tab switch lands a swipe along its axis is
	 * its own wherever it starts — over a scroller, a panner or a tabbed
	 * pane inside — so a run of swipes keeps moving through the same tabs
	 * (vividn, da-d9p5m). A second: time for the next flick of a run, and
	 * short of a reader who has looked at the tab and reached into it.
	 */
	const AGAIN_MS = 1000;

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
	/**
	 * The `×`'s act, for a widget that offers it in its own body (the
	 * welcome widget's «Close welcome», da-zhz0u) — or null wherever the
	 * `×` is not offered: outside layout mode below the root (a root tab's
	 * close is a way out of the page, not a change to it), and on the
	 * root's last tab.
	 */
	const close = writable<(() => void) | null>(null);
	setContext('layout-frame', { alarm, titleHost, hosted, close });
	let h2: HTMLElement | null = null;
	$: titleHost.set(h2);

	$: active = pane.tabs[pane.active];
	$: close.set(
		(locked && !root) || !canClose ? null : () => apply((l) => closeTab(l, pane.id, pane.active))
	);
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

	/**
	 * What this session has set on the tab in front (da-vhwq): the option
	 * items' state. `keyOf` walks the tab's own path, which is the ONE
	 * derivation the renderer and the store use too — see Layout.svelte.
	 */
	$: sessionOf = keyOf($layout, active.id);
	$: held = $session[sessionOf] ?? {};
	/**
	 * The widget's own options, inline (da-vhwq): drawn from the registry's
	 * declarations and changed in the menu, and — with something held —
	 * ending in «Save options with layout».
	 */
	$: options = widgetOptions(active, held, sessionOf, $offers);

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
	 * is a fact about the LAYOUT and so is the same in either menu.
	 */
	$: acts =
		active.layout === undefined
			? undefined
			: {
					dirty: $modified.has(active.id),
					save: () => attempt(save(active.id)),
					restore: () => attempt(restore(active.id)),
					reset: shippedTree(active.layout) ? () => attempt(resetLayout(active.id)) : undefined,
					saveAs: () => {
						const to = prompt('Save this layout as:', '')?.trim();
						if (to) attempt(saveAs(active.id, to));
					}
				};

	/**
	 * The menu, from the one builder the root bar uses too (`menu.ts`). In
	 * normal mode it is what the pane IS (`named`): the widget's reset and
	 * own options, and the acts of a layout it holds. In layout mode it is
	 * what is done to it (`arranged`), with that list one step in under
	 * `Widget options` (vividn: «Widget Options > (rather than being the
	 * primary menu, this goes in a secondary menu when in layout mode»).
	 */
	$: all = menuItems({
		choose: () => void fillTab(active.id),
		// About the widget in front.
		reset: resetWidget(active),
		options,
		// A PANE IS RENAMEABLE TOO (da-s7sdm), not just a tab of a
		// strip: with one tab the tab IS the pane, and the name on the
		// frame is that tab's. Named after whichever it is, as `close`
		// is. A tab holding a LAYOUT is still named by that layout.
		rename: active.layout === undefined ? { label: `Rename ${thing}`, run: rename(active) } : undefined,
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
		acts,
		saveAs: holder && !root ? saveAsLayout(active.id) : undefined
	});
	$: own = locked ? named(all) : arranged(all);

	/**
	 * THE SECOND DOOR ON `Change Widget` (da-efr4i). vividn, 2026-09-22:
	 * «Long pressing or right clicking on a widget title opens the change
	 * widget menu.» A hold on the title with one tab, on the tab in front
	 * with more.
	 *
	 * Layout mode's alone, like the item itself (da-vvcxc): undefined in
	 * normal mode, which turns the gesture off entirely ($lib/hold.ts). The
	 * gesture is the app's ONE hold — the same 450ms, the same right click,
	 * the same menu key — and it is `preventDefault`ed only here: a reader
	 * right-clicking a pane's CONTENT still gets the browser's own menu,
	 * which is where text is selected and copied.
	 */
	$: changeWidget = locked ? undefined : () => void fillTab(active.id);

	/**
	 * `Rename tab` / `Rename pane` (da-6ji5, da-s7sdm): the answer is the
	 * tab's `title`. The field starts at the name it wears now; emptied,
	 * the title goes — the key with it — and
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

	/** `Save as layout` (da-6ji5): only when asked — a split is never saved by splitting. */
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

	// ---- the bar's handle (da-vvcxc) ------------------------------------

	/**
	 * In layout mode the maximize badge is the BAR'S HANDLE. vividn,
	 * 2026-10-14: «rather than a bar menu, the maximaize button becomes a
	 * handle where you can drag the bar to different positions on the
	 * pane». Dropped, the bar goes to the edge of the pane nearest the
	 * pointer, gathered at the third of that edge it is over (`bar.align`,
	 * read the way the eye reads it: the left-hand bar runs bottom to top).
	 * The arrows move it to an edge from the keyboard. A tap does nothing.
	 */
	let section: HTMLElement;
	let handle: { id: number; x: number; y: number } | null = null;
	/** Where a drop would put the bar now, drawn on that edge while the handle is held. */
	let dropAt: { edge: BarEdge; align: BarAlign } | null = null;

	function placeAt(x: number, y: number): { edge: BarEdge; align: BarAlign } {
		const r = section.getBoundingClientRect();
		const gap = { top: y - r.top, bottom: r.bottom - y, left: x - r.left, right: r.right - x };
		const edge = (Object.keys(gap) as BarEdge[]).reduce((a, b) => (gap[b] < gap[a] ? b : a));
		const along =
			edge === 'top' || edge === 'bottom'
				? (x - r.left) / r.width
				: edge === 'right'
					? (y - r.top) / r.height
					: (r.bottom - y) / r.height;
		return { edge, align: along < 1 / 3 ? 'start' : along < 2 / 3 ? 'center' : 'end' };
	}

	function grabBar(event: PointerEvent) {
		if (locked) return;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		handle = { id: event.pointerId, x: event.clientX, y: event.clientY };
	}

	function dragBar(event: PointerEvent) {
		if (!handle || event.pointerId !== handle.id) return;
		if (!dropAt && Math.hypot(event.clientX - handle.x, event.clientY - handle.y) < 4) return;
		dropAt = placeAt(event.clientX, event.clientY);
	}

	function dropBar() {
		const to = dropAt;
		handle = null;
		dropAt = null;
		if (to) apply((l) => setBar(l, pane.id, to));
	}

	const ARROW: Record<string, BarEdge> = { ArrowUp: 'top', ArrowDown: 'bottom', ArrowLeft: 'left', ArrowRight: 'right' };

	function keyBar(event: KeyboardEvent) {
		const edge = ARROW[event.key];
		if (locked || !edge) return;
		event.preventDefault();
		apply((l) => setBar(l, pane.id, { ...pane.bar, edge }));
	}

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
	 * the tab. `fresh` is whether it started inside this frame's window
	 * after a switch (`AGAIN_MS`), read at the start too.
	 */
	type Swipe = {
		x: number;
		y: number;
		target: EventTarget | null;
		panners: HTMLElement[];
		fresh: boolean;
		state: 'pending' | 'ours' | 'theirs';
	};
	let touch: Swipe | null = null;
	/** The finger's pointer id, for the cancel a window's claim sends what is under it. */
	let finger = -1;

	/** Any change of the tab in front — a swipe landing, a tap, the root bar — opens the window. */
	const self = {};
	let shown = pane.active;
	$: if (pane.active !== shown) {
		shown = pane.active;
		switched = { frame: self, until: performance.now() + AGAIN_MS };
	}

	/** The content box: the window the strip moves in, and what says how far off is off. */
	let contentEl: HTMLElement | null = null;
	/** How far the strip is from home along the bar's axis, in px. */
	let offset = 0;
	/**
	 * The tab drawn beside the one in front — the one the finger is
	 * bringing on — or -1. Handed to the slot, so the root mounts it.
	 */
	let peek = -1;
	/** Whether that offset is moving on its own — the slide, or the snap back. */
	let gliding = false;
	/**
	 * How far in from the root's frame a root tab's layout lands: a single
	 * pane inside the frame's 1px outline (`.pane`), a split on it.
	 */
	const lands = (tab: Tab | undefined) => (tab && !isSplit(tab.content) ? 1 : 0);
	/**
	 * How much smaller the neighbour lands than the tab in front, each side
	 * (da-466lv): its box is the front's, so it is drawn in the box it will
	 * land in rather than measured at one and drawn again at the other.
	 */
	$: shy = root && peek >= 0 ? lands(pane.tabs[peek]) - lands(active) : 0;

	/**
	 * Whether a swipe moves between this pane's tabs: not the root's in
	 * layout mode, whose tabs are the layouts and which is left only by
	 * asking how (`leave`, mode.ts — the root bar's tabs do).
	 */
	$: swipes = many && (locked || !root);

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
	 * quite reaches its end. An element that declares the axis can, but at
	 * an end it names in `data-pan-stop` — `start`, `end` or both, a day
	 * strip's hour window (da-xg1vi).
	 */
	function takes(el: HTMLElement, axis: 'x' | 'y', along: number): boolean {
		if ((el.dataset.pans ?? '').includes(axis))
			return !(el.dataset.panStop ?? '').includes(along > 0 ? 'start' : 'end');
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
			target: event.target,
			panners: pannersUnder(event.target, event.currentTarget as HTMLElement, vertical ? 'y' : 'x'),
			fresh: switched?.frame === self && performance.now() < switched.until,
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
	 *
	 * Inside the window after a switch (da-d9p5m) the frame also hears the
	 * move in the capture phase, `windowed`, ahead of everything under it,
	 * and takes it if it has a tab that way, asking nothing under the
	 * finger; else the move goes on to the rule above.
	 */
	function onTouchMove(event: TouchEvent, windowed = false) {
		const was = touch;
		if (!was || was.state === 'theirs' || (windowed && !was.fresh)) return;
		if (event.touches.length > 1) return void cancel();
		const t = event.touches[0];
		const dx = t.clientX - was.x;
		const dy = t.clientY - was.y;
		const along = vertical ? dy : dx;
		const across = vertical ? dx : dy;
		if (was.state === 'pending') {
			if (Math.max(Math.abs(along), Math.abs(across)) < COMMIT_PX) return;
			const axis = vertical ? 'y' : 'x';
			// A widget that has already taken the move for a gesture of its
			// own — a day strip's long-press range at the whole day, which is
			// at both ends — keeps it (da-xg1vi).
			const toward = Math.abs(along) > Math.abs(across) && inRange(pane.active + (along < 0 ? 1 : -1));
			if (windowed && !toward) return;
			const mine = toward && (windowed || (!event.defaultPrevented && !was.panners.some((el) => takes(el, axis, along))));
			was.state = mine ? 'ours' : 'theirs';
			if (!mine) return;
			// A widget panning by pointer had the first moves: it is told the
			// gesture is gone, as the browser tells it when it takes a pan.
			if (windowed)
				was.target?.dispatchEvent(new PointerEvent('pointercancel', { pointerId: finger, pointerType: 'touch', bubbles: true }));
		}
		event.stopPropagation();
		event.preventDefault();
		if (still()) return;
		// The strip waits out the first SWIPE_PX, then follows from there,
		// and never towards a side with no tab on it.
		const way = along < 0 ? 1 : -1;
		if (inRange(pane.active + way)) peek = pane.active + way;
		offset = Math.abs(along) > SWIPE_PX && peek === pane.active + way ? along + way * SWIPE_PX : 0;
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
		if (Math.abs(along) < SWIPE_PX || !inRange(next)) void home();
		else void slide(next, along < 0 ? -1 : 1);
	}

	function cancel() {
		touch = null;
		void home();
	}

	/** Back where it started, and the neighbour put away. */
	async function home() {
		await glide(0);
		if (!touch) peek = -1;
	}

	/**
	 * The swap, with the motion: the strip goes the rest of the way, which
	 * brings the neighbour exactly home, and in the same frame the tab is
	 * switched and the strip put back — the neighbour becoming the tab in
	 * front where it already stood, so nothing on screen moves. A tab
	 * pressed in the strip, or reached by an arrow, still switches
	 * instantly: the motion belongs to the swipe.
	 */
	async function slide(next: number, way: number) {
		if (still()) return void activate(next);
		const extent = (vertical ? contentEl?.clientHeight : contentEl?.clientWidth) ?? 0;
		await glide(way * extent);
		activate(next);
		offset = 0;
		peek = -1;
	}

	/** A slide the shell asked for is under way: the strip stands a box apart before it moves. */
	let arriving = false;
	/** The crossfade that stands in for a slide under reduced motion. */
	let fading = false;
	/** The tab asked for while a slide was under way: slid to when it lands. */
	let queued: string | null = null;

	/**
	 * A slide the SHELL asks for (`sliding`, da-6du3o) — a place coming in
	 * from the right, or going back to the tab it opened beside — is a
	 * swipe's release without the finger: the neighbour is set a box away
	 * first, so the glide starts from there. Under reduced motion, a fade.
	 * One at a time: a request during a slide waits for it, and is found by
	 * its id once the slide's own close has moved the indices.
	 */
	$: if (root && $sliding !== null) void goTo($sliding);

	async function goTo(id: string) {
		sliding.set(null);
		if (arriving) return void (queued = id);
		const index = pane.tabs.findIndex((t) => t.id === id);
		if (!inRange(index) || index === pane.active) return;
		if (still()) {
			fading = true;
			return activate(index);
		}
		peek = index;
		arriving = true;
		await tick();
		// The strip's resting place is computed before the transition goes on.
		void contentEl?.offsetWidth;
		await slide(index, index > pane.active ? -1 : 1);
		arriving = false;
		// After the tick that closes a place left behind, so the id is found where it now is.
		await tick();
		const next = queued;
		queued = null;
		if (next !== null) void goTo(next);
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
				if ((e.target as Element).parentElement === el && e.propertyName === 'transform') done();
			};
			const timer = setTimeout(done, SLIDE_MS + 50);
			el?.addEventListener('transitionend', onEnd);
		});
		gliding = false;
	}
</script>

<div class="host {kind === undefined ? '' : titleOf(kind)}" data-widget={kind}>
<section
	bind:this={section}
	class="{holder ? 'holder' : 'pane'} edge-{pane.bar.edge}"
	class:alarm={$alarm}
	class:vertical
	class:root
	class:nested
	class:bare
	class:clip={!root && kind !== undefined}
	class:editing={!locked}
	data-bar-drop={dropAt?.edge}
	data-pane={pane.id}
	data-title={title}
	style:--row="{ROW}px"
	style:--badge="{$badge}px"
>
	{#if !bare}
	<div class="titlebar">
		<div class="along align-{pane.bar.align}">
			<!-- Maximize (da-00am) FIRST, before the tab names (da-bzhq).
			     vividn, 2026-09-21: «move the maximize button to be before the
			     tab names, rather than on the opposite side of the bar» —
			     which retires the far-corner placement da-jlgp asked for, so
			     the CSS that dealt it out to a corner is gone and the DOM
			     order is the drawn order again. In normal mode, since it
			     changes nothing saved; in layout mode it is the bar's HANDLE
			     (da-vvcxc, `grabBar` above). -->
			{#if locked}
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
			{:else}
			<button
				type="button"
				class="control badge maximize handle"
				aria-label="move the {title} pane's bar"
				title="drag to another edge to move the bar"
				on:pointerdown={grabBar}
				on:pointermove={dragBar}
				on:pointerup={dropBar}
				on:pointercancel={() => ((handle = null), (dropAt = null))}
				on:keydown={keyBar}
			>
				<!-- Four arrows out of the middle: this moves. -->
				<svg viewBox="0 0 16 16" aria-hidden="true">
					<path d="M8 1.5v13M1.5 8h13M8 1.5 6 3.5M8 1.5l2 2M8 14.5l-2-2M8 14.5l2-2M1.5 8l2-2M1.5 8l2 2M14.5 8l-2-2M14.5 8l-2 2" />
				</svg>
			</button>
			{/if}
			{#if many}
				<div
					class="tabs"
					role="tablist"
					aria-orientation={vertical ? 'vertical' : 'horizontal'}
					bind:this={strip}
					use:holds={{ within: '[role="tab"][aria-selected="true"]', run: changeWidget }}
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
				<div class="titlebox" use:holds={{ run: changeWidget }}>
					<h2 bind:this={h2}>{#if !$hosted}{title}{#if $modified.has(active.id)}<span class="dot" title="modified">•</span>{/if}{/if}</h2>
					{#if own.length > 0}
						<PaneMenu items={own} look="title" label="menu of the {title} pane" />
					{/if}
				</div>
			{/if}
			<!-- Inside `.along`, after the title or the last tab: the `+`
			     and the `×`. The DOM order is the drawn order and the
			     keyboard's both — handle, title/tabs, `+`, `×` — so it is
			     asserted rather than assumed (layout-maximize.test.mjs).

			     Layout mode's alone (da-vvcxc): outside it nothing can be
			     split, tabbed or closed, and these two are how it would be.
			     There is no `▾` any more — vividn, 2026-10-14: «no more down
			     arrow button at all» — what it held is the name's menu in
			     layout mode. -->
			{#if !locked}
				<button
					type="button"
					class="control badge add"
					use:act={'open'}
					aria-label="add a tab to the {title} pane"
					title="add a tab"
					on:click={() => void addChosen()}>+</button
				>
				<!-- And the × that closes what the title names (da-efr4i).
				     vividn, 2026-09-22: «in unlocked mode, add an x to the
				     widget titles that let's you close it.» The same act as
				     the menu's Close, wearing the same `×` (da-ygc9) — so a
				     frequently-wanted act has two routes, as `Change Widget`
				     now does — greyed exactly where that item is greyed, on
				     the root's own last tab, which is the layout itself.
				     LAST on the row: the one destructive badge, at the far
				     end from the `+`, the way Close is the far end of the
				     list. What a mis-tap costs is what `Close` costs and no
				     more: the DRAFT is what it writes, the save is untouched
				     and `Restore layout` is the way back, and the last pane
				     of a layout closes to an EMPTY pane whose one button is
				     `Change Widget` — never to nothing. -->
				<button
					type="button"
					class="control badge close"
					disabled={!canClose}
					aria-label="close the {title} {thing}"
					title="close this {thing}"
					on:click={() => apply((l) => closeTab(l, pane.id, pane.active))}>{CLOSE}</button
				>
			{/if}
		</div>
	</div>
	{/if}
	<!-- svelte-ignore a11y-no-static-element-interactions -->
	<div
		class="content"
		class:gliding
		class:moving={offset !== 0 || gliding || arriving}
		class:fading
		on:animationend={() => (fading = false)}
		bind:this={contentEl}
		style:--front="translate{vertical ? 'Y' : 'X'}({offset}px)"
		style:--beside="translate{vertical ? 'Y' : 'X'}(calc({offset}px {peek > pane.active ? '+' : '-'} (100% + {2 * shy}px)))"
		style:--shy="{shy}px"
		style:--slide="{SLIDE_MS}ms"
		on:pointerdown|capture={swipes ? (e) => (finger = e.pointerId) : undefined}
		on:touchstart={swipes ? onTouchStart : undefined}
		on:touchmove|capture|nonpassive={swipes ? (e) => onTouchMove(e, true) : undefined}
		on:touchmove|nonpassive={swipes ? onTouchMove : undefined}
		on:touchend={swipes ? onTouchEnd : undefined}
		on:touchcancel={swipes ? cancel : undefined}
	>
		<slot {peek} />
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
		/* TWO tokens, because a badge is two boxes (da-wyofs).

		   `--badge` is the box a POINTER lands on, and only that: the row's
		   height where the pointer is fine, a finger's 24px where it is
		   coarse (reachable.test.mjs). Nobody sees it. It is set on the
		   section from `badge` above, the one number Layout.svelte's floors
		   read as well (da-kpjn).

		   `--mark` is the box that is DRAWN — the title's ground and
		   outline, the maximize's box and glyph, the `+` and the `×`. One
		   value under either pointer, because a reader looking at a phone
		   and a reader looking at a screen are looking at the same app.
		   It is the row, so everything on the bar hangs half a row outside
		   its outline and the half-row gap between panes (Layout.svelte's
		   `BORDER`) is exactly enough to hold it clear of the neighbour.

		   They were ONE token until da-wyofs, so every ask that tuned the
		   drawing moved the hit area, and every ask that tuned the hit area
		   moved the drawing on the pointer nobody was looking at. */
		--mark: var(--row);
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
		/* The BOX A FINGER LANDS ON (da-wyofs). The button over it is
		   `inset: 0` of this, so the touch floor `reachable.test.mjs`
		   holds every pane control to lives here — and the h2 inside is
		   the smaller thing that is drawn, centred on the same line. */
		align-items: center;
		block-size: var(--badge);
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
		/* «too much padding on these widget titles» (vividn, 2026-09-22,
		   da-lihui) — 0.4rem until then. The font-size below is right and
		   does not move: what was too much was the room around the words,
		   not the words. The box does not GROW by this; it is the one
		   thing on the bar that can still give room back. */
		padding-inline: 0.25rem;
		/* The DRAWN height, one value under either pointer (da-wyofs).
		   It was `--badge` — the touch floor — until then, so a phone got
		   a 24px title with the same 12px words in it and the extra eight
		   pixels landed as padding: «too padded and wide on mobile and
		   overlap with other panes» (vividn, 2026-09-22). The title is still
		   a button (da-9vtw) and still on that floor; the floor is
		   `.titlebox` above, which this is centred in. */
		block-size: var(--mark);
		line-height: calc(var(--mark) - 2px);
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
	   rotated bar too — which is the order the Tab walk reads. */
	.add,
	.close,
	.maximize {
		flex: none;
		pointer-events: auto;
	}

	/* The title and the tabs are HELD, not read (da-efr4i): a long press on
	   one must not also select its words or raise the platform's text
	   callout. The fields widget says the same of its buttons, for the same
	   gesture. The pane's CONTENT is untouched — selecting and copying what
	   a pane shows is the whole reason the web re-renders at all. */
	.titlebox,
	.tabs {
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
	}

	/* «I want it to be fully in the corner so it is aligned with both the
	   horizontal and vertical borders» (vividn, 2026-09-22, da-lihui,
	   overruling da-fphjw's «push it into the corner more», which left it
	   2px short). Everything on this bar straddles the line the bar IS,
	   so the maximize already straddles the border it runs along. To
	   straddle the other one too its CENTRE must land on the corner
	   POINT — where the two straight edges would meet if the 6px radius
	   did not round them off, which is not a point on the arc.

	   The row's box starts at the pane's padding box, half a pixel inside
	   that point, and `.along` holds it 0.5rem further in. So the badge
	   gives back the 8px, that 0.5px, and half its own width. It alone:
	   `.along`'s padding still holds the `+` and the `×` off the curve at
	   the other end, and they still want that. Logical, so it is the same
	   corner whichever edge the bar is on and on the left-hand bar's half
	   turn too — the corner the alignment calls `start`. A holder's row
	   is not laid on an outline at all, so the rule is the pane's alone. */
	.pane > .titlebar > .along > .maximize {
		margin-inline-start: calc(-8.5px - var(--badge) / 2);
	}

	/* «reduce the distance between maximize and tabnames and then between
	   tabnames and the + and downarrow» (vividn, 2026-09-23, da-33ves).
	   `.along`'s gap is EVERY distance on the row, so the two he named are
	   taken in at their ends instead, and `+` and `×` keep theirs.
	   Two pixels, which still leaves two between the hit areas. */
	.along > .maximize {
		margin-inline-end: -2px;
	}
	.along > .add {
		margin-inline-start: -2px;
	}

	/* THE BUTTON READS SMALLER; THE HIT AREA DOES NOT. vividn, 2026-09-22
	   (da-lihui): «maximize button didn't actually seem to get smaller
	   […] it can still be a bit smaller than it is (reduce padding
	   some)». da-fphjw shrank the GLYPH and he saw no change, because the
	   box drawn around it never moved — so the box is what moves now.

	   The way the beta ⓘ did it: the drawn thing is small, the
	   thing a finger lands on is not. The BUTTON keeps `--badge` square —
	   the 16px/24px floor `reachable.test.mjs` holds every badge to — and
	   draws nothing of its own; the ground, the outline and the radius go
	   on a `::before` smaller than it, in the same grid cell as the
	   glyph, so the two are centred together and the mark paints over the
	   ground. Two pixels of padding a side where there were four (three
	   since da-33ves, below), and the glyph is untouched: `--badge` sizes the hit area and the mark, and
	   only the visible box is off it now.

	   `.control.badge`'s hover colours the BUTTON's border, which is not
	   the one on the screen any more, so the drawn edge follows a
	   property that all three states set.

	   The size of that drawn box came off `--badge` until da-wyofs, which
	   meant it was 12px under a cursor and 20px under a finger — the same
	   badge drawn twice, at two sizes, and only the first of them ever
	   measured. It comes off `--mark` now, so it is one size for everyone:
	   12px until «now the maximize button is too small, make a little
	   bigger» (vividn, 2026-09-23, da-33ves), and two pixels off `--mark`
	   since — 14px, which is what the `+` and the `×` beside it
	   paint (below). It was the one badge on the row drawn smaller.

	   THE OTHER BADGES ARE THE SAME CLAIM, so they are the same rule
	   (da-wyofs; the `×` joined them in da-efr4i). The `+` and the `×`
	   drew their ground and outline on the
	   BUTTON, which is `--badge` square: on a phone that put a 24px box on
	   a bar whose panes are eight pixels apart, and the badge of the pane
	   below lay 3.5px over the pane above — half of vividn's «overlap with
	   other panes», the title being the other half. They keep the finger's
	   box and draw inside it, like the maximize. Their box is `--mark`
	   whole rather than inset from it: that is the size they have always
	   drawn at under a cursor, and nothing about a cursor changes here.
	   `--mark` less two, as painted: the inset is from the button's
	   PADDING box, one transparent pixel inside its border. Absolutely placed instead of `grid-area`, because their glyph
	   is a character and an anonymous grid item cannot be sent to a named
	   cell; `isolation` makes the button the stacking context the `z-index`
	   is measured in, so the box lands under the glyph and over nothing
	   else. Svelte repeats its scope class over a bare class, which is
	   what lets these outweigh `.control.badge:hover:not(:disabled)` in
	   `app.css` and keep the button's own outline away under the pointer.
	   Asserted in layout-maximize.test.mjs.

	   ⚑ THE `×` WAS IN FLIGHT WHILE THIS LANDED and the two rebased past
	   each other with no conflict, which is how a badge comes to be on the
	   row and off this mechanism: it went on drawing on its BUTTON. Under
	   a cursor that is invisible, `--badge` and `--mark` being the same
	   16px there; under a finger it was a 24px drawn box between two 16px
	   ones, and one more of the bleeds da-wyofs removed. The tests read
	   every badge now, and read both pointers, which is what makes the
	   next one arrive noisily instead of quietly. */
	.maximize,
	.add,
	.close {
		--edge: var(--rule);
		background: none;
		border-color: transparent;
	}
	.maximize::before {
		content: '';
		grid-area: 1 / 1;
		inline-size: calc(var(--mark) - 2px);
		block-size: calc(var(--mark) - 2px);
		background: var(--ground);
		border: 1px solid var(--edge);
		border-radius: 5px;
	}
	.add,
	.close {
		position: relative;
		isolation: isolate;
	}
	.add::before,
	.close::before {
		content: '';
		position: absolute;
		z-index: -1;
		box-sizing: border-box;
		inset: calc((var(--badge) - var(--mark)) / 2);
		background: var(--ground);
		border: 1px solid var(--edge);
		border-radius: 6px;
	}
	.maximize:hover:not(:disabled),
	.maximize:focus-visible,
	.maximize[aria-pressed='true'],
	.add:hover:not(:disabled),
	.add:focus-visible,
	.close:hover:not(:disabled),
	.close:focus-visible {
		--edge: var(--accent);
	}

	/* LAYOUT MODE (da-vvcxc) is very visibly a mode: every pane's outline
	   is the accent and twice as heavy, the maximize is a HANDLE in the
	   accent (four arrows: this moves), and while it is held the edge a
	   drop would put the bar on is drawn. */
	.pane.editing {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent);
	}
	.handle {
		--edge: var(--accent);
		color: var(--accent);
		cursor: grab;
		touch-action: none;
	}
	.handle:active {
		cursor: grabbing;
	}
	.editing[data-bar-drop='top'] {
		box-shadow: 0 0 0 1px var(--accent), inset 0 4px 0 var(--accent-bright);
	}
	.editing[data-bar-drop='bottom'] {
		box-shadow: 0 0 0 1px var(--accent), inset 0 -4px 0 var(--accent-bright);
	}
	.editing[data-bar-drop='left'] {
		box-shadow: 0 0 0 1px var(--accent), inset 4px 0 0 var(--accent-bright);
	}
	.editing[data-bar-drop='right'] {
		box-shadow: 0 0 0 1px var(--accent), inset -4px 0 0 var(--accent-bright);
	}

	/* A flat mono-colour mark in `currentColor` (da-tnv3's rule for the
	   lock): strokes, so the two states are the same weight, and `block`
	   so a rotated bar does not give the box a baseline's slack.

	   The box is INSIDE the drawing, not the 16px the viewBox is: a mark is
	   `--mark` square with a 1px outline, so a 16px glyph on a 16px mark
	   hung a pixel out of each side (da-jlgp). FOUR pixels in from it —
	   three until «it can be a bit smaller as well» (vividn, 2026-09-21,
	   da-fphjw) — and square, so which way the bar runs makes no
	   difference to it. Off `--mark` and not `--badge` since da-wyofs: a
	   glyph a cursor saw at 8px was drawn at SIXTEEN under a finger, which
	   is the literal doubling vividn saw. The HIT AREA is not touched: the
	   mark is one size and the button stays the 16px/24px floor
	   `reachable.test.mjs` holds it to. The stroke does not scale with the
	   box — a hairline at 8px and a slab at 16px are the same mark drawn
	   twice — so it stays the 1.25px it was drawn as.

	   Its grid cell is named because the `::before` above names one: an
	   auto-placed item is put in the next FREE cell, so an unnamed glyph
	   would fall into a second row and neither would be centred. */
	.maximize svg {
		display: block;
		grid-area: 1 / 1;
		inline-size: calc(var(--mark) - 8px);
		block-size: calc(var(--mark) - 8px);
		fill: none;
		stroke: currentColor;
		stroke-width: 1.25;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.maximize svg path {
		vector-effect: non-scaling-stroke;
	}

	/* Filling the page is a state, so it is drawn as one — the ink here,
	   and the drawn edge in the `--edge` rule above. */
	.maximize[aria-pressed='true'] {
		color: var(--accent);
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
		block-size: var(--mark);
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

	/* The swipe's motion (da-kcmp, da-24fgb): the content box stays and
	   clips, and what moves in it is the strip — the tab in front, made a
	   box of the same size for the while (it is `display: contents` at
	   rest, TabBody.svelte), and its neighbour a box away. Under the finger
	   they follow it with no transition at all; the slide the release
	   finishes and the snap back of one too short are the same strip,
	   moved with one. Under `prefers-reduced-motion` nothing is ever
	   `moving` (the handlers), so the swap is the instant one it was. */
	.content.moving > :global(.tab-body:not(.behind)) {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		transform: var(--front);
	}
	/* `--beside`'s 100% is the neighbour's own box, which is the content's
	   padding box because `.behind` is `inset: 0` in it (TabBody.svelte),
	   less `--shy` either side: the box it lands in, where the root's in
	   front is another kind (da-466lv). 100% and both margins is one box
	   away, whatever the box. */
	.content.moving > :global(.tab-body.beside) {
		visibility: visible;
		margin: var(--shy);
		transform: var(--beside);
	}
	.content.gliding > :global(.tab-body:is(:not(.behind), .beside)) {
		transition: transform var(--slide) ease-out;
	}
	/* Under reduced motion a place arrives or leaves by a fade (da-6du3o). */
	.content.fading {
		animation: fade var(--slide) ease-out;
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
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
	/* The strip's frame: what hangs out of the box is not drawn. And a
	   transformed box widens the overflow of whatever scrolls it — the
	   root's content IS the page's scroller — so while it is moving,
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
	   draws none and moves nothing. `chromeOf` counts this.

	   `--badge` AND NOT `--mark` (da-wyofs, which nearly got this wrong).
	   Everywhere else on the bar the two panes are only neighbours and
	   what matters is what is drawn — a finger's four pixels of slack
	   either side of the mark are not on the screen, so they cannot lie
	   over anything. A strip is not a neighbour: it is an opaque row that
	   paints AND TAKES THE POINTER above what is under it (`z-index: 3`,
	   over the panes' 2). What slides beneath it is not merely hidden, it
	   is untappable — so what this has to keep clear is the box a finger
	   lands on. `layout-menu.test.mjs`'s two Split-in-a-tab-stack cases
	   are that claim, and they are read under a coarse pointer. */
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
	   are whole. The badge again, not the mark: a cut here takes the
	   OUTERMOST pixels of a badge, which under a finger are the hit area's
	   and not the drawing's, and a control at the page's edge is the one
	   that can least afford to lose them. */
	.holder.root.bare > .content {
		margin: calc(max(var(--row), var(--badge)) / -2);
		padding: calc(max(var(--row), var(--badge)) / 2);
	}
</style>
