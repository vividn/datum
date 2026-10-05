<!--
	The REPL as a widget kind: the REPL itself, wired to the engine.

	Whichever pane it sits in, it announces itself to `engine.ts` so a
	widget-bar press finds it (`press`), and its `ran` is the engine's
	`ran` — the one beat every pane refreshes on. `reversed` is its option
	(registry.ts, da-gvdc).
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import Repl from '$lib/Repl.svelte';
	import { ran, registerRepl } from './engine.js';

	export let reversed = false;

	let repl: Repl;
	let unregister: (() => void) | undefined;

	$: if (repl && !unregister) unregister = registerRepl(repl);
	onDestroy(() => unregister?.());
</script>

<Repl bind:this={repl} {reversed} on:ran={(e) => void ran(e.detail)} />
