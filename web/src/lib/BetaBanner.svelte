<!--
	The beta banner.

	datum.day is early software holding data that exists in one browser and
	nowhere else. That is worth saying once, plainly, above the app.

	What it says about the data is read off the database that actually
	opened, not assumed: while storage is in-memory (see `storageKind` in
	$lib/db.ts) a reload really does discard everything, and a banner that
	said "stored in this browser" would be a promise the app isn't keeping.
	Dismissal sticks per browser; this is a warning, not a nag.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { initDb, storageKind, type StorageKind } from '$lib/db.js';

	const DISMISS_KEY = 'datum-beta-banner-dismissed';

	// Hidden until mounted: the static build has no localStorage, so
	// rendering the banner server-side would flash it for someone who
	// already dismissed it.
	let show = false;
	let storage: StorageKind | null = null;

	onMount(async () => {
		show = localStorage.getItem(DISMISS_KEY) !== '1';
		// Idempotent — the page's own init and this share one promise.
		await initDb();
		storage = storageKind();
	});

	function dismiss() {
		show = false;
		localStorage.setItem(DISMISS_KEY, '1');
	}
</script>

{#if show}
	<aside class="beta">
		<span class="tag">beta</span>
		<p>
			datum.day is early software.
			{#if storage === 'memory'}
				Entries are held in memory only — reloading this page discards them.
			{:else}
				Your entries are stored in this browser only: clearing site data deletes them, and there is
				no sync yet.
			{/if}
		</p>
		<button type="button" on:click={dismiss} aria-label="Dismiss beta notice">×</button>
	</aside>
{/if}

<style>
	.beta {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.4rem 0.75rem;
		background: #2a2010;
		border-bottom: 1px solid #5a4418;
		font-size: 0.8rem;
		line-height: 1.35;
	}

	.tag {
		flex-shrink: 0;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		font-size: 0.7rem;
		color: #0a0a1a;
		background: #f0b429;
		border-radius: 3px;
		padding: 0.05rem 0.35rem;
	}

	p {
		margin: 0;
		color: #e8d5a8;
	}

	button {
		margin-left: auto;
		flex-shrink: 0;
		background: transparent;
		border: none;
		color: #c0a060;
		font-size: 1.1rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 0.2rem;
	}

	button:hover {
		color: #f0b429;
	}
</style>
