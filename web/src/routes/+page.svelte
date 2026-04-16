<script lang="ts">
	import { onMount } from 'svelte';
	import { initDb, insertEntry, getRecentEntries } from '$lib/db.js';
	import { parseInput } from '$lib/parse.js';
	import { generateId, buildSlug, contentHash } from '$lib/entryId.js';
	import type { Entry } from '$lib/types.js';

	let inputValue = '';
	let entries: Entry[] = [];
	let status = '';
	let statusType: 'ok' | 'error' | '' = '';
	let dbReady = false;
	let deviceId = '';

	onMount(async () => {
		deviceId = localStorage.getItem('datum-device-id') ?? generateDeviceId();
		localStorage.setItem('datum-device-id', deviceId);
		await initDb();
		dbReady = true;
		entries = await getRecentEntries(20);
	});

	function generateDeviceId(): string {
		return 'web-' + Math.random().toString(36).slice(2, 10);
	}

	async function handleSubmit() {
		const raw = inputValue.trim();
		if (!raw) return;

		const parsed = parseInput(raw);
		if (!parsed) {
			status = 'Could not parse input';
			statusType = 'error';
			return;
		}

		const now = new Date();
		let entryTime = now;
		if (parsed.timeOffset !== undefined) {
			entryTime = new Date(now.getTime() + parsed.timeOffset * 60 * 1000);
		} else if (parsed.timeAbsolute) {
			const [hStr, mStr] = parsed.timeAbsolute.match(/^(\d{2})(\d{2})$/)?.slice(1) ?? [];
			if (hStr && mStr) {
				entryTime = new Date(now);
				entryTime.setHours(parseInt(hStr), parseInt(mStr), 0, 0);
			}
		}

		const offsetMinutes = -now.getTimezoneOffset();
		const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
		const timeStart = {
			utc: entryTime.toISOString(),
			offsetMinutes,
			timezone
		};

		const hash = await contentHash(parsed.field, timeStart, undefined, parsed.value, parsed.extraData);
		const id = generateId();
		const slug = buildSlug(parsed.field, timeStart);

		const entry: Entry = {
			id,
			slug,
			contentHash: hash,
			field: parsed.field,
			verb: parsed.verb,
			value: parsed.value,
			extraData: parsed.extraData,
			unit: parsed.unit,
			timeStart,
			createdAt: now.toISOString(),
			modifiedAt: now.toISOString(),
			deviceId
		};

		try {
			await insertEntry(entry);
			inputValue = '';
			status = `✓ ${entry.verb} ${entry.field}${entry.value !== undefined ? ' = ' + JSON.stringify(entry.value) : ''}`;
			statusType = 'ok';
			entries = await getRecentEntries(20);
		} catch (e) {
			status = `Error: ${e instanceof Error ? e.message : String(e)}`;
			statusType = 'error';
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') handleSubmit();
	}

	function formatTime(entry: Entry): string {
		const t = entry.timeStart?.utc ?? entry.createdAt;
		const d = new Date(t);
		return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	}

	function formatEntry(entry: Entry): string {
		let s = `[${entry.verb}] ${entry.field}`;
		if (entry.value !== undefined) {
			s += ` = ${JSON.stringify(entry.value)}`;
		}
		if (entry.unit) s += ` ${entry.unit}`;
		if (entry.extraData) {
			const extras = Object.entries(entry.extraData as Record<string, unknown>)
				.map(([k, v]) => `${k}=${JSON.stringify(v)}`)
				.join(' ');
			s += `  ${extras}`;
		}
		return s;
	}
</script>

<div class="app">
	<div class="prompt-bar">
		<span class="prompt-symbol">❯</span>
		<input
			type="text"
			bind:value={inputValue}
			on:keydown={handleKeydown}
			placeholder={dbReady ? 'occur coffee 2 cups' : 'loading db...'}
			disabled={!dbReady}
			autofocus
		/>
		<button on:click={handleSubmit} disabled={!dbReady}>enter</button>
	</div>

	{#if status}
		<div class="status" class:ok={statusType === 'ok'} class:error={statusType === 'error'}>
			{status}
		</div>
	{/if}

	<div class="tail">
		{#if entries.length === 0}
			<div class="empty">No entries yet. Try: <code>switch mood happy</code></div>
		{:else}
			{#each entries as entry (entry.id)}
				<div class="row">
					<span class="time">{formatTime(entry)}</span>
					<span class="entry-text">{formatEntry(entry)}</span>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	.app {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		max-width: 900px;
	}

	.prompt-bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		background: #0d0d1a;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
		padding: 0.5rem 0.75rem;
	}

	.prompt-symbol {
		color: #7c7cff;
		font-size: 1rem;
	}

	input {
		flex: 1;
		background: transparent;
		border: none;
		outline: none;
		color: #e0e0ff;
		font-family: monospace;
		font-size: 1rem;
	}

	input::placeholder {
		color: #444466;
	}

	input:disabled {
		opacity: 0.5;
	}

	button {
		background: #1a1a3a;
		color: #7c7cff;
		border: 1px solid #2a2a6a;
		border-radius: 3px;
		padding: 0.2rem 0.6rem;
		font-family: monospace;
		font-size: 0.85rem;
		cursor: pointer;
	}

	button:hover:not(:disabled) {
		background: #2a2a5a;
	}

	button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.status {
		font-family: monospace;
		font-size: 0.9rem;
		padding: 0.3rem 0.5rem;
		border-radius: 3px;
	}

	.status.ok {
		color: #4caf88;
		background: #0d2a1e;
	}

	.status.error {
		color: #f44;
		background: #2a0d0d;
	}

	.tail {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.row {
		display: flex;
		gap: 1rem;
		font-family: monospace;
		font-size: 0.9rem;
		padding: 0.2rem 0.4rem;
		border-radius: 2px;
	}

	.row:hover {
		background: #12122a;
	}

	.time {
		color: #555577;
		min-width: 5rem;
		flex-shrink: 0;
	}

	.entry-text {
		color: #c8c8e8;
	}

	.empty {
		color: #444466;
		font-style: italic;
		font-size: 0.9rem;
	}
</style>
