<!--
	`datum.day/sync/join#<ticket>` (da-l9kdr): the join link an invite is
	shown as, and its QR holds. Like `/sync/approve`, it opens the Sync PLACE
	and the page is `/` again, keeping the `?db=` it was opened with — the
	database it joins. The ticket rides in the `#`, which no server sees and
	the `goto` drops from the history: it is read first, and the Sync widget
	asks before it joins (`joining`).
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { withDb } from '$lib/database.js';
	import { openPlace } from '$lib/layout/shell.js';
	import { joining } from '$lib/syncView.js';

	onMount(async () => {
		if (location.hash.length > 1) joining.set(location.hash.slice(1));
		await goto(withDb('/'), { replaceState: true });
		await openPlace('sync');
	});
</script>
