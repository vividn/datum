<!--
	`datum.day/sync/approve#<record>` (da-6du3o): the link `sync invite`
	prints for a passkey to approve (`chain::approve_link`). It opens the
	Sync PLACE beside the tab the page lands on, and the page is `/` again.
	The record rides in the `#`, which the `goto` drops: it is read first,
	and the Sync widget draws its approval card (da-bjxts).
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { withDb } from '#lib/database.ts';
	import { openPlace } from '#lib/layout/shell.ts';
	import { approving } from '#lib/syncView.ts';

	onMount(async () => {
		if (location.hash.length > 1) approving.set(`https://datum.day/sync/approve${location.hash}`);
		await goto(withDb('/'), { replace: true });
		await openPlace('sync');
	});
</script>
