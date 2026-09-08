<!--
	The beta banner.

	datum.day is early software holding data that exists in one browser and
	nowhere else. That is worth saying once, plainly, above the app.

	What it says about the data is read off the database that actually
	opened, not assumed: while storage is in-memory (see `storageKind` in
	$lib/db.ts) a reload really does discard everything, and a banner that
	said "stored in this browser" would be a promise the app isn't keeping.

	It also says WHY, where the app knows. "In memory only" covers two
	unlike situations, and only one of them the reader can undo: OPFS's
	pool is single-writer, so a second tab of datum.day gets no database
	while the first holds it. The worker reports that case as its own
	storage kind, and it earns the sentence that names the remedy — use
	the other tab, or close it and reload this one (da-60ww).

	Dismissal sticks per browser; this is a warning, not a nag — but it
	dismisses the BETA notice only. While storage is degraded the banner is
	the only thing telling you that what you type is about to be lost, and a
	warning you can switch off is not a warning. Measured 2026-09-08 against
	the live site: dismiss once, open a second tab, type an entry — the app
	took it, discarded it, and said nothing at all.
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
	// Distinct from `storage === null`: that means "not determined yet", this
	// means "we tried and it failed". Both must avoid the reassurance, but a
	// reader who is never going to get an answer deserves to be told so.
	let storageFailed = false;

	onMount(async () => {
		show = localStorage.getItem(DISMISS_KEY) !== '1';
		// Idempotent — the page's own init and this share one promise.
		// If it throws, `storage` stays null and the banner says so rather than
		// falling through to a reassurance it cannot support.
		try {
			await initDb();
			storage = storageKind();
		} catch {
			storage = null;
			storageFailed = true;
		}
	});

	function dismiss() {
		show = false;
		localStorage.setItem(DISMISS_KEY, '1');
	}

	// Degraded storage overrides dismissal. Every branch here is one where an
	// entry typed now does not survive a reload, including `null` — not yet
	// known is not a licence to stay quiet about it.
	$: degraded = storageFailed || storage === null || storage === 'memory' || storage === 'memory-other-tab';
	$: visible = show || degraded;
</script>

{#if visible}
	<aside class="beta">
		<span class="tag">beta</span>
		<p>
			datum.day is early software.
			{#if storageFailed}
				The database could not be opened, so nothing you log here will be saved.
			{:else if storage === null}
				Checking where your entries are stored…
			{:else if storage === 'memory-other-tab'}
				Another datum.day tab has the database open, so entries here are held in memory only —
				reloading this page discards them. Use that tab, or close it and reload this one.
			{:else if storage === 'memory'}
				Entries are held in memory only — reloading this page discards them.
			{:else}
				Your entries are stored in this browser only: clearing site data deletes them, and there is
				no sync yet.
			{/if}
		</p>
		{#if !degraded}
			<button type="button" on:click={dismiss} aria-label="Dismiss beta notice">×</button>
		{/if}
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
