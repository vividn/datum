<!--
	The stats widget on a page of its own — where it lives until the
	dashboard engine registers it as a kind (da-n7r0). Its settings are
	kept here in the meantime, in `localStorage`; the engine will keep them
	in the layout instead.
-->
<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Stats from '$lib/Stats.svelte';
	import { initDb } from '$lib/db.js';
	import { onExternalChange } from '$lib/changes.js';
	import { DEFAULT_STATS, type StatsSettings } from '$lib/stats.js';

	const STORE_KEY = 'datum.stats.v1';

	let ready = false;
	let epoch = 0;
	let settings: StatsSettings = DEFAULT_STATS;

	onMount(async () => {
		// Blocked storage throws on the access; the default stands.
		try {
			settings = { ...DEFAULT_STATS, ...JSON.parse(localStorage.getItem(STORE_KEY) ?? '{}') };
		} catch {
			// Nothing to restore.
		}
		await initDb();
		ready = true;
	});

	// Another tab wrote: the numbers may have changed.
	onDestroy(onExternalChange(() => (epoch += 1)));

	function remember(event: CustomEvent<StatsSettings>): void {
		try {
			localStorage.setItem(STORE_KEY, JSON.stringify(event.detail));
		} catch {
			// Nowhere to write. This visit still works.
		}
	}
</script>

<h2>stats</h2>

{#if ready}
	<Stats {settings} {epoch} on:change={remember} />
{:else}
	<p class="quiet">loading...</p>
{/if}

<style>
	h2 {
		font-family: monospace;
		color: #7c7cff;
		margin: 0 0 1rem 0;
		font-size: 1.2rem;
		font-weight: normal;
	}

	.quiet {
		color: #444466;
		font-family: monospace;
	}
</style>
