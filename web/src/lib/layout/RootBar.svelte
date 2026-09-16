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
	a root tab and a widget opens that kind's built-in one-widget layout.

	What the root's menu is missing, and why, is `menu.ts`'s comment: no
	`Break out` (a tab breaks out of its STRIP, and the root has no strip
	to leave) and no `Bar` (the root draws no bar of its own — this one is
	the shell's). Its `Split` is `addEdge`: a new pane down one whole side
	of the layout — what the page's four `…` edge handles used to do
	(da-qk2k) — which is what a split beside the shell can only mean
	(da-a642). What it has that a pane's has not is `export backup…`:
	there is one database.

	The layout's own acts — save, restore, save as…, lock — are in the
	menu too, where the orange dot is (vividn: "Inside the menu there
	could be a lock, save, and save as options. That seems more
	consistent. Maybe highlight Restore and Save so that it is obvious
	that that is what the orange dot is referring to"). They were a row of
	icons at the right end of this bar until da-wjob; the row is gone, no
	shortcut kept. While the tab is modified Save and Restore are marked
	in the menu and the tab keeps its dot, so the dot's meaning is one
	press from the dot. Restore goes back to the last save; with nothing
	to discard it opens the save's versions under the bar instead, and
	`→ forward` undoes the last restore. A tab holding a layout that
	SHIPPED with datum has a fifth, `Reset layout` (da-luwe), beside
	Restore: the other way back, to `defaults.ts` rather than to the save.
	The lock is this device's (shell.ts), and a LOCKED root tab follows
	the pane rule — the menu holds those acts alone, so `Unlock layout`
	stays reachable.

	A root tab is one COPY of its layout and has no privilege over a copy
	nested in a pane (da-crp5): the same acts are in that pane's menu
	(PaneFrame.svelte), and both ask for them by the copy's TAB ID, which
	is what says which copy's tree is being saved — so this menu acts on
	the ROOT tab's layout however deep the layouts under it go.

	At the right end is a gear (da-pp1a): the GLOBAL settings menu, the
	root's alone the way `export backup…` is, and the same `PaneMenu` as
	every other menu here. vividn, from the train: "Can we add a global
	settings menu at the top right (little gear icon)? ... Maybe we can put
	the version and hash string in that menu as well." That stamp — the
	version, a separator and the short sha, dim and disabled because it is
	a fact and not an act — is one of the two things it holds. It was the
	bottom bar's until da-wjob and this bar's until da-pp1a; it is out of
	the way for good, and still one press from anyone looking for it.

	The other is «Reset default layouts…» (da-jbcv). It held an act of that
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

	On a phone the tabs scroll sideways rather than wrapping — one row,
	always — while the `+` and the gear stay where they are.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { downloadBackup } from '$lib/backup.js';
	import { confirm } from '$lib/confirm.js';
	import { choose, fillTab } from './chooser.js';
	import { apply, layout, loaded } from './current.js';
	import { layoutActs, menuItems } from './menu.js';
	import PaneMenu from './PaneMenu.svelte';
	import { labelOf, titleOf } from './registry.js';
	import { activate, close, modified, open, resetDefaults, resetLayout, resetWidget, restore, save, saveAs, shippedTree, toggleLock, versions } from './shell.js';
	import { addEdge } from './tree.js';
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
	$: locked = !!active?.locked;
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
		if (picked) await act(() => open('layout' in picked ? picked.layout : titleOf(picked.kind)));
	}

	async function act(job: () => Promise<unknown>): Promise<void> {
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

	const doSave = () => act(async () => id !== undefined && save(id));

	/** Modified: back to the last save. Clean: the versions, to pick one. */
	const doRestore = () =>
		act(async () => {
			if (name === undefined || id === undefined) return;
			if (dirty) return restore(id);
			trail = trail ? null : await versions(name).catch(() => []);
		});

	const pick = (version?: number) =>
		act(async () => {
			if (id !== undefined) await restore(id, version);
			trail = null;
		});

	/** Back to the tree that shipped: the draft alone, so Restore is the way back (da-luwe). */
	const doReset = () => act(async () => id !== undefined && resetLayout(id));

	const doSaveAs = () =>
		act(async () => {
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
					saveAs: doSaveAs,
					toggleLock: () => id !== undefined && toggleLock(id)
				};

	$: items = locked
		? acts
			? layoutActs(acts)
			: []
		: menuItems({
				choose: () => id !== undefined && void fillTab(id),
				// The same rule a pane's frame reads: a root tab holding a bare
				// widget — one whose kind has no built-in layout (da-y2f6) — can
				// be reset like any other. One holding a layout cannot.
				reset: resetWidget(active),
				// No `breakOut`: a tab breaks out of its strip, and the root has
				// no strip to leave (da-a642). No `bar`: this bar IS the root's.
				split: { what: 'split: a new pane', run: (t) => apply((l) => addEdge(l, SIDE[t])) },
				close: { label: 'Close tab', disabled: tabs.length < 2, run: () => close($layout.root.active) },
				acts,
				extra: [{ label: 'export backup…', run: () => void act(downloadBackup) }]
			});

	/**
	 * «Reset default layouts» (da-jbcv), behind a confirm: it writes, and
	 * what it writes over is the reader's own save of a name datum ships.
	 * Its narrow aim is the whole of what it says, because the act it
	 * replaces — «Reset all layouts…», which deleted every saved layout on
	 * every device — was withdrawn for being wider than its name (da-luwe).
	 */
	const doResetDefaults = () =>
		act(async () => {
			const yes = await confirm(
				'Every layout that ships with datum goes back to the way it came, and the tabs at the top of the page go back to just “day”.\n\n' +
					'Layouts you saved yourself, and layouts a kit installed, are not touched — they stay in the +.',
				'reset'
			);
			if (yes) await resetDefaults();
		});

	/**
	 * The gear's menu: the page's own settings, which belong to no layout
	 * and no pane — the one act above, and which build is in front of you,
	 * disabled, because it is a fact and not an act.
	 *
	 * A constant: it does not depend on which layout is in front, which is
	 * the whole of what makes this menu the page's and not a tab's.
	 */
	const settings = [
		{ label: 'Reset default layouts…', run: doResetDefaults },
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

	/** The gear's eight teeth, by the angle each is turned to. */
	const TEETH = [0, 45, 90, 135, 180, 225, 270, 315];
</script>

<nav class="root-bar" class:modified={dirty} aria-busy={!ready} aria-label="layouts">
	<div class="tabs" role="tablist" aria-orientation="horizontal" bind:this={strip}>
		{#each $loaded ? tabs : [] as tab, i (tab.id)}
			{#if i === $layout.root.active && items.length > 0}
				<!-- The tab in front is the menu's button; the others switch. -->
				<PaneMenu
					{items}
					look="tab"
					label="menu of the {labelOf(tab)} layout"
					title="layout menu"
					role="tab"
					aria-selected="true"
					tabindex="0"
					class={$modified.has(tab.id) ? 'modified' : ''}
					on:keydown={(e) => onTabKey(e, i)}
					>{labelOf(tab)}{#if $modified.has(tab.id)}<span class="dot" title="modified">•</span>{/if}</PaneMenu
				>
			{:else}
				<button
					type="button"
					role="tab"
					class="control tab"
					class:modified={$modified.has(tab.id)}
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
	<button type="button" class="control badge add" aria-label="open a layout" title="open a layout" on:click={add}>+</button>
	{#if error}<span class="error" role="alert">{error}</span>{/if}
	<PaneMenu
		items={settings}
		look="badge"
		label="settings"
		title="settings"
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

	.tabs :global(.tab:hover),
	.tabs :global(.tab:focus-visible) {
		color: var(--accent-bright);
	}

	/* The `+` is a badge, and does not stretch. */
	.add {
		flex: none;
	}

	.dot {
		margin-left: 0.3rem;
		color: var(--warn);
	}

	/* The gear: the bar's right end at every width, and a badge like the
	   `+` — so it is on its 24px floor under a fingertip too
	   (reachable.test.mjs). `auto` here rather than on the tabs, which
	   scroll.

	   A BIGGER badge than the rest of the bar's, and a drawn cog rather
	   than the `⚙` character (da-jbcv). vividn: "Make the gear icon much
	   bigger, it's almost invisible." The character was the trouble: at
	   the bar's font size its ink is a 8x16px scratch, and how much of
	   the em it fills is the reader's font's business and not ours. A
	   path fills the box it is given, on every device, and the box is
	   this one. */
	.root-bar :global(.gear) {
		margin-left: auto;
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

	.error {
		font-size: 0.75rem;
		/* Not `--error`: a lighter red, because this one is a line of words
		   on the bar rather than a mark on a control. */
		color: #ff7b7b;
		white-space: nowrap;
	}
</style>
