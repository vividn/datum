<!--
	Save, restore and save-as for the layout this device shows (da-wpgy).

	The saved layout is an entry in the `_internal` database ($lib/layout/
	store.ts); the unsaved draft is this device's ($lib/layout/draft.ts).
	While the live tree differs from the save, Save and Restore both stand
	out — vividn: "Both should become more pronounced when a layout is modified
	so that the user can decide." Restore then goes back to the last save;
	with nothing to discard it opens the save's versions instead, and
	`→ forward` undoes the last restore.
-->
<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { writable, type Writable } from 'svelte/store';
	import { onExternalChange } from '$lib/changes.js';
	import { initDb } from '$lib/db.js';
	import { loadLayout, restore, saveAs, saveLayout, versions } from './store.js';
	import { clearDraft, currentName, isModified, loadDraft, saveDraft, setCurrentName } from './draft.js';
	import { LAYOUT_VERSION, type Layout } from './types.js';

	/** What a layout with no save starts as. TODO(da-jdop): the engine's default. */
	function fallback(name: string): Layout {
		return {
			version: LAYOUT_VERSION,
			name,
			root: {
				id: 'root',
				tabs: [{ id: 'tab', content: { id: 'widget', kind: 'daystrip', settings: {} } }],
				active: 0,
				bar: { edge: 'top', align: 'start' }
			}
		};
	}

	/**
	 * The live tree. TODO(da-jdop): the engine (E1) owns it — pass its
	 * `layout` store from $lib/layout/current.ts once it lands. Until then
	 * the controls hold a stub that only a draft, or a save, changes.
	 */
	export let layout: Writable<Layout> = writable(fallback(currentName()));

	/** What Restore brings back: the last save, or the fallback when none. */
	let saved = $layout;
	let ready = false;
	let busy = false;
	/** The save's versions, newest first, while the list is open. */
	let trail: { version: number; modifyTime: string }[] | null = null;
	let error = '';

	$: modified = isModified($layout, saved);
	// Every change to the live tree is the draft; a tree equal to the save has none.
	$: if (ready) {
		if (modified) saveDraft($layout);
		else clearDraft($layout.name);
	}

	onMount(async () => {
		await initDb().catch(() => {});
		const name = currentName();
		saved = (await loadLayout(name).catch(() => null)) ?? fallback(name);
		layout.set(loadDraft(name) ?? saved);
		ready = true;
	});

	// Another tab saved: the baseline moves, and a tree nobody modified here
	// moves with it. A save that cannot be read (deleted, say) moves nothing.
	onDestroy(
		onExternalChange(async () => {
			if (!ready) return;
			const followed = !modified;
			const next = await loadLayout($layout.name).catch(() => null);
			if (!next) return;
			saved = next;
			if (followed) layout.set(next);
		}, '_internal')
	);

	async function act(job: () => Promise<void>): Promise<void> {
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

	const save = () =>
		act(async () => {
			await saveLayout($layout);
			saved = $layout;
		});

	const toggleVersions = () =>
		act(async () => {
			trail = trail ? null : await versions($layout.name).catch(() => []);
		});

	const pick = (version?: number) =>
		act(async () => {
			saved = await restore($layout.name, version);
			layout.set(saved);
			trail = null;
		});

	const saveAsNew = () =>
		act(async () => {
			const name = prompt('Save this layout as:', '')?.trim();
			if (!name) return;
			const copy = await saveAs($layout, name);
			// The edit went into the copy; the original keeps its save.
			clearDraft($layout.name);
			setCurrentName(name);
			saved = copy;
			layout.set(copy);
		});
</script>

<div class="layout-controls" class:modified>
	<button type="button" class="save" disabled={!ready || busy} on:click={save}>save</button>
	{#if modified}
		<button type="button" class="restore" disabled={!ready || busy} on:click={() => layout.set(saved)}
			>restore</button
		>
	{:else}
		<button
			type="button"
			class="restore"
			aria-expanded={trail !== null}
			disabled={!ready || busy}
			on:click={toggleVersions}>restore ▾</button
		>
	{/if}
	<button type="button" class="save-as" disabled={!ready || busy} on:click={saveAsNew}>save as…</button>
	{#if trail && !modified}
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
	{#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
	/* Gives way on a narrow screen — the buttons wrap under each other — so
	   the header never pushes the page sideways. */
	.layout-controls {
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 0.75rem;
		min-width: 0;
	}

	button {
		white-space: nowrap;
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 0.85rem;
		color: #7c7cff;
		cursor: pointer;
	}

	button:hover:not(:disabled) {
		color: #adadff;
	}

	button:disabled {
		opacity: 0.5;
		cursor: default;
	}

	/* Modified: the two buttons that decide it stand out. */
	.modified .save,
	.modified .restore {
		color: #ffd166;
		font-weight: bold;
	}

	.versions {
		position: absolute;
		top: 100%;
		left: 0;
		z-index: 10;
		margin: 0.3rem 0 0;
		padding: 0.3rem 0.6rem;
		list-style: none;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
		white-space: nowrap;
		font-size: 0.85rem;
		color: #55558a;
	}

	.error {
		font-size: 0.75rem;
		color: #ff7b7b;
	}
</style>
