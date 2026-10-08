<!--
	The root tab bar — the page's top bar, in place of the header (da-0a5o).

	It is the pane title bar, formatted as a top bar (da-wjob). vividn: it
	"is actually just a specially formatted version of the same thing —
	make sure it is really completely aligned with the other widget picker,
	just formatted differently". So the ACTIVE tab's name is the button of
	the same `PaneMenu` a pane's title opens, holding the same items from
	the same builder (`menu.ts`); an inactive tab still just switches, and
	the arrow keys walk them. After them a `+` that opens the one chooser
	(da-uyx1) — the layouts, then the widget kinds, where a layout opens as
	a root tab and a widget opens that kind's built-in one-widget layout —
	and beside the `+` a `▾` holding what is DONE to the page: `Split` and
	`Close tab` (da-bzhq). That is the same slicing a pane's bar makes of
	the same list (`named` and `managed`, menu.ts): the NAME opens what the
	tab shows and what it is saved as, the `▾` what is done to it.

	Unlike a pane's, this `+` does NOT go under the lock: it opens ANOTHER
	layout beside the locked one and never touches its tree, and it is how
	a reader leaves a page they have locked. The `▾` goes anyway, because a
	locked tab's list is its acts alone and none of them is a management
	act — the same answer, by the rule rather than by a second condition.

	What the root's menu is missing, and why, is `menu.ts`'s comment: no
	`Break out` (a tab breaks out of its STRIP, and the root has no strip
	to leave) and no `Bar` (the root draws no bar of its own — this one is
	the shell's). Its `Split` is `addEdge`: a new pane down one whole side
	of the layout — what the page's four `…` edge handles used to do
	(da-qk2k) — which is what a split beside the shell can only mean
	(da-a642). What it has that a pane's has not is `Keep tab`, on a
	preview (below).

	The layout's own acts — save, restore, save as — are in the menu too,
	where the orange dot is (vividn: "Inside the menu there could be a lock,
	save, and save as options. That seems more consistent. Maybe highlight
	Restore and Save so that it is obvious that that is what the orange dot
	is referring to"). They were a row of icons at the right end of this
	bar until da-wjob; the row is gone, no shortcut kept. While the tab is
	modified Save and Restore are marked in the menu and the tab keeps its
	dot, so the dot's meaning is one press from the dot. Restore goes back
	to the last save; with nothing to discard it opens the save's versions
	under the bar instead, and `→ forward` undoes the last restore. A tab
	holding a layout that SHIPPED with datum has another, `Reset layout`
	(da-luwe), beside Restore: the other way back, to `defaults.ts` rather
	than to the save.

	THE LOCK IS NOT AMONG THEM any more (da-bzhq). vividn, 2026-09-21: "Let's
	move the lock button outside of the layout menu and put it by the gear
	icon to be more discoverable." So it is a padlock BUTTON beside the
	gear — on the gear's LEFT, between it and the beta pair's ⓘ, since
	"move the lock button to the other side of the gear" (da-fphjw) gave
	the bar's far end back to the gear. Pressed rather than chosen,
	`aria-pressed` for which way it is; HIGHLIGHTED when it is UNLOCKED,
	that being the state with consequences (da-fphjw, see `.lock.open`).
	It is this device's (shell.ts), not the layout's, which is why its
	title says so in words — and which layouts open locked is untouched
	(da-gshh). A PANE's menu keeps `Lock layout` / `Unlock
	layout`, there being no gear beside a pane and a locked layout tab
	needing a way out (da-crp5).

	A root tab is one COPY of its layout and has no privilege over a copy
	nested in a pane (da-crp5): the same acts are in that pane's menu
	(PaneFrame.svelte), and both ask for them by the copy's TAB ID, which
	is what says which copy's tree is being saved — so this menu acts on
	the ROOT tab's layout however deep the layouts under it go.

	At the right end is a gear (da-pp1a): the GLOBAL settings menu, the
	root's alone, and the same `PaneMenu` as every other menu here. vividn, from the train: "Can we add a global
	settings menu at the top right (little gear icon)? ... Maybe we can put
	the version and hash string in that menu as well." That stamp — the
	version, a separator and the short sha, dim and disabled because it is
	a fact and not an act — is one of the two things it holds. It was the
	bottom bar's until da-wjob and this bar's until da-pp1a; it is out of
	the way for good, and still one press from anyone looking for it.

	First in it is «Backups» (da-9ow8m), the place of backup plans and
	backup files (layout/BackupsWidget.svelte), which it opens by
	`openPlace` (da-ea5q2): a modal on a desktop, a tab on a phone. It
	took the place of the root tab menu's `export backup…`, so a backup
	has one home.

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

	Just before the lock, the `beta` mark and its ⓘ (Beta.svelte): vividn on
	PR #25, "move the beta marker up to the top bar near the gear logo"
	(da-y992). They were under the logo on the bottom bar until then. So
	the bar's right end reads ⓘ · lock · gear (da-fphjw).

	On a phone the tabs scroll sideways rather than wrapping — one row,
	always — while the `+`, the `▾`, the beta pair, the lock and the gear
	stay where they are.

	A tab DRAGGED along the strip moves there (da-w19ny, `reorder.ts`):
	picked up by a mouse that moves, or by a finger that holds, so a
	sideways finger still scrolls. The order is the tree's (`moveTab`), so
	it is written where the open list is and a reload keeps it.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { currentDb } from '$lib/database.js';
	import { run } from '$lib/run.js';
	import { syncStates } from '$lib/sync.js';
	import Beta from '$lib/Beta.svelte';
	import { act } from '$lib/act.js';
	import { openBackups } from '$lib/backups.js';
	import { confirm } from '$lib/confirm.js';
	import { choose, fillTab } from './chooser.js';
	import { apply, layout, loaded } from './current.js';
	import { layoutActs, managed, menuItems, named } from './menu.js';
	import { widgetOptions } from './options.js';
	import { keyOf, session } from './session.js';
	import PaneMenu from './PaneMenu.svelte';
	import { labelOf, titleOf } from './registry.js';
	import { activate, close, keepTab, modified, open, previews, resetDefaults, resetLayout, resetWidget, restore, save, saveAs, shippedTree, toggleLock, versions } from './shell.js';
	import { reorders } from './reorder.js';
	import { addEdge, moveTab } from './tree.js';
	import type { BarEdge, Tab } from './types.js';
	import type { Toward } from './tree.js';

	let busy = false;
	let error = '';
	/** The active save's versions, newest first, while the list is open. */
	let trail: { version: number; modifyTime: string }[] | null = null;

	/** The root's `Split`: an arrow is a side of the whole layout (`addEdge`). */
	const SIDE: Record<Toward, BarEdge> = { left: 'left', right: 'right', up: 'top', down: 'bottom' };

	$: tabs = $layout.root.tabs;
	$: active = tabs[$layout.root.active] as Tab | undefined;
	$: name = active?.layout;
	/** The acts work on THIS COPY of the layout (da-crp5): its tab id, not its name. */
	$: id = active?.id;
	$: dirty = id !== undefined && $modified.has(id);
	/**
	 * A PREVIEW (da-xamd): seeded from a link and italic, and written
	 * NOWHERE yet — not this device's open list, not a draft, not the layout
	 * store — so it is gone on a reload. `Keep tab` below is what writes it.
	 */
	$: preview = name !== undefined && $previews.has(name);
	$: locked = !!active?.locked;
	/** When this database's sync was paused, while it is (da-wlbyx). */
	$: paused = $syncStates[currentDb()]?.paused;
	/** How far a sync session's apply is, while one runs (da-a7oa4). */
	$: applying = $syncStates[currentDb()]?.applying;

	/**
	 * The widget's own options, inline (da-vhwq), for a root tab that SHOWS
	 * ONE widget — on a phone that tab is the whole page, so this menu is
	 * the only door to them. The scope a session setting is held against is
	 * the layout this tab names, which is the outermost there is.
	 */
	$: sessionOf = keyOf($layout, id ?? '');
	$: held = $session[sessionOf] ?? {};
	$: options = widgetOptions(active, held, sessionOf, locked);
	/** Loaded, and no act in flight: what a test waits for before pressing. */
	$: ready = $loaded && !busy && active !== undefined;

	/**
	 * The `+`: the chooser, and what it came back with. A layout opens as a
	 * root tab; a widget kind opens its built-in one-widget layout, whose
	 * name is the kind's title (defaults.ts). Nothing is above a root tab,
	 * so nothing is refused here.
	 */
	async function add(): Promise<void> {
		const picked = await choose();
		if (picked) await attempt(() => open('layout' in picked ? picked.layout : titleOf(picked.kind)));
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
	 * The acts belong to the layout the active tab HOLDS — and a root
	 * tab need not hold one: a tab can be a bare widget, which the tree
	 * allows and a pane tab given a widget from the chooser becomes
	 * (da-8o07). At the root that same choice lands the widget's built-in
	 * layout instead, so it keeps its acts (da-y2f6). The same rule a
	 * pane's frame reads.
	 */
	$: acts =
		name === undefined
			? undefined
			: {
					dirty,
					locked,
					save: doSave,
					restore: doRestore,
					// Only a layout that HAS a shipped tree can be sent back to
					// one, so a layout the reader saved themselves has no act
					// here at all (da-luwe).
					reset: shippedTree(name) ? doReset : undefined,
					saveAs: doSaveAs
					// No `toggleLock`: the lock is a BUTTON beside the gear now
					// (da-bzhq), not an item of this menu. See `doToggleLock`.
				};

	/**
	 * The lock, beside the gear (da-bzhq). vividn, 2026-09-21: "Let's move
	 * the lock button outside of the layout menu and put it by the gear
	 * icon to be more discoverable."
	 *
	 * It is THIS DEVICE'S (shell.ts), not the layout's — which is what the
	 * button's title says in so many words, so that a control sitting in
	 * the page's own corner is not read as a property of the layout
	 * everyone shares. What it locks is unchanged, and so is which layouts
	 * open locked (da-gshh): only where the press is.
	 */
	const doToggleLock = () => id !== undefined && toggleLock(id);

	/**
	 * `Keep tab` (da-xamd), next to the Close that is the other answer to a
	 * preview: only a preview has it, because only a preview is not kept
	 * already. A PLACE (da-6du3o) is one, and opens locked, being shipped —
	 * so a lock does not take it away: keeping writes nothing into the tree.
	 */
	$: keep = preview ? [{ label: 'Keep tab', run: () => void attempt(async () => id !== undefined && keepTab(id)) }] : [];

	$: all = locked
		? [...options, ...(acts ? layoutActs(acts) : []), ...keep]
		: menuItems({
				choose: () => id !== undefined && void fillTab(id),
				// The same rule a pane's frame reads, and it is about SHAPE: a
				// root tab showing ONE widget — a bare one, or the one-widget
				// layout `/repl` and `/nowview` open (da-ze1x) — offers that
				// widget's acts beside the layout's own. On a phone that tab is
				// the whole page, so this menu is the only door to it. A layout
				// of several widgets shows a split here and offers neither.
				reset: resetWidget(active),
				options,
				// No `breakOut`: a tab breaks out of its strip, and this bar is the
				// shell, not a strip (da-a642, da-n632). No `bar`: this bar IS the root's.
				split: { what: 'split: a new pane', run: (t) => apply((l) => addEdge(l, SIDE[t])) },
				close: { label: 'Close tab', disabled: tabs.length < 2, run: () => close($layout.root.active) },
				acts,
				extra: keep
			});

	/**
	 * The two slices (da-bzhq, `menu.ts`): the layout's NAME opens what
	 * this tab shows and what it is saved as; the `▾` beside the `+` opens
	 * what is done to the page — its `Split`, which is `addEdge`, and
	 * `Close tab`. A LOCKED root tab's list is the acts alone, none of
	 * them a management act, so the `▾` is not drawn at all.
	 *
	 * The `+` is NOT hidden under a lock, unlike a pane's (PaneFrame): it
	 * opens ANOTHER layout beside this one and never touches the locked
	 * tree, and it is how a reader leaves a page they have locked.
	 */
	$: items = named(all);
	$: manage = managed(all);

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
				'Every layout that ships with datum goes back to the way it came, and the tabs at the top of the page go back to just “day”.\n\n' +
					'Layouts you saved yourself, and layouts a kit installed, are not touched — they stay in the +.',
				'reset'
			);
			if (yes) await resetDefaults();
		});

	/**
	 * The gear's menu: the page's own settings, which belong to no layout
	 * and no pane — the backups, the act above, and which build is in front of you,
	 * disabled, because it is a fact and not an act.
	 *
	 * A constant: it does not depend on which layout is in front, which is
	 * the whole of what makes this menu the page's and not a tab's.
	 */
	const settings = [
		{ label: 'Backups', act: 'open' as const, run: () => void openBackups() },
		{ label: 'Reset default layouts', act: 'open' as const, run: doResetDefaults },
		{ label: `${__DATUM_VERSION__} · ${__DATUM_COMMIT__}`, disabled: true }
	];

	/** Arrow keys walk the tabs; the tab focused is the tab shown. */
	async function onTabKey(event: KeyboardEvent, index: number) {
		const next = event.key === 'ArrowRight' ? index + 1 : event.key === 'ArrowLeft' ? index - 1 : null;
		if (next === null || next < 0 || next >= tabs.length) return;
		event.preventDefault();
		activate(next);
		// After the tick: switching swaps which tab is the menu's button, so
		// the tab to focus is the one that is there once it has (da-9vtw).
		await tick();
		strip?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
	}

	let strip: HTMLElement | null = null;

	/** A tab's marks: the dot's class, and the italics of a preview. */
	const marks = (tab: Tab, dots: Set<string>, shown: Set<string>) =>
		[dots.has(tab.id) ? 'modified' : '', tab.layout !== undefined && shown.has(tab.layout) ? 'preview' : '']
			.filter(Boolean)
			.join(' ');

	/** The gear's eight teeth, by the angle each is turned to. */
	const TEETH = [0, 45, 90, 135, 180, 225, 270, 315];
</script>

<nav class="root-bar" class:modified={dirty} aria-busy={!ready} aria-label="layouts">
	<div
		class="tabs"
		role="tablist"
		aria-orientation="horizontal"
		bind:this={strip}
		use:reorders={{ tabs: '[role="tab"]', move: (from, to) => apply((l) => moveTab(l, l.root.id, from, to)) }}
	>
		{#each $loaded ? tabs : [] as tab, i (tab.id)}
			{#if i === $layout.root.active && items.length > 0}
				<!-- The tab in front is the menu's button; the others switch. -->
				<PaneMenu
					{items}
					look="tab"
					label="menu of the {labelOf(tab)} layout"
					title="layout menu"
					data-help="The layout in front, and what it is saved as: save it, restore an earlier save, save it under another name, reset it to how it shipped, and, on a tab of one widget, set that widget's options."
					role="tab"
					aria-selected="true"
					tabindex="0"
					class={marks(tab, $modified, $previews)}
					on:keydown={(e) => onTabKey(e, i)}
					>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</PaneMenu
				>
			{:else}
				<button
					type="button"
					role="tab"
					class="control tab"
					class:modified={$modified.has(tab.id)}
					class:preview={tab.layout !== undefined && $previews.has(tab.layout)}
					aria-selected={i === $layout.root.active}
					tabindex={i === $layout.root.active ? 0 : -1}
					on:click={() => activate(i)}
					on:keydown={(e) => onTabKey(e, i)}
				>
					{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}
				</button>
			{/if}
		{/each}
	</div>
	<button type="button" class="control badge add" use:act={'open'} aria-label="open a layout" title="open a layout"
		data-help="A layout is a page of panes, each holding a widget. Opens one as a new tab — a saved layout, or a single widget on its own; the tabs switch between them."
		on:click={add}>+</button
	>
	<!-- The `▾` beside the `+` (da-bzhq): what is done to the page —
	     `Split`, which at the root is a side of the whole layout, and
	     `Close tab`. Drawn only when it has something in it, which is what
	     takes it away under a lock. -->
	{#if manage.length > 0}
		<PaneMenu
			items={manage}
			look="badge"
			label="manage the layout"
			title="manage this layout"
			class="manage"
			data-help="What is done to the whole page: Split, which opens a new pane down one whole side of the layout, and Close tab, which closes the layout in front."
			aria-label="manage the layout">▾</PaneMenu
		>
	{/if}
	{#if error}<span class="error" role="alert">{error}</span>{/if}
	<!-- Sync paused (da-wlbyx) is loud: vividn calls it risky. Every tab hears
	     it from the database's sync leader, and a tap resumes. -->
	{#if paused}
		<button type="button" class="control paused" title="sync paused since {paused} — tap to resume" on:click={() => run(['sync', 'resume'])}
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
	<span class="end"><Beta /></span>
	<!-- The lock, ON THE GEAR'S OTHER SIDE — vividn, 2026-09-21 (da-fphjw):
	     "Move the lock button to the other side of the gear". It was at
	     the bar's far end past the gear (da-bzhq); now the gear takes the
	     end and the lock is between it and the beta pair's ⓘ. It stays
	     BESIDE the gear either way, which is the whole of why it was put
	     here — "by the gear icon to be more discoverable" — and the ⓘ is
	     still the thing before it (da-y992, brand.test.mjs).

	     Its title says whose the lock is: THIS DEVICE'S (shell.ts), so a
	     control in the page's own corner is not read as a property of the
	     layout everyone shares. -->
	{#if acts}
		<button
			type="button"
			class="control badge lock"
			class:open={!locked}
			aria-pressed={locked}
			aria-label="{locked ? 'unlock' : 'lock'} the {name} layout on this device"
			title="{locked ? 'unlock' : 'lock'} this layout on this device"
			data-help="Locked, this layout's panes lose their +, ▾ and ×, and nothing you do changes the layout. The lock is this device's alone. Tap the padlock again to unlock it."
			on:click={doToggleLock}
		>
			<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
				<!-- The body is the same box either way; the SHACKLE is the
				     same arch either way too, and what says which state this
				     is, is that OPEN it has been swung off the body about its
				     right foot (da-fphjw). vividn: "It is hard to see that the
				     lock is unlocked with the current icon" — which drew the
				     two states a shackle's end apart, a difference of about a
				     pixel at the size this is drawn. A half-turned mark is a
				     different silhouette at any size. -->
				<path class="shackle" d="M5 7V5a3 3 0 0 1 6 0v2" transform={locked ? null : 'rotate(34 11 7)'} />
				<rect class="body" x="3.5" y="7" width="9" height="6.5" rx="1.5" />
			</svg>
		</button>
	{/if}
	<PaneMenu
		items={settings}
		look="badge"
		label="settings"
		title="settings"
		data-help="Settings for this whole page: Backups, where backups are planned, made and restored; Reset default layouts, which puts back the layouts datum ships with; and the version this is."
		class="gear"
		aria-label="settings"
		><svg class="cog" viewBox="-12 -12 24 24" width="20" height="20" aria-hidden="true">
			{#each TEETH as angle}<rect x="-1.9" y="-11.4" width="3.8" height="5.6" rx="1" transform="rotate({angle})" />{/each}
			<circle class="ring" r="6.2" />
		</svg></PaneMenu
	>
	{#if trail && !dirty}
		<ul class="versions">
			{#if trail.length}
				<li><button type="button" on:click={() => pick()}>→ forward</button></li>
			{:else}
				<li>not saved yet</li>
			{/if}
			{#each trail as v (v.version)}
				<li>
					<button type="button" on:click={() => pick(v.version)}
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

	/* The `+` is a badge, and does not stretch. Nor is the `▾` beside it
	   (da-bzhq), which is the same badge with another character in it. */
	.add,
	.root-bar :global(button.manage) {
		flex: none;
	}

	/* The lock (da-bzhq), right beside the gear and the same 24px badge as
	   the `+`. A flat mono-colour mark in `currentColor`, like the
	   maximize's. */
	.lock {
		flex: none;
	}

	.lock svg {
		display: block;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.4;
		stroke-linecap: round;
		stroke-linejoin: round;
		/* The SWUNG shackle's stroke grazes the top of its own viewBox.
		   A turn moves WHICH point of the arch is the highest, so it is not
		   the standing apex (8,2) that decides this — that one lands at
		   y=1.18, comfortably inside. Swept, the top of the SWUNG arch is
		   the point 56° along the arc, (6.32, 2.51), which the 34° turn
		   about (11,7) puts at y=0.664, and the 0.7px half-stroke takes to
		   -0.04. A viewBox is a clip, so without this the mark would be cut
		   there — by a hundredth of a pixel today, and by however much the
		   angle is ever changed by. Nothing can reach the gear or the ⓘ
		   either way: this is a 16px mark centred in a 24px badge, so it has
		   four pixels of room on every side, which shell.test.mjs measures
		   over the drawn shackle rather than over this box. */
		overflow: visible;
	}

	/* THE LOUD STATE IS THE UNLOCKED ONE (da-fphjw). vividn: "highlight when
	   it is unlocked rather than locked". Being locked is the state where
	   nothing can go wrong; being UNLOCKED is the one with consequences,
	   since the layout can be moved under you — so the highlight marks the
	   state that can surprise someone, not the state that cannot. It is
	   `--warn` rather than the accent for the same reason: the accent is
	   what this app paints a thing that is simply in force. */
	.lock.open {
		color: var(--warn);
		border-color: var(--warn);
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

	/* The gear: at the bar's right end at every width, and a badge like the
	   `+` — so it is on its 24px floor under a fingertip too
	   (reachable.test.mjs). It rides at the end because `.end`, two places
	   before it, carries the `auto`; the lock rides along between them
	   (da-fphjw).

	   A BIGGER badge than the rest of the bar's, and a drawn cog rather
	   than the `⚙` character (da-jbcv). vividn: "Make the gear icon much
	   bigger, it's almost invisible." The character was the trouble: at
	   the bar's font size its ink is a 8x16px scratch, and how much of
	   the em it fills is the reader's font's business and not ours. A
	   path fills the box it is given, on every device, and the box is
	   this one. */
	/* The bar's right end: the beta pair, then the LOCK, then the gear
	   (da-fphjw). `auto` on the pair rather than on the tabs, which scroll,
	   so everything after it is carried to the end together.

	   NOT positioned, on purpose, and nothing between it and the bar may
	   be: the beta note (Beta.svelte) is placed against the ROOT BAR, the
	   nearest positioned ancestor. Placed against the pair instead, a
	   phone-wide note ran off the screen's left by the gear's width
	   (brand.test.mjs, «its note opens DOWN … whole on a phone»). */
	.end {
		margin-left: auto;
		flex: none;
	}

	.root-bar :global(.gear) {
		flex: none;
		--badge: 28px;
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
