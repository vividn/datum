<!--
	`datum now --bare`: each tracked field's current state, as the CLI lists it.

	This page used to be a dashboard of its own, read out of the browser's own
	SQL and formatted here — one card per field, with a value slot the page
	filled by its own rules (da-sgvh). With the CLI as the only runner
	(da-g8o8) it shows what `datum now` says, the terminal's answer to the
	same question: a point has no state, so it is not listed, and a stopped
	block is not a current state either.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import CliOutput from '$lib/CliOutput.svelte';
	import { initDb } from '$lib/db.js';
	import { run, type RunResult } from '$lib/run.js';

	let result: RunResult | null = null;

	onMount(async () => {
		await initDb();
		result = await run(['now', '--bare']);
	});
</script>

<div class="now-view">
	<h2>now</h2>

	{#if !result}
		<p class="quiet">loading...</p>
	{:else if result.stdout}
		<CliOutput text={result.stdout} />
	{:else if result.stderr}
		<CliOutput text={result.stderr} error={result.code !== 0} />
	{:else}
		<p class="quiet">No field has a state yet. Start logging from the <a href="/">main page</a>.</p>
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

	.quiet {
		color: #444466;
		font-family: monospace;
	}

	.quiet a {
		color: #7c7cff;
	}
</style>
