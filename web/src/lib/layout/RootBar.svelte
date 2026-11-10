<!--
	The root tab bar, at the top of the screen, where it took the header's
	place (da-0a5o), with the gear and the help at its left (da-6no9e, which
	had it at the foot for a day). vividn, 2026-10-31: «let's move the tab
	bar back to the top, i don't like it at the bottom. But keep the gear
	and the help button on the left like they are now» (da-mt32t).

	It is the pane title bar, formatted as a bar of the page (da-wjob). vividn: it
	"is actually just a specially formatted version of the same thing —
	make sure it is really completely aligned with the other widget picker,
	just formatted differently". So the ACTIVE tab's name is the button of
	the same `PaneMenu` a pane's title opens, from the same builder
	(`menu.ts`); an inactive tab still just switches, and the arrow keys
	walk them. After them a `+` that opens the one chooser (da-uyx1) — the
	layouts, then the widget kinds, where a layout opens as a root tab and
	a widget opens that kind's built-in one-widget layout. Its first layout
	is «New layout» (da-1a9fh): named, then opened blank.

	MODIFY MODE (da-vvcxc, `mode.ts`) is entered HERE: a long press on the
	open tab let go where it was, its context menu — a right click, the
	menu key, Shift+F10 — or its menu's Modify. vividn, 2026-10-14: «long
	pressing (and release without dragging to reorder the tabs) or right
	clicking on a primary tab name enters unlocked/layout mode». It replaced
	the padlock that stood by the gear (da-fphjw) and the `▾` beside the `+`
	(da-bzhq). In it the bar is hatched in the accent, and the tab being
	modified wears ↺ on its left and ✓ on its right (da-00c29), vividn,
	2026-11-10: «don't show the [x] buttons on every tab / - instead show a
	check button on its right and a revert (circle arrow) button on it's
	left, which applies the changes or resets respectively. No buttons on
	other tabs, which should be dimmed.» So the other tabs, the `+` and the
	gear are dimmed and do nothing but be dragged along the strip, and the
	open tab's name opens what the `▾` held — `Change Widget`, `Split`,
	which at the root is `addEdge`, a new pane down one whole side of the
	layout (da-a642), and `Close tab`.

	In normal mode the name opens the tab's own menu (da-bfqw4, `tabItems`
	in menu.ts). vividn, 2026-11-10: «Choose another... / Modify (enters
	modification mode, like long pressing) / Save (appears onl if modified.
	if db is in a sync chain, then "Save & sync") / Reset (appears only if
	modified. Otherwise if there are previous saved versions, "Revert") /
	Factory default (only for prebuilt layouts and only appears if
	different) / Reorder <- ->, menu should stay open and in the same place
	so that you can quickly bump over a few / x Close», and of Choose
	another, «replace in place». So Choose another fills THIS tab from the
	chooser, as Change Widget does in layout mode, where the `+` opens a new
	one. While the tab is modified Save and Reset are marked, which is what
	the orange dot points at (vividn: "Maybe highlight Restore and Save so
	that it is obvious that that is what the orange dot is referring to").
	Reset goes back to the last save; Revert opens the save's versions
	under the bar, and `→ forward` undoes the last restore. Factory default
	(da-luwe's Reset layout) goes back to `defaults.ts` or the kit's tree
	instead, writing nothing. Reorder's arrows move the tab and leave the
	menu open where it was, under the finger.

	A root tab is one COPY of its layout and has no privilege over a copy
	nested in a pane (da-crp5): the same acts are in that pane's menu
	(PaneFrame.svelte), and both ask for them by the copy's TAB ID, which
	is what says which copy's tree is being saved — so this menu acts on
	the ROOT tab's layout however deep the layouts under it go.

	At the left end is a gear (da-pp1a): the GLOBAL settings menu, the
	root's alone, and the same `PaneMenu` as every other menu here. vividn, from the train: "Can we add a global
	settings menu at the top right (little gear icon)? ... Maybe we can put
	the version and hash string in that menu as well." That stamp — the
	version, a separator and the short sha, dim and disabled because it is
	a fact and not an act — is one of the things it holds. It was the
	bottom bar's until da-wjob and this bar's until da-pp1a; it is out of
	the way for good, and still one press from anyone looking for it. Under
	it is the word on the beta: that datum.day is early software, and
	where this page keeps your data (da-eue7y, below).

	First in it is «Backups» (da-9ow8m), the place of backup plans and
	backup files (layout/BackupsWidget.svelte), which it opens by
	`openPlace` (da-ea5q2): a modal on a desktop, a tab on a phone. It
	took the place of the root tab menu's `export backup…`, so a backup
	has one home. «Sync» beside it opens the Sync place the same way
	(da-s9r58), a door a newcomer can see, and «Settings» after it the
	Settings place (da-99g5q, layout/SettingsWidget.svelte), where the
	page's settings went: tracking the timezone and wrapping tabs.

	The other is «Reset default layouts» (da-jbcv). It held an act of that
	shape once — «Reset all layouts…», which deleted every saved layout, on
	every device once `_internal` synced, and cleared this device's drafts
	and locks — and it went (da-luwe) for doing more than its name said.
	vividn, 18:15: "Definitely don't want to delete *all* layouts, rather
	just reset the built-in layouts that ship with datum"; and 18:16,
	asking for it back: "Reset default layouts, should reset all built in
	layouts, as well as the tabs shown on the root. Other layouts are still
	accessible to be added with the + though, and are not reset." That is
	exactly what `resetDefaults` (shell.ts) does, behind a confirm, because
	it writes. The per-layout `Reset layout` is unchanged and is still the
	one that writes nothing.

	A PREVIEW tab (da-xamd) is drawn in ITALICS, and its menu has one item
	nobody else's has: «Keep tab», beside the Close that is the other
	answer to it. vividn: "it could be in italics until a user says keep tab
	after opening from a link" — so the slant is the whole of the state,
	and what it says is that nothing about this tab is written down yet.
	Keeping it saves the tree under its name and the italics go; closing it
	writes nothing at all.

	The gear wears a little `beta` banner (da-eue7y). vividn, 2026-10-14:
	"let's move the "beta" tag to just be a little banner on the gear
	button? Right now on mobile it takes up way too much space in the
	header bar. move the beta info message into the gear icon by the
	version number". It was a `beta` mark and an ⓘ before the gear
	(da-y992), whose note is now the gear menu's last item, under the
	version.

	Beside the gear, where the padlock was, a (?) (da-hibn3): the commands
	widget's Help button, so the page's next tap is explained on any page,
	one with no commands widget too. vividn, 2026-10-14: «replace it with a (?)
	button which functions like the help button, so it let's you query and
	learn more about what ever on the page, even when the command widget is
	not visible». Its `data-command` is the button's own: pressed again while
	armed, it is the general help. Layout mode has none: the mode's menus say
	what they do, and on a phone its tabs' × need the room (vremya on #764).
	Armed it reads «(click on something)», a pill, until a tap or Escape
	disarms it (da-okj8i), as the commands widget's Help does.
	It is filled with the accent, the palette's one color, so it is seen
	(da-ylkin), vividn, 2026-11-04: «Let's fill in the (?) button with the
	primary color so that it is a bit more visible».

	Left of the gear, an orange «↑ new» once a newer build of the
	page is deployed (da-ylkin, `#lib/upgrade.ts`): SvelteKit's `updated`,
	which reads `/_app/version.json` as the tab shows or takes focus, and
	hourly. vividn, 2026-11-04: «an orange colored button […] so that users
	are aware that they can upgrade», and 2026-11-05 (da-qob64): «The upgrade
	button in the header looks like for premium features and I think
	":uparrow: New version" will read better. Let's also move it to the left
	of the gear menu». A tap reloads at once, so it wears the double outline.
	It reads «↑ new», its full words its accessible name and title (da-jq1g1),
	vividn, 2026-11-06: «just be an arrow and "new" so that it doesnt take up
	quite so much space».

	At the right end, a ⚠ (da-8fucg) while a sync notice — a new device,
	a new admin, an unlock — is unacknowledged. vividn, 2026-10-09: "a little
	warning icon in the upper bar next to the gear and stuff. If you click
	on it it displays a message box saying that another device joined. ok
	to dismiss. tells user to go to sync to manage". The Sync widget's own
	`⚠ … [OK]` was the only place one showed, so a layout without that
	widget never showed it.

	On a phone the tabs scroll sideways rather than wrapping — one row,
	always — while the `+` and the gear stay where they are, and what is
	at the right end stays there.

	A tab DRAGGED along the strip moves there (da-w19ny, `reorder.ts`):
	picked up by a mouse that moves, or by a finger that holds, so a
	sideways finger still scrolls. The order is the tree's (`moveTab`), so
	it is written where the open list is and a reload keeps it.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { currentDb } from '#lib/database.ts';
	import { run } from '#lib/run.ts';
	import { pausedArgv } from '#lib/calls.ts';
	import { syncStates } from '#lib/sync.ts';
	import { ackNotice, expelDevice, noticeWords } from '#lib/syncView.ts';
	import { act } from '#lib/act.ts';
	import { storageSentence } from '#lib/storageState.ts';
	import { openBackups } from '#lib/backups.ts';
	import { ask, choose as chooseAnswer, confirm } from '#lib/confirm.ts';
	import { ARMED, arm, helping, helpsOf } from '#lib/help.ts';
	import { upgrade } from '#lib/upgrade.ts';
	import { updated } from '$app/state';
	import { choose, fillTab } from './chooser.js';
	import { apply, editing, layout, loaded } from './current.js';
	import { CHANGE, CLOSE, aimed, helpItems, tabItems, type Item } from './menu.js';
	import { APPLY, RESET, accept, discard, enter, onKey as onModeKey } from './mode.js';
	import { offers, widgetOptions } from './options.js';
	import { keyOf, session } from './session.js';
	import PaneMenu from './PaneMenu.svelte';
	import { ALIASES } from './defaults.js';
	import { labelOf } from './registry.js';
	import { activate, close, keepTab, modified, open, openBlank, openPlace, previews, resetDefaults, resetLayout, resetWidget, restore, save, saveAs, taken, unlikeShipped, versions } from './shell.js';
	import { reorders } from './reorder.js';
	import { addEdge, moveTab, setTall } from './tree.js';
	import type { BarEdge, Tab } from './types.js';
	import { isSplit } from './types.js';
	import type { Toward } from './tree.js';

	let busy = $state(false);
	/** How many saves each layout has behind it, by name, as last read (`countSaves`). */
	let saves: Record<string, number> = $state({});
	/** Reads of them under way, which keep the bar `aria-busy`: the menu's Revert waits on them. */
	let counting = $state(0);
	let error = $state('');
	/** The active save's versions, newest first, while the list is open. */
	let trail: { version: number; modifyTime: string }[] | null = $state(null);

	/** The root's `Split`: an arrow is a side of the whole layout (`addEdge`). */
	const SIDE: Record<Toward, BarEdge> = { left: 'left', right: 'right', up: 'top', down: 'bottom' };

	const tabs = $derived($layout.root.tabs);
	const active = $derived(tabs[$layout.root.active] as Tab | undefined);
	const name = $derived(active?.layout);
	/** The acts work on THIS COPY of the layout (da-crp5): its tab id, not its name. */
	const id = $derived(active?.id);
	const dirty = $derived(id !== undefined && $modified.has(id));
	/**
	 * A PREVIEW (da-xamd): seeded from a link and italic, and written
	 * NOWHERE yet — not this device's open list, not a draft, not the layout
	 * store — so it is gone on a reload. `Keep tab` below is what writes it.
	 */
	const preview = $derived(name !== undefined && $previews.has(name));
	/** When this database's sync was paused, while it is (da-wlbyx). */
	const paused = $derived($syncStates[currentDb()]?.paused);
	/** In a sync chain, so a save reaches the other devices: «Save & sync» (da-bfqw4). */
	const chained = $derived($syncStates[currentDb()]?.endpoint !== undefined);
	/** How far a sync session's apply is, while one runs (da-a7oa4). */
	const applying = $derived($syncStates[currentDb()]?.applying);
	/**
	 * The sync notices nobody has acknowledged (da-8fucg), of every database
	 * a tab is on: each one's sync leader reads them and says so to every
	 * tab, as it does `paused`. A database no tab is on has no endpoint
	 * running, so nothing joins it unseen, and its old notices are said
	 * again when it is opened.
	 */
	const noticed = $derived(Object.values($syncStates).flatMap((s) => (s.notices ?? []).map((n) => ({ db: s.db, n }))));
	/** The members a copy found on joining are not news (da-o9xdd): an ⓘ, unless something else is. */
	const alarming = $derived(noticed.some(({ n }) => n.kind !== 'member'));

	/**
	 * The widget's own options, inline (da-vhwq), for a root tab that IS a
	 * bare widget, a kind with no layout of its own (`widgetInto`): it has
	 * no pane, so this menu is the only door to them. A layout of one
	 * widget holds it in a pane, whose menu has them (da-7nh4l). The scope a session setting is held against is
	 * the layout this tab names, which is the outermost there is.
	 */
	const sessionOf = $derived(keyOf($layout, id ?? ''));
	const held = $derived($session[sessionOf] ?? {});
	const options = $derived(widgetOptions(active, held, sessionOf, $offers));
	/** Loaded, and no act in flight: what a test waits for before pressing. */
	const ready = $derived($loaded && !busy && counting === 0 && active !== undefined);

	/**
	 * The `+`: the chooser, and what it came back with. A layout opens as a
	 * root tab; a widget kind opens its built-in one-widget layout, by the
	 * name `widgetInto` reaches it by (`ALIASES` in defaults.ts: `collect`,
	 * the widget, is not `collect`, the page); «New layout» is named first.
	 * Nothing is above a root tab, so nothing is refused here.
	 */
	async function add(): Promise<void> {
		const picked = await choose([], true);
		if (!picked) return;
		if ('blank' in picked) return named();
		await attempt(() => open('layout' in picked ? picked.layout : (ALIASES[picked.kind] ?? picked.kind)));
	}

	/**
	 * «New layout» (da-1a9fh): asked for a name until it is one nothing
	 * answers to yet — a taken one is said and asked again — then opened
	 * blank. Cancel creates nothing.
	 */
	async function named(): Promise<void> {
		const question = 'Name the new layout:';
		let [said, last] = [question, ''];
		for (;;) {
			const to = await ask(said, 'create', last);
			if (to === null) return;
			if (!(await taken(to))) return attempt(() => openBlank(to));
			[said, last] = [`«${to}» is taken. ${question}`, to];
		}
	}

	async function attempt(job: () => Promise<unknown>): Promise<void> {
		busy = true;
		error = '';
		try {
			await job();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
		// What it did may have added a save, or moved the tab to a name that has some.
		void countSaves();
	}

	const doSave = () => attempt(async () => id !== undefined && save(id));

	/** Modified: back to the last save. Clean: the versions, to pick one. */
	const doRestore = () =>
		attempt(async () => {
			if (name === undefined || id === undefined) return;
			if (dirty) return restore(id);
			trail = trail ? null : await versions(name).catch(() => []);
		});

	const pick = (version?: number) =>
		attempt(async () => {
			if (id !== undefined) await restore(id, version);
			trail = null;
		});

	/** Back to the tree that shipped: the draft alone, so Restore is the way back (da-luwe). */
	const doReset = () => attempt(async () => id !== undefined && resetLayout(id));

	const doSaveAs = () =>
		attempt(async () => {
			if (id === undefined) return;
			const to = prompt('Save this layout as:', '')?.trim();
			if (!to) return;
			await saveAs(id, to);
		});

	/**
	 * How many saves the open layout has behind it: Revert is offered where
	 * there is one (da-bfqw4). A read is a CLI command, so it is made as the
	 * menu opens and after this bar's own acts, and never as a tab is
	 * switched to, which runs nothing (da-tajsm): a layout not read yet
	 * counts none until its menu opens.
	 */
	async function countSaves(): Promise<void> {
		const of = name;
		if (of === undefined) return;
		counting++;
		try {
			const trail = preview ? [] : await versions(of).catch(() => []);
			saves = { ...saves, [of]: trail.length };
		} finally {
			counting--;
		}
	}

	/**
	 * The layout's acts belong to the layout the active tab HOLDS — and a
	 * root tab need not hold one: a tab can be a bare widget, which the tree
	 * allows and a pane tab given a widget from the chooser becomes
	 * (da-8o07). At the root that same choice lands the widget's built-in
	 * layout instead, so it keeps its acts (da-y2f6).
	 */
	const holds = $derived(name !== undefined && id !== undefined);

	/**
	 * `Keep tab` (da-xamd): only a preview has it, because only a preview is
	 * not kept already. Keeping writes nothing into the tree.
	 */
	const keep = $derived(preview ? [{ label: 'Keep tab', run: () => void attempt(async () => id !== undefined && keepTab(id)) }] : []);

	/**
	 * What the open tab's name opens. In NORMAL MODE, what the tab is saved
	 * as: the layout's acts, and `Keep tab` on a preview — and on a tab that
	 * is a bare widget, that widget's reset and options first (da-ze1x).
	 *
	 * In MODIFY MODE (da-vvcxc), «all the down arrow actions» (vividn) — what
	 * the root `▾` held: `Change Widget`, `Split`, which at the root is a side
	 * of the whole layout (`addEdge`, da-a642), «Taller than the screen»
	 * (da-py6pl: the layout may grow past one screen and the page scrolls;
	 * off fits it back, each pane keeping its share), and `Close tab`, last.
	 * The ways out are the tab's ↺ and ✓ (da-00c29).
	 */
	const arrange = $derived([
		{ label: CHANGE, act: 'open' as const, run: () => id !== undefined && void fillTab(id) },
		{ label: 'Split', row: aimed('split: a new pane', (t) => apply((l) => addEdge(l, SIDE[t]))) },
		{
			label: 'Taller than the screen',
			toggled: active?.tall !== undefined,
			run: () => id !== undefined && apply((l) => setTall(l, id, active?.tall === undefined ? 1 : undefined))
		},
		{ label: 'Close tab', glyph: CLOSE, disabled: tabs.length < 2, run: () => close($layout.root.active) }
	] satisfies Item[]);

	const reset = $derived(resetWidget(active));
	/** A bare widget's tour and guide (da-7dflk), after its options, as a pane's menu has them. */
	const helps = $derived(active && !isSplit(active.content) ? helpItems(helpsOf(active.content.kind)) : []);
	/** Move the open tab one place, the menu staying open on it (da-bfqw4); none at that end. */
	const bump = (by: number) => {
		const [from, to] = [$layout.root.active, $layout.root.active + by];
		return to < 0 || to >= tabs.length ? undefined : () => apply((l) => moveTab(l, l.root.id, from, to));
	};
	const items = $derived(
		$editing
			? arrange
			: tabItems({
					choose: () => id !== undefined && void fillTab(id),
					modify: enter,
					widget: [...(reset ? [{ label: 'Reset widget', run: reset }] : []), ...options, ...helps],
					save: holds && dirty ? { run: doSave, sync: chained } : undefined,
					reset: holds && dirty ? doRestore : undefined,
					revert: holds && (saves[name ?? ''] ?? 0) > 0 ? doRestore : undefined,
					factory: holds && active !== undefined && unlikeShipped(active) ? doReset : undefined,
					saveAs: holds ? doSaveAs : undefined,
					extra: keep,
					left: bump(-1),
					right: bump(1),
					close: { disabled: tabs.length < 2, run: () => close($layout.root.active) }
				})
	);

	/** Another root tab; none in modify mode, whose other tabs are dimmed and still. */
	function go(index: number) {
		if (index === $layout.root.active || $editing) return;
		activate(index);
	}

	/** The tab's ✓, focused as the mode is entered, so Enter applies and a keyboard is where the way out is. */
	let applyButton: HTMLButtonElement | null = $state(null);
	$effect(() => {
		if ($editing) void tick().then(() => applyButton?.focus({ preventScroll: true }));
	});

	/**
	 * «Reset default layouts» (da-jbcv), behind a confirm: it writes, and
	 * what it writes over is the reader's own save of a name datum ships.
	 * Its narrow aim is the whole of what it says, because the act it
	 * replaces — «Reset all layouts…», which deleted every saved layout on
	 * every device — was withdrawn for being wider than its name (da-luwe).
	 */
	const doResetDefaults = () =>
		attempt(async () => {
			const yes = await confirm(
				'Every layout that ships with datum goes back to the way it comes, one you changed included, and the tabs at the top of the page go back to the ones a first visit opens.\n\n' +
					'Layouts you saved under a name of your own, and layouts a kit installed, are not touched — they stay in the +.',
				'reset'
			);
			if (yes) await resetDefaults();
		});

	/**
	 * The ⚠'s dialog (da-8fucg): each notice, where devices are managed, and
	 * an OK that acknowledges what it showed, as the Sync widget's OK does —
	 * and for each device that joined, an expel, should it not be the
	 * reader's (da-o9xdd), which acknowledges them too. Nothing is checked of
	 * an ack: one settles a device's rejoins with it, the next then has
	 * nothing to acknowledge, and whatever is left the leader says again, the
	 * ⚠ with it.
	 */
	const doNotices = () =>
		attempt(async () => {
			const shown = noticed;
			// One button a device, its admission and its rejoin alike; a label
			// two devices carry names its id as well.
			const byId = new Map<string, { db: string; label: string; endpoint: string }>();
			for (const { db, n } of shown)
				if ((n.kind === 'endpoint' || n.kind === 'rejoin') && n.endpoint) byId.set(`${db} ${n.endpoint}`, { db, label: n.label, endpoint: n.endpoint });
			const devices = [...byId.values()];
			const shared = (label: string) => devices.filter((d) => d.label === label).length > 1;
			const expels = devices.map((d) => `expel ${d.label}${shared(d.label) ? ` (${d.endpoint.slice(0, 8)})` : ''}`);
			const text =
				shown.map(({ db, n }) => noticeWords(db, n)).join('\n\n') +
				(devices.length ? '\n\nNot yours? Expel it, and it syncs no more.' : '') +
				'\n\nTo manage devices, open the Sync widget: the + in this bar, then sync. OK acknowledges this.';
			const picked = await chooseAnswer(text, ['OK', ...expels], 'not now', 'run');
			if (picked === null) return;
			const expelled = devices[expels.indexOf(picked)];
			if (expelled) await expelDevice(expelled.db, expelled.endpoint);
			for (const { db, n } of shown) await ackNotice(db, n);
		});

	/**
	 * The gear's menu: the page's own settings, which belong to no layout
	 * and no pane — the backups, sync and settings places, the act below, and which build is in front of you,
	 * disabled, because it is a fact and not an act, with the word on the
	 * beta under it (da-eue7y).
	 *
	 * It does not depend on which layout is in front, which is the whole of
	 * what makes this menu the page's and not a tab's: only the storage
	 * sentence moves it.
	 */
	const settings = $derived([
		{ label: 'Backups', act: 'open' as const, run: () => void openBackups() },
		{ label: 'Sync', act: 'open' as const, run: () => void openPlace('sync') },
		{ label: 'Settings', act: 'open' as const, run: () => void openPlace('settings') },
		{ label: 'Reset default layouts', act: 'open' as const, run: doResetDefaults },
		{ label: `${__DATUM_VERSION__} · ${__DATUM_COMMIT__}`, disabled: true },
		{ label: `datum.day is early software. ${$storageSentence}`, note: true as const }
	]);

	/** Arrow keys walk the tabs; the tab focused is the tab shown. */
	async function onTabKey(event: KeyboardEvent, index: number) {
		const next = event.key === 'ArrowRight' ? index + 1 : event.key === 'ArrowLeft' ? index - 1 : null;
		if (next === null || next < 0 || next >= tabs.length || $editing) return;
		event.preventDefault();
		go(next);
		// After the tick: switching swaps which tab is the menu's button, so
		// the tab to focus is the one that is there once it has (da-9vtw).
		await tick();
		strip?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
	}

	let strip: HTMLElement | null = $state(null);

	/**
	 * Whatever moved it — a swipe, a tap, a key, a script — the tab in front
	 * is scrolled to the strip's middle, so its neighbours show either side
	 * as far as they fit (da-i0hbs, vividn: «the tab bar at the top should
	 * scroll with the active tab … and also possible to see what the next
	 * and previous tabs are»). The browser clamps at the ends, and a strip
	 * whose tabs all fit has nowhere to go.
	 */
	$effect(() => {
		void [$layout.root.active, tabs.length, $loaded];
		void tick().then(() => {
			const tab = strip?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
			if (!strip || !tab) return;
			const [s, t] = [strip.getBoundingClientRect(), tab.getBoundingClientRect()];
			const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
			strip.scrollTo({ left: strip.scrollLeft + t.left - s.left - (s.width - t.width) / 2, behavior: still ? 'instant' : 'smooth' });
		});
	});

	/** A tab's marks: the dot's class, and the italics of a preview. */
	const marks = (tab: Tab, dots: Set<string>, shown: Set<string>) =>
		[dots.has(tab.id) ? 'modified' : '', tab.layout !== undefined && shown.has(tab.layout) ? 'preview' : '']
			.filter(Boolean)
			.join(' ');

	/** Pressed «↑ new», waiting for the CLI to be idle to reload; false again if it failed. */
	let upgrading = $state(false);

	/** The gear's eight teeth, by the angle each is turned to. */
	const TEETH = [0, 45, 90, 135, 180, 225, 270, 315];
</script>

<svelte:window onkeydown={onModeKey} />

<nav class="root-bar" class:modified={dirty} aria-busy={!ready} aria-label="layouts" data-layout-mode={$editing ? '' : undefined}>
	{#if !$editing && updated.current}
	<button
		type="button"
		class="control upgrade"
		use:act={'run'}
		disabled={upgrading}
		aria-label="New version: reload to update"
		title="New version: reload to update"
		data-help="A newer datum.day is out. Loads it, past every cache, once nothing is running or syncing; your data stays as it is."
		onclick={() => {
			upgrading = true;
			upgrade().catch(() => (upgrading = false));
		}}>↑ new</button
	>
	{/if}
	<PaneMenu
		items={settings}
		look="badge"
		label="settings"
		title="settings"
		data-help="Settings for this whole page: Backups, where backups are planned, made and restored; Sync, this page's devices and how each stands; Settings, where timezone tracking and wrapping tabs on a swipe are set; Reset default layouts, which puts back the layouts datum ships with; and the version this is, with where this page keeps your data — datum.day is in beta."
		class="gear"
		aria-label="settings"
		disabled={!!$editing}
		><svg class="cog" viewBox="-12 -12 24 24" width="20" height="20" aria-hidden="true">
			{#each TEETH as angle}<rect x="-1.9" y="-11.4" width="3.8" height="5.6" rx="1" transform="rotate({angle})" />{/each}
			<circle class="ring" r="6.2" />
		</svg><span class="beta" aria-hidden="true">beta</span></PaneMenu
	>
	{#if !$editing}
	<button
		type="button"
		class="control badge help"
		class:armed={$helping}
		data-command="help"
		aria-pressed={$helping}
		aria-label="help"
		title={$helping ? 'tap anything to have it explained; Help again for general help' : 'explain the next thing you tap'}
		onclick={arm}>{$helping ? ARMED : '?'}</button
	>
	{/if}
	<!-- A hold on the OPEN tab, let go where it was, or its context menu —
	     a right click, the menu key, Shift+F10 — enters modify mode
	     (da-vvcxc); a hold that moves still reorders (`reorder.ts`). -->
	<div
		class="tabs"
		role="tablist"
		aria-orientation="horizontal"
		bind:this={strip}
		use:reorders={{
			tabs: '[role="tab"]',
			move: (from, to) => apply((l) => moveTab(l, l.root.id, from, to)),
			hold: (tab) => tab.getAttribute('aria-selected') === 'true' && (enter(), true)
		}}
	>
		{#each $loaded ? tabs : [] as tab, i (tab.id)}
			<!-- The tab being modified wears ↺ on its left and ✓ on its right
			     (da-00c29); the others none. -->
			{#if $editing && i === $layout.root.active}
				<button type="button" class="control way-out reset" aria-label="reset the changes" title="reset the changes (Escape)" onclick={discard}
					>{RESET}</button
				>
			{/if}
			{#if i === $layout.root.active && items.length > 0}
				<!-- The tab in front is the menu's button; the others switch. -->
				<PaneMenu
					{items}
					onshow={countSaves}
					look="tab"
					label="menu of the {labelOf(tab)} layout"
					title="layout menu"
					data-help="The layout in front: choose another in its place, modify it, save it, reset it to its last save or revert to an earlier one, put it back to how it shipped, move it along the tabs, or close it. Long press or right-click it to modify it too, where its panes are split, moved, resized and closed."
					role="tab"
					aria-selected="true"
					tabindex="0"
					class={marks(tab, $modified, $previews)}
					onkeydown={(e: KeyboardEvent) => onTabKey(e, i)}
					>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</PaneMenu
				>
			{:else}
				<button
					type="button"
					role="tab"
					class="control tab"
					class:modified={$modified.has(tab.id)}
					class:preview={tab.layout !== undefined && $previews.has(tab.layout)}
					aria-disabled={$editing ? 'true' : undefined}
					aria-selected={i === $layout.root.active}
					tabindex={i === $layout.root.active ? 0 : -1}
					onclick={() => go(i)}
					onkeydown={(e) => onTabKey(e, i)}
				>
					{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}
				</button>
			{/if}
			{#if $editing && i === $layout.root.active}
				<button
					type="button"
					class="control way-out apply"
					aria-label="apply the changes"
					title="apply the changes (Enter): kept on this device, Save in the tab's menu writes them"
					bind:this={applyButton}
					onclick={accept}>{APPLY}</button
				>
			{/if}
		{/each}
	</div>
	<button type="button" class="control badge add" use:act={'open'} aria-label="open a layout" title="open a layout" disabled={!!$editing}
		data-help="A layout is a page of panes, each holding a widget. Opens one as a new tab — a saved layout, a new blank one you name, or a single widget on its own; the tabs switch between them."
		onclick={add}>+</button
	>
	<span class="end">
	{#if error}<span class="error" role="alert">{error}</span>{/if}
	<!-- Sync paused (da-wlbyx) is loud: vividn calls it risky. Every tab hears
	     it from the database's sync leader, and a tap resumes. -->
	{#if paused}
		<button type="button" class="control paused" title="sync paused since {paused} — tap to resume" onclick={() => run(pausedArgv(false))}
			>sync paused</button
		>
	{/if}
	<!-- Any session's apply, not just a join's (da-a7oa4): each step holds the
	     database, so a command typed meanwhile waits, and this says why. -->
	{#if applying}
		<span class="syncing" role="status" title="a command typed meanwhile waits for the step under way"
			>syncing {applying.done.toLocaleString()} of {applying.of.toLocaleString()}</span
		>
	{/if}
	{#if noticed.length}
		<button
			type="button"
			class="control notices"
			use:act={'open'}
			aria-label="sync notices"
			title="a device joined, or the sync chain changed: tap to read"
			data-help="Something changed in a sync chain: a device joined, an admin was added, or the chain was unlocked. Tap to read it and acknowledge it."
			onclick={doNotices}>{alarming ? '⚠' : 'ⓘ'}</button
		>
	{/if}
	</span>
	{#if trail && !dirty}
		<ul class="versions">
			{#if trail.length}
				<li><button type="button" onclick={() => pick()}>→ forward</button></li>
			{:else}
				<li>not saved yet</li>
			{/if}
			{#each trail as v (v.version)}
				<li>
					<button type="button" onclick={() => pick(v.version)}
						>v{v.version} · {new Date(v.modifyTime).toLocaleString()}</button
					>
				</li>
			{/each}
		</ul>
	{/if}
</nav>

<style>
	.root-bar {
		position: relative;
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
		padding: 0.3rem 0.5rem;
		background: var(--ground);
		border-bottom: 1px solid var(--rule);
		min-width: 0;
	}

	/* One row, always: the strip scrolls sideways on a phone, and the bar
	   itself clips nothing — the versions list hangs below it. */
	.tabs {
		display: flex;
		gap: 4px;
		flex: 0 1 auto;
		min-width: 0;
		overflow-x: auto;
		overflow-y: hidden;
		scrollbar-width: none;
	}

	/* `.control` (app.css) is the pill; a root tab is a wide one, and a
	   fingertip's 24px tall. The pane frames' tabs are the same shape a
	   size down (PaneFrame.svelte). The active one is a PaneMenu's button,
	   which carries `.control.tab` from its `look` and this `.tab` from the
	   class passed through — so both are the same pill. */
	.tabs :global(.tab) {
		block-size: 24px;
		padding: 0 0.6rem;
		font-size: 0.8rem;
		letter-spacing: 0.06em;
		white-space: nowrap;
		flex: none;
	}

	/* The tab in front says so with its outline — which is why the hover
	   below lives here and not on `.control`: it has to be able to argue
	   with this rule, and it can only do that from inside the component. */
	.tabs :global(.tab[aria-selected='true']) {
		color: var(--accent);
		border-color: var(--accent);
	}

	/* A tab being dragged (da-w19ny) rides over the others, and the tab it
	   would land beside wears an accent line on that side. Inset, because
	   the strip clips: the first tab's outer edge is the strip's. */
	.tabs {
		-webkit-user-select: none;
		user-select: none;
		-webkit-touch-callout: none;
	}

	.tabs :global([data-dragging]) {
		position: relative;
		z-index: 1;
		cursor: grabbing;
	}

	.tabs :global([data-drop='before']) {
		box-shadow: inset 2px 0 0 var(--accent);
	}

	.tabs :global([data-drop='after']) {
		box-shadow: inset -2px 0 0 var(--accent);
	}

	.tabs :global(.tab:hover),
	.tabs :global(.tab:focus-visible) {
		color: var(--accent-bright);
	}

	/* The `+` is a badge, and does not stretch. */
	.add {
		flex: none;
	}

	/* LAYOUT MODE (da-vvcxc), very visibly: the bar is hatched and ruled in
	   the accent, as the page under it is (routes/+page.svelte), and each
	   tab wears its close on its right, tucked against it. The rule is
	   thickened by a shadow, not a wider border: entering the mode moves
	   nothing on the page. */
	.root-bar[data-layout-mode] {
		background: repeating-linear-gradient(
			-45deg,
			color-mix(in srgb, var(--accent) 16%, var(--ground)) 0 6px,
			var(--ground) 6px 14px
		);
		border-bottom-color: var(--accent);
		box-shadow: 0 1px 0 var(--accent);
	}

	/* What modify mode cannot change is dimmed (da-00c29), vividn: «in
	   general things should be dimmed except for the things you can change
	   which can be highlighted»: the other tabs, the `+` and the gear. */
	.root-bar[data-layout-mode] .tabs :global(.tab[aria-selected='false']),
	.root-bar[data-layout-mode] .add,
	.root-bar[data-layout-mode] :global(.gear) {
		opacity: 0.4;
	}

	/* The modified tab's ↺ and ✓, tucked against it either side. */
	.way-out {
		flex: none;
		block-size: 24px;
		min-inline-size: 24px;
		padding: 0 0.3rem;
		color: var(--accent);
		border-color: var(--accent);
	}

	.way-out.reset {
		margin-inline-end: -2px;
	}

	.way-out.apply {
		margin-inline-start: -2px;
		background: var(--accent);
		color: var(--page);
		font-weight: 700;
	}

	.dot {
		margin-left: 0.3rem;
		color: var(--warn);
	}

	/* A PREVIEW is italic (da-xamd) — vividn: "it could be in italics until a
	   user says keep tab after opening from a link", the way an editor
	   draws a file it is only showing you. The tab is a tab in every other
	   respect; what the slant says is that nothing about it is written
	   down, and `Keep tab` in its menu is what writes it. */
	.tabs :global(.preview) {
		font-style: italic;
	}

	/* The gear: at the bar's left end at every width, and a badge like the
	   `+` — so it is on its 24px floor under a fingertip too
	   (reachable.test.mjs).

	   A BIGGER badge than the rest of the bar's, and a drawn cog rather
	   than the `⚙` character (da-jbcv). vividn: "Make the gear icon much
	   bigger, it's almost invisible." The character was the trouble: at
	   the bar's font size its ink is a 8x16px scratch, and how much of
	   the em it fills is the reader's font's business and not ours. A
	   path fills the box it is given, on every device, and the box is
	   this one. */
	/* The bar's right end: what sync and an act have to say. `auto` here rather than on
	   the tabs, which scroll, so it is carried to the end. */
	.end {
		display: flex;
		align-items: center;
		gap: 4px;
		margin-left: auto;
		flex: none;
	}

	/* The (?) is round and filled with the accent (da-ylkin), its glyph the
	   page's ground on it, hovered too. Armed it is the brighter accent, with
	   a ring of it round the bar's ground, and a pill wide as its words. */
	.root-bar .help,
	.root-bar .help:hover:not(:disabled),
	.root-bar .help:focus-visible {
		flex: none;
		border-radius: 999px;
		background: var(--accent);
		border-color: var(--accent);
		color: var(--page);
		font-weight: 700;
	}

	.root-bar .help.armed {
		background: var(--accent-bright);
		border-color: var(--accent-bright);
		box-shadow: 0 0 0 2px var(--ground), 0 0 0 3px var(--accent-bright);
		inline-size: auto;
		padding-inline: 0.5rem;
		white-space: nowrap;
	}

	/* «↑ new» (da-ylkin, da-qob64, da-jq1g1): orange, vividn's color for it, which is the
	   palette's `--alarm`; filled, and its double outline drawn round the
	   fill with the bar's ground between the two lines. */
	.upgrade {
		flex: none;
		block-size: 24px;
		padding: 0 0.4rem;
		background: var(--alarm) padding-box;
		color: var(--page);
		font-weight: 700;
		white-space: nowrap;
	}

	/* `:global`, as every `[data-act]` rule: `use:act` writes the attribute,
	   which Svelte cannot see, so it would drop the rule as unused. */
	.upgrade:global([data-act='run']) {
		border: 3px double var(--alarm);
	}

	.upgrade:disabled {
		cursor: progress;
		opacity: 0.6;
	}

	/* The ⚠ (da-8fucg) is a pill and not a badge: it opens a dialog, and so
	   wears the `›` (#lib/act.ts). */
	.notices {
		flex: none;
		block-size: 24px;
		padding: 0 0.4rem;
		color: var(--warn);
		border-color: var(--warn);
		white-space: nowrap;
	}

	.root-bar :global(.gear) {
		position: relative;
		flex: none;
		--badge: 28px;
	}

	/* The little banner across the gear's foot (da-eue7y), in the amber of
	   its own the `beta` mark always wore: it says the software is early,
	   which is no colour of the palette's — not `--warn`, which is unsaved
	   work, nor the database's hue. Taps go through it to the gear. */
	.beta {
		position: absolute;
		bottom: -4px;
		left: 50%;
		transform: translateX(-50%);
		padding: 1px 2px;
		border-radius: 2px;
		background: #f0b429;
		color: var(--page);
		font-size: 0.5rem;
		line-height: 1;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		pointer-events: none;
	}

	.cog {
		fill: currentColor;
	}

	.cog .ring {
		fill: none;
		stroke: currentColor;
		stroke-width: 3.6;
	}

	.versions {
		position: absolute;
		top: 100%;
		right: 0;
		z-index: 10;
		margin: 0.3rem 0 0;
		max-block-size: calc(100cqh - 3rem);
		overflow-y: auto;
		padding: 0.3rem 0.6rem;
		list-style: none;
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 6px;
		white-space: nowrap;
		font-size: 0.85rem;
		color: var(--muted);
	}

	.versions button {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: var(--accent);
		cursor: pointer;
	}

	.versions button:hover {
		color: var(--accent-bright);
	}

	.paused {
		padding: 0 0.5rem;
		color: var(--page);
		background: var(--error);
		border-color: var(--error);
		font-weight: 600;
		white-space: nowrap;
	}

	.syncing {
		font-size: 0.75rem;
		color: var(--accent);
		white-space: nowrap;
	}

	.error {
		font-size: 0.75rem;
		/* Not `--error`: a lighter red, because this one is a line of words
		   on the bar rather than a mark on a control. */
		color: #ff7b7b;
		white-space: nowrap;
	}
</style>
