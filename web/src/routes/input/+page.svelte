<!--
	The input widget on a page of its own (da-0dhc): where it can be driven
	until the dashboard engine registers it as a widget kind, and its
	per-widget URL after that.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import Input from '$lib/Input.svelte';
	import { initDb } from '$lib/db.js';

	let ready = false;

	onMount(async () => {
		await initDb();
		ready = true;
	});
</script>

<div class="page"><Input disabled={!ready} /></div>

<style>
	.page {
		display: grid;
		height: 100%;
	}
</style>
