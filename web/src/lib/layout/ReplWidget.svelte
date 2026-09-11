<!--
	The REPL as a widget kind: the REPL itself, wired to the engine.

	Whichever pane it sits in, it announces itself to `engine.ts` so a
	widget-bar press finds it (`press`), and its `ran` is the engine's
	`ran` — the one beat every pane refreshes on.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import Repl from '$lib/Repl.svelte';
	import { ran, registerRepl } from './engine.js';

	let repl: Repl;
	let unregister: (() => void) | undefined;

	$: if (repl && !unregister) unregister = registerRepl(repl);
	onDestroy(() => unregister?.());
</script>

<Repl bind:this={repl} on:ran={() => void ran()} />
