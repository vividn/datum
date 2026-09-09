<!--
	The beta banner.

	datum.day is early software holding data that exists in one browser and
	nowhere else. That is worth saying once, plainly, above the app.

	What it says about the data is read off the database that actually
	opened, not assumed: while storage is in-memory (see the `storage` store in
	$lib/storageState.ts) a reload really does discard everything, and a banner that
	said "stored in this browser" would be a promise the app isn't keeping.

	It also says WHY, where the app knows. "In memory only" covers two
	unlike situations, and only one of them the reader can undo: OPFS's
	pool is single-writer, so a second tab of datum.day gets no database
	while the first holds it. The worker reports that case as its own
	storage kind, and it earns the sentence that names the remedy — use
	the other tab, or close it -- this one recovers the database on its own (da-60ww).

	Dismissal sticks per browser; this is a warning, not a nag — but it
	dismisses the BETA notice only. While storage is degraded the banner is
	the only thing telling you that what you type is about to be lost, and a
	warning you can switch off is not a warning. Measured 2026-09-08 against
	the live site: dismiss once, open a second tab, type an entry — the app
	took it, discarded it, and said nothing at all.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { storage } from '$lib/storageState.js';

	const DISMISS_KEY = 'datum-beta-banner-dismissed';

	// Hidden until mounted: the static build has no localStorage, so
	// rendering the banner server-side would flash it for someone who
	// already dismissed it.
	let show = false;

	// The store, not a one-shot read: storage can recover after this mounts
	// (another tab closes, the pool frees up, the worker migrates across) and
	// a banner warning about lost data should go away when it stops being
	// true. `$storage` is the same value the repl frame tints itself from.
	// A blocked store means the dismissal cannot be remembered, so the
	// banner shows — which is the right way round for a warning.
	onMount(() => {
		try {
			show = localStorage.getItem(DISMISS_KEY) !== '1';
		} catch {
			show = true;
		}
	});

	function dismiss() {
		show = false;
		try {
			localStorage.setItem(DISMISS_KEY, '1');
		} catch {
			// It goes away for this session and comes back on the next.
		}
	}

	// Degraded storage overrides dismissal. Every branch here is one where an
	// entry typed now does not survive a reload, including `null` — not yet
	// known is not a licence to stay quiet about it.
	// Wider than `isDegraded` on purpose: the frame must not flash orange for
	// the few hundred ms before storage is known, but this banner should say
	// "checking" during them — not yet known is not a licence to stay quiet.
	$: degraded = $storage !== 'opfs';
	$: visible = show || degraded;
</script>

{#if visible}
	<aside class="beta">
		<span class="tag">beta</span>
		<p>
			datum.day is early software.
			{#if $storage === 'failed'}
				The database could not be opened, so nothing you log here will be saved.
			{:else if $storage === null}
				Checking where your entries are stored…
			{:else if $storage === 'memory-other-tab'}
				The database is still being handed over from a tab that just closed, so entries here are
				held in memory only for the moment. Nothing to do — this tab takes it over on its own and
				keeps what you typed.
			{:else if $storage === 'memory'}
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
