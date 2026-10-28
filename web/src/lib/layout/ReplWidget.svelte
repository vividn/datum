<!--
	The REPL as a widget kind: the REPL itself, wired to the engine.

	Whichever pane it sits in, it announces itself to `engine.ts` so a
	widget-bar press finds it (`press`), and its `ran` is the engine's
	`ran` — the one beat every pane refreshes on. `reversed` is its option
	(registry.ts, da-gvdc).
-->
<script lang="ts">
	import Repl from '#lib/Repl.svelte';
	import { ran, registerRepl } from './engine.js';

	let { reversed = false }: { reversed?: boolean } = $props();

	let repl = $state<Repl>();

	$effect(() => {
		if (repl) return registerRepl(repl);
	});
</script>

<Repl bind:this={repl} {reversed} onran={(result) => void ran(result)} />
