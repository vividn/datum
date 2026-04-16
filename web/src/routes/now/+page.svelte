<script lang="ts">
	import { onMount } from 'svelte';
	import { initDb, getLatestPerField } from '$lib/db.js';
	import type { Entry } from '$lib/types.js';

	let entries: Entry[] = [];
	let loading = true;

	onMount(async () => {
		await initDb();
		entries = await getLatestPerField();
		loading = false;
	});

	function formatTime(entry: Entry): string {
		const t = entry.timeStart?.utc ?? entry.createdAt;
		const d = new Date(t);
		const now = new Date();
		const diffMs = now.getTime() - d.getTime();
		const diffMin = Math.floor(diffMs / 60000);
		if (diffMin < 1) return 'just now';
		if (diffMin < 60) return `${diffMin}m ago`;
		const diffH = Math.floor(diffMin / 60);
		if (diffH < 24) return `${diffH}h ago`;
		return d.toLocaleDateString();
	}

	function formatValue(entry: Entry): string {
		if (entry.value === undefined) return '';
		let s = JSON.stringify(entry.value);
		if (entry.unit) s += ` ${entry.unit}`;
		return s;
	}
</script>

<div class="now-view">
	<h2>now</h2>

	{#if loading}
		<p class="loading">loading...</p>
	{:else if entries.length === 0}
		<p class="empty">No data yet. Start logging from the <a href="/">main page</a>.</p>
	{:else}
		<div class="grid">
			{#each entries as entry (entry.field)}
				<div class="field-card">
					<div class="field-name">{entry.field}</div>
					<div class="field-value">
						{#if entry.value !== undefined}
							<span class="value">{formatValue(entry)}</span>
						{:else}
							<span class="verb">{entry.verb}</span>
						{/if}
					</div>
					<div class="field-time">{formatTime(entry)}</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	h2 {
		font-family: monospace;
		color: #7c7cff;
		margin: 0 0 1rem 0;
		font-size: 1.2rem;
		font-weight: normal;
	}

	.loading, .empty {
		color: #444466;
		font-family: monospace;
	}

	.empty a {
		color: #7c7cff;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 0.75rem;
	}

	.field-card {
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 6px;
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.field-name {
		font-family: monospace;
		font-size: 0.85rem;
		color: #7c7cff;
	}

	.field-value {
		font-family: monospace;
		font-size: 1rem;
		color: #e0e0ff;
	}

	.verb {
		color: #8888aa;
		font-style: italic;
	}

	.field-time {
		font-family: monospace;
		font-size: 0.75rem;
		color: #444466;
		margin-top: auto;
	}
</style>
