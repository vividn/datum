<!--
	The root tab bar — the page's top bar, in place of the header (da-0a5o).

	The root pane's tabs, each a pill: click or the arrow keys to switch,
	a dot on one whose tree differs from its save. After them a `+` that
	opens the one chooser (da-uyx1) — the layouts, then the widget kinds,
	where a layout opens as a root tab and a widget opens that kind's
	built-in one-widget layout — and a `…` with what the header used to
	offer. At the right end the four acts on the ACTIVE tab's layout:
	save, restore, save as…, lock — real buttons with names, 24px, glyphs
	drawn inline. While the tab is modified, save and restore stand out
	(vividn: "Both should become more pronounced when a layout is modified so
	that the user can decide"), and restore goes back to the last save;
	with nothing to discard it opens the save's versions instead, and
	`→ forward` undoes the last restore. The lock is this device's
	(shell.ts).

	On a phone the tabs scroll sideways rather than wrapping — one row,
	always — while the `+`, the `…` and the icons stay where they are.
-->
<script lang="ts">
	import { downloadBackup } from '$lib/backup.js';
	import { choose } from './chooser.js';
	import { layout, loaded } from './current.js';
	import PaneMenu from './PaneMenu.svelte';
	import { labelOf, titleOf } from './registry.js';
	import { activate, close, modified, open, restore, save, saveAs, toggleLock, versions } from './shell.js';
	import type { Tab } from './types.js';

	let busy = false;
	let error = '';
	/** The active save's versions, newest first, while the list is open. */
	let trail: { version: number; modifyTime: string }[] | null = null;

	$: tabs = $layout.root.tabs;
	$: active = tabs[$layout.root.active] as Tab | undefined;
	$: name = active?.layout;
	$: dirty = name !== undefined && $modified.has(name);
	$: locked = !!active?.locked;
	$: ready = $loaded && !busy && name !== undefined;

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

	$: moreItems = [
		{ label: 'Close tab', disabled: tabs.length < 2, run: () => close($layout.root.active) },
		{ label: 'export backup…', run: () => void act(downloadBackup) }
	];

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

	const doSave = () => act(async () => name !== undefined && save(name));

	const doRestore = () =>
		act(async () => {
			if (name === undefined) return;
			if (dirty) return restore(name);
			trail = trail ? null : await versions(name).catch(() => []);
		});

	const pick = (version?: number) =>
		act(async () => {
			if (name !== undefined) await restore(name, version);
			trail = null;
		});

	const doSaveAs = () =>
		act(async () => {
			if (name === undefined) return;
			const to = prompt('Save this layout as:', '')?.trim();
			if (!to) return;
			await saveAs(name, to);
		});

	/** Arrow keys walk the tabs; the tab focused is the tab shown. */
	function onTabKey(event: KeyboardEvent, index: number) {
		const next = event.key === 'ArrowRight' ? index + 1 : event.key === 'ArrowLeft' ? index - 1 : null;
		if (next === null || next < 0 || next >= tabs.length) return;
		event.preventDefault();
		activate(next);
		(event.currentTarget as HTMLElement).parentElement
			?.querySelectorAll<HTMLElement>('[role="tab"]')
			[next]?.focus();
	}
</script>

<nav class="root-bar" class:modified={dirty} aria-label="layouts">
	<div class="tabs" role="tablist" aria-orientation="horizontal">
		{#each $loaded ? tabs : [] as tab, i (tab.id)}
			<button
				type="button"
				role="tab"
				class="control tab"
				class:modified={tab.layout !== undefined && $modified.has(tab.layout)}
				aria-selected={i === $layout.root.active}
				tabindex={i === $layout.root.active ? 0 : -1}
				on:click={() => activate(i)}
				on:keydown={(e) => onTabKey(e, i)}
			>
				{labelOf(tab)}{#if tab.layout !== undefined && $modified.has(tab.layout)}<span class="dot" title="modified">•</span>{/if}
			</button>
		{/each}
	</div>
	<button type="button" class="control badge add" aria-label="open a layout" title="open a layout" on:click={add}>+</button>
	<PaneMenu items={moreItems} label="menu of the root bar" />
	{#if error}<span class="error" role="alert">{error}</span>{/if}
	<div class="acts">
		<button type="button" class="control badge act save" disabled={!ready} aria-label="save layout" title="save" on:click={doSave}>
			<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" /></svg>
		</button>
		<button
			type="button"
			class="control badge act restore"
			disabled={!ready}
			aria-label={dirty ? 'restore layout' : 'restore a version'}
			aria-expanded={dirty ? undefined : trail !== null}
			title={dirty ? 'restore the last save' : 'versions'}
			on:click={doRestore}
		>
			<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M1 4v6h6M3.51 15a9 9 0 1 0 2.13-9.36L1 10" /></svg>
		</button>
		<button type="button" class="control badge act save-as" disabled={!ready} aria-label="save layout as…" title="save as…" on:click={doSaveAs}>
			<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1M15.5 12.5v6M12.5 15.5h6" /></svg>
		</button>
		<button
			type="button"
			class="control badge act lock"
			disabled={!ready}
			aria-label={locked ? 'unlock layout' : 'lock layout'}
			aria-pressed={locked}
			title={locked ? 'unlock' : 'lock'}
			on:click={() => name !== undefined && toggleLock(name)}
		>
			<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" />{#if locked}<path d="M7 11V7a5 5 0 0 1 10 0v4" />{:else}<path d="M7 11V7a5 5 0 0 1 9.9-1" />{/if}</svg>
		</button>
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
	</div>
</nav>

<style>
	.root-bar {
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
	   size down (PaneFrame.svelte). */
	.tab {
		block-size: 24px;
		padding: 0 0.6rem;
		font-size: 0.8rem;
		letter-spacing: 0.06em;
		white-space: nowrap;
	}

	/* The tab in front says so with its outline — which is why the hover
	   below lives here and not on `.control`: it has to be able to argue
	   with this rule, and it can only do that from inside the component. */
	.tab[aria-selected='true'] {
		color: var(--accent);
		border-color: var(--accent);
	}

	.tab:hover,
	.tab:focus-visible {
		color: var(--accent-bright);
	}

	/* The `+` is a badge like the `…` beside it, and does not stretch. */
	.add {
		flex: none;
	}

	.dot {
		margin-left: 0.3rem;
		color: var(--warn);
	}

	.acts {
		position: relative;
		display: flex;
		gap: 4px;
		margin-left: auto;
		flex: none;
	}

	/* `.control.badge` (app.css) draws the box. An act carries the accent
	   at rest rather than the muted ink: it is a thing to do, not a thing
	   to find. */
	.act {
		color: var(--accent);
	}

	/* Hover is repeated from `.control.badge` rather than inherited from it
	   for the same reason the tabs repeat theirs: a scoped selector carries
	   svelte's hash TWICE, so `.act[aria-pressed]` below outweighs anything
	   app.css can say, and a locked lock would keep its amber under the
	   pointer. Whatever has a local state to argue with hover keeps its
	   hover local. */
	.act:hover:not(:disabled),
	.act:focus-visible {
		color: var(--accent-bright);
		border-color: var(--accent);
	}

	.act svg {
		width: 14px;
		height: 14px;
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.act:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.act[aria-pressed='true'] {
		color: var(--warn);
		border-color: var(--warn);
	}

	/* Modified: the two acts that decide it stand out. */
	.modified .save,
	.modified .restore {
		color: var(--warn);
		border-color: var(--warn);
	}
	.modified .save svg,
	.modified .restore svg {
		stroke-width: 3;
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
