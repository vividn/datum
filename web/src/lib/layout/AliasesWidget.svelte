<!--
	The aliases widget (da-x6yj): every alias `datum alias list --json`
	lists, read again on every epoch — the user's own first, then the ones
	a spec derives, grouped under their field and written in its colour
	where the spec has one (the fields widget's colour, da-c7u8). Grouping
	and reordering of the reader's own are not here: that idea is still
	forming.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { browser } from '$app/env';
	import Pane from '$lib/Pane.svelte';
	import { run } from '$lib/run.js';
	import { aliasesArgv } from '$lib/calls.js';
	import type { Alias } from '$lib/commands.js';
	import { fields } from './engine.js';

	let { epoch = 0 }: { epoch?: number } = $props();

	let listed = $state.raw<Alias[] | null>(null);
	/** Listings started and the newest one on screen: last started wins (see CliPane.svelte). */
	let started = 0;
	let shown = -1;

	$effect.pre(() => {
		const at = epoch;
		if (browser) untrack(() => void list(at));
	});

	async function list(_epoch: number): Promise<void> {
		const at = ++started;
		const result = await run(aliasesArgv());
		if (at < shown || result.code !== 0) return;
		shown = at;
		try {
			listed = JSON.parse(result.stdout) as Alias[];
		} catch {
			// Not the JSON asked for (a process killed mid-write): the list stays as it was.
		}
	}

	const user = $derived((listed ?? []).filter((a) => a.source === 'user'));
	const bySpec = $derived(
		[
			...(listed ?? [])
				.filter((a) => a.source === 'derived')
				.reduce((m, a) => m.set(a.field ?? '', [...(m.get(a.field ?? '') ?? []), a]), new Map<string, Alias[]>())
		].sort(([a], [b]) => a.localeCompare(b))
	);
	const colors = $derived(new Map($fields.map((w) => [w.field, w.color])));
</script>

<Pane title="aliases">
	{#if !listed}
		<p class="quiet">…</p>
	{:else if listed.length === 0}
		<p class="quiet">no aliases yet — alias set &lt;name&gt; "&lt;command&gt;"</p>
	{/if}
	{#if user.length}
		<ul>
			{#each user as a (a.name)}<li><b>{a.name}</b> → {a.expansion}</li>{/each}
		</ul>
	{/if}
	{#each bySpec as [field, aliases] (field)}
		<section style:color={colors.get(field)}>
			<h3>{field}</h3>
			<ul>
				{#each aliases as a (a.name)}<li><b>{a.name}</b> → {a.expansion}</li>{/each}
			</ul>
		</section>
	{/each}
</Pane>

<style>
	ul {
		margin: 0 0 0.6rem;
		padding: 0;
		list-style: none;
		font-family: monospace;
	}
	h3 {
		margin: 0;
		font-size: 0.8rem;
		font-weight: normal;
		opacity: 0.7;
	}
	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
</style>
