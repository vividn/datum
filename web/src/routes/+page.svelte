<!--
	The frame — the shell's single screen (`/`).

	It used to be a CSS grid with four fixed areas, laid out as the
	wireframe (da-hrz0) drew it. It is a layout tree now (da-jdop): panes
	holding widgets, split and tabbed and resized from each pane's `…`
	menu, with today's arrangement as the default (`$lib/layout/defaults`)
	and the reader's own kept as a draft (`$lib/layout/current`). Its root
	pane's tabs are the page's top bar (da-0a5o, `$lib/layout/RootBar`),
	each one a layout, and the shell (`$lib/layout/shell`) fills the tree
	on boot — from `+layout.svelte`, so `loaded` is already on its way by
	the time this mounts.

	Every widget still talks to datum through `run(argv)` in $lib/run.ts
	and nothing else. What this page owns is the one refresh beat: the
	engine's epoch, bumped after every command here and after every write
	another tab makes, flows down the tree as a prop to every pane.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import Layout from '$lib/layout/Layout.svelte';
	import PaneMenu from '$lib/layout/PaneMenu.svelte';
	import { apply, layout, loaded } from '$lib/layout/current.js';
	import { epoch, ran } from '$lib/layout/engine.js';
	import { addEdge } from '$lib/layout/tree.js';
	import type { BarEdge } from '$lib/layout/types.js';
	import { onExternalChange } from '$lib/changes.js';

	// The dashboard extends from its edges (da-qk2k): a faint `…` in the
	// page's margin on each side, whose one item adds a pane spanning that
	// whole side of the layout in front. None under the lock.
	const EDGES: BarEdge[] = ['top', 'right', 'bottom', 'left'];
	$: locked = !!$layout.root.tabs[$layout.root.active]?.locked;

	// Another tab wrote. Same treatment as a local command finishing — bump
	// the epoch the panes hang off and rebuild the widget bar — so there is
	// one refresh path rather than two that can disagree.
	//
	// Subscribed here rather than from `onMount`: Svelte takes a teardown from
	// a SYNCHRONOUS onMount only, and an async one returns a promise it cannot
	// call. Safe during init because `onExternalChange` opens nothing outside
	// a browser (see $lib/changes.ts) and returns an empty teardown there.
	onDestroy(onExternalChange(() => void ran()));
</script>

<div class="frame">
	{#if $loaded}
		<Layout pane={$layout.root} epoch={$epoch} rootId={$layout.root.id} />
		{#if !locked}
			{#each EDGES as edge (edge)}
				<div class="edge {edge}">
					<PaneMenu
						faint
						label="{edge} edge of the dashboard"
						title="extend the dashboard"
						items={[{ label: 'Add widget', run: () => apply((l) => addEdge(l, edge)) }]}
					/>
				</div>
			{/each}
		{/if}
	{/if}
</div>

<style>
	.frame {
		position: relative;
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	/* Each handle fills the page's margin (the main's 0.75rem padding,
	   +layout.svelte) outside the middle of one side of the root's box,
	   2rem along it — over neither a pane nor the prompt bar. Above the
	   root's scroller, which reaches out over that margin. */
	.edge {
		position: absolute;
		z-index: 3;
		display: flex;
		margin: auto;
	}
	.top,
	.bottom {
		left: 0;
		right: 0;
		width: 2rem;
		height: 0.75rem;
	}
	.top {
		top: -0.75rem;
	}
	.bottom {
		bottom: -0.75rem;
	}
	.left,
	.right {
		top: 0;
		bottom: 0;
		width: 0.75rem;
		height: 2rem;
		writing-mode: vertical-lr;
	}
	.left {
		left: -0.75rem;
	}
	.right {
		right: -0.75rem;
	}
</style>
