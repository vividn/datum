<!--
	`datum.day/<name>` (da-0a5o): a saved layout, a built-in one, or a
	widget kind — opened as a root tab (added if absent, brought to the
	front) and then the page is `/` again, so the URL is the way IN and the
	root bar is where you are. A reload lands on the same tab: which tabs
	are open, and which is active, is this device's ($lib/layout/draft.ts).

	`/now`, `/stats` and `/input` still work: they are the layouts of those
	names. A name that is none of these is a quiet notice with a link home.

	One catch-all route, not a route per name, so the deployed site — where
	every path is served by adapter-static's fallback (svelte.config.js) —
	answers any name without a file per name.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { open } from '$lib/layout/shell.js';

	let unknown: string | null = null;

	onMount(async () => {
		const name = $page.params.name ?? '';
		if (await open(name)) await goto('/', { replaceState: true });
		else unknown = name;
	});
</script>

{#if unknown !== null}
	<p class="notice">
		nothing is called “{unknown}” — no layout, no widget. <a href="/">home</a>
	</p>
{/if}

<style>
	.notice {
		margin: 1rem 0;
		color: #444466;
		font-style: italic;
	}

	.notice a {
		color: #7c7cff;
	}
</style>
