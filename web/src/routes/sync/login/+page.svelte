<!--
	`datum.day/sync/login#<login>` (da-mzqs4): the link `datum sync login`
	prints. As `/sync/join` does, it opens the Sync PLACE and the page is `/`
	again; the login rides in the `#`, which no server sees, and the Sync
	widget asks before it signs the CLI in (`loggingIn`).
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { withDb } from '#lib/database.ts';
	import { openPlace } from '#lib/layout/shell.ts';
	import { loggingIn } from '#lib/syncView.ts';

	onMount(async () => {
		if (location.hash.length > 1) loggingIn.set(location.hash.slice(1));
		await goto(withDb(resolve('/')), { replace: true });
		await openPlace('sync');
	});
</script>
