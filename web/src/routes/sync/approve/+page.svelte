<!--
	`datum.day/sync/approve#<record>` (da-6du3o): the link `sync invite`
	prints for a passkey to approve (`chain::approve_link`). It opens the
	Sync PLACE beside the tab the page lands on, and the page is `/` again.
	Reading the record after the `#` and approving it is da-bjxts's.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { withDb } from '$lib/database.js';
	import { openPlace } from '$lib/layout/shell.js';

	onMount(async () => {
		await goto(withDb('/'), { replaceState: true });
		await openPlace('sync');
	});
</script>
