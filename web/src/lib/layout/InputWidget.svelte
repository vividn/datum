<!--
	The input widget as a widget kind (da-0dhc): the prompt with no output
	pane, wired to the engine — its `ran` is the one beat every pane
	refreshes on, and what it records lands in the bottom bar's receipt.
-->
<script lang="ts">
	import Input from '$lib/Input.svelte';
	import { ran, ready } from './engine.js';
</script>

<Input disabled={!$ready} on:ran={() => void ran()} />
