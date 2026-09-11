<!--
	A quiet notice where a widget would be: what a tab shows when the layout
	it names cannot be inlined — it is already open above it, or nothing is
	saved under that name (`$lib/layout/shell.ts`).
-->
<script lang="ts">
	import Pane from '$lib/Pane.svelte';

	export let settings: Record<string, unknown> = {};
	$: text = typeof settings.text === 'string' ? settings.text : '';
</script>

<Pane title="notice">
	<p class="quiet">{text}</p>
</Pane>

<style>
	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}
</style>
