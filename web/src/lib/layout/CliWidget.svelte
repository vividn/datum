<!--
	The CLI as a widget kind: its scrollback, wired to the engine.

	Whichever pane it sits in, it announces itself to `engine.ts` so a
	widget-bar press finds it (`press`), and its `ran` is the engine's
	`ran` — the one beat every pane refreshes on. `reversed` is its option
	(registry.ts, da-gvdc).
-->
<script lang="ts">
	import CliScrollback from '#lib/CliScrollback.svelte';
	import { ran, registerCli } from './engine.js';

	let { reversed = false }: { reversed?: boolean } = $props();

	let scrollback = $state<CliScrollback>();

	$effect(() => {
		if (scrollback) return registerCli(scrollback);
	});
</script>

<CliScrollback bind:this={scrollback} {reversed} onran={(result) => void ran(result)} />
